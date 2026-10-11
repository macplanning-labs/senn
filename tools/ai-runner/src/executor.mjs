// executor.mjs — 1 件の依頼を実行する(リポジトリの用意 → サンドボックスで AI → 点検 → コミット → push)。
// 方針: AI は作業ディレクトリの中の編集だけ。コミット・push は、AI の外(ここ)で行う。
//  - .git の本体は作業ディレクトリの外に置く(AI が .git/config やフックを書き換えても、ここの git には効かない)。
//  - ホストの git は、フック・fsmonitor を無効にし、設定ファイルも読まない。
import { spawn, execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { buildDockerArgs, assertSafeArgs, containerName, gatewayName, GATEWAY_PORT } from './sandbox.mjs';
import { UsageMeter } from './meter.mjs';
import { scanDiff, protectedPaths, redact } from './secrets.mjs';
import { workBranch } from './verify.mjs';

const execFileP = promisify(execFile);
const HOOKS_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'hooks');
const GATEWAY_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'gateway');
const CONFIG_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'config');

/**
 * サンドボックスのネットワークを確かめる(ランナーの起動時)。
 * - 外に出られない内部ネットワークであること(bridge などだと、AI が LAN や外部に直接つながれる)
 * - 中継役と AI のコンテナ以外が、つながっていないこと(以前の版のプロキシなどが残っていると、
 *   中継役を通らずに外へ出られる)
 * - 中継役が外へ出るためのネットワークがあること
 */
export async function assertIsolatedNetwork(config, execImpl = execFileP) {
  const name = config.sandboxNetwork;
  const setup = 'tools/ai-runner/docker/setup-networks.sh を実行してください';
  let out = '';
  try {
    ({ stdout: out } = await execImpl(config.dockerBin, ['network', 'inspect', '--format', '{{.Internal}}|{{range .Containers}}{{.Name}} {{end}}', name]));
  } catch {
    throw new Error(`サンドボックスのネットワーク ${name} が見つかりません。${setup}`);
  }
  const [internal, members = ''] = String(out).trim().split('|');
  if (internal !== 'true') {
    throw new Error(`サンドボックスのネットワーク ${name} は、外に出られる設定です(internal: true ではありません)。${setup}`);
  }
  const others = members.split(/\s+/).filter((n) => n && !/^senn-ai-(gw-)?[0-9a-f-]{8}$/.test(n));
  if (others.length) {
    throw new Error(`内部ネットワーク ${name} に、中継役と AI のコンテナ以外がつながっています(${others.join(', ')})。中継役を通らずに外へ出られるため起動しません。以前の版のプロキシなら docker rm -f senn-ai-proxy で取り除いてください`);
  }
  try {
    await execImpl(config.dockerBin, ['network', 'inspect', '--format', '{{.Name}}', config.egressNetwork]);
  } catch {
    throw new Error(`中継役が使うネットワーク ${config.egressNetwork} が見つかりません。${setup}`);
  }
}

function gitEnv(config) {
  const askpass = path.join(config.home, 'askpass.sh');
  return {
    PATH: process.env.PATH,
    HOME: config.home,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_TERMINAL_PROMPT: '0',
    GIT_ASKPASS: askpass,
    SENN_RUNNER_GIT_TOKEN: config.githubToken,
  };
}

const SAFE_GIT = ['-c', 'core.hooksPath=/dev/null', '-c', 'core.fsmonitor=false', '-c', 'protocol.ext.allow=never', '-c', 'protocol.file.allow=never'];

async function git(config, gitDir, workTree, args, opts = {}) {
  const base = gitDir ? [`--git-dir=${gitDir}`, `--work-tree=${workTree}`] : [];
  const { stdout } = await execFileP(config.gitBin, [...SAFE_GIT, ...base, ...args], {
    env: gitEnv(config), maxBuffer: 64 * 1024 * 1024, cwd: workTree ?? config.home, ...opts,
  });
  return stdout;
}

function ensureAskpass(config) {
  fs.mkdirSync(config.home, { recursive: true, mode: 0o700 });
  const p = path.join(config.home, 'askpass.sh');
  fs.writeFileSync(p, '#!/bin/sh\nprintf "%s" "$SENN_RUNNER_GIT_TOKEN"\n', { mode: 0o700 });
}

/** リポジトリを用意して、作業ブランチを切る。 */
export async function prepareWorkspace(config, job, dir) {
  ensureAskpass(config);
  const workspace = path.join(dir, 'ws');
  const gitDir = path.join(dir, 'gitdir');
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const url = `https://github.com/${job.repo}.git`;
  await execFileP(config.gitBin, [...SAFE_GIT, 'clone', '--quiet', '--single-branch', '--branch', job.baseBranch, '--separate-git-dir', gitDir, url, workspace], {
    env: gitEnv(config), maxBuffer: 16 * 1024 * 1024,
  });
  // AI からは、.git(gitdir を指すファイル)を見せない
  fs.rmSync(path.join(workspace, '.git'), { force: true, recursive: true });
  await git(config, gitDir, workspace, ['checkout', '--quiet', '-b', workBranch(job)]);
  return { workspace, gitDir };
}

/** 作業後の点検・コミット・(安全モードでなければ)push。結果を返す。 */
export async function finalizeWork(config, job, { workspace, gitDir }, log) {
  await git(config, gitDir, workspace, ['add', '-A']);
  const names = (await git(config, gitDir, workspace, ['diff', '--cached', '--name-only', '-z'])).split('\0').filter(Boolean);
  if (names.length === 0) return { changed: false, pushed: false, note: '変更はありませんでした' };

  const prot = protectedPaths(names);
  if (prot.length) return { changed: true, pushed: false, blocked: `保護された場所の変更があるため、保存しません: ${prot.slice(0, 5).join(', ')}` };

  const diff = await git(config, gitDir, workspace, ['diff', '--cached', '--no-color', '-U0']);
  const findings = scanDiff(diff, names, [config.oauth, config.githubToken, config.runnerToken, config.signingSecret, config.cfAccess?.secret]);
  if (findings.length) {
    const where = [...new Set(findings.map((f) => `${f.kind}(${f.where})`))].slice(0, 5).join(', ');
    return { changed: true, pushed: false, blocked: `秘密情報の疑いがあるため、保存しません: ${where}` };
  }
  const gl = await runGitleaks(config, workspace, gitDir, log);
  if (gl) return { changed: true, pushed: false, blocked: gl };

  await git(config, gitDir, workspace, ['-c', 'user.name=SENN AI Runner', '-c', 'user.email=ai-runner@senn.invalid', 'commit', '--quiet', '-m', `${job.ticketKey}: AI による対応`]);
  if (config.safeMode || !config.githubToken) {
    return { changed: true, pushed: false, note: config.safeMode ? '安全モードのため、push していません(作業ディレクトリのコミットのみ)' : 'GITHUB_TOKEN が無いため push していません' };
  }
  await git(config, gitDir, workspace, ['push', '--quiet', 'origin', `${workBranch(job)}:${workBranch(job)}`]);
  return { changed: true, pushed: true, branch: workBranch(job) };
}

async function runGitleaks(config, workspace, gitDir, log) {
  try {
    await execFileP('gitleaks', ['--version']);
  } catch { return null; } // 入っていなければ、組み込みの検査だけ
  try {
    // 設定はランナーが持つものを使う(作業ディレクトリの .gitleaks.toml / .gitleaksignore は AI が書き換えられる)。
    // 環境変数も、ランナーの秘密を含む process.env ではなく、git 用の最小限のものにする
    const { SENN_RUNNER_GIT_TOKEN: _omit, ...env } = gitEnv(config);
    await execFileP('gitleaks', ['protect', '--staged', '--no-banner', '--source', workspace,
      '--config', path.join(CONFIG_DIR, 'gitleaks.toml'), '--gitleaks-ignore-path', CONFIG_DIR], { env: { ...env, GIT_DIR: gitDir, GIT_WORK_TREE: workspace } });
    return null;
  } catch (e) {
    log(`gitleaks の指摘: ${String(e.stdout ?? e.message).slice(0, 300)}`);
    return 'gitleaks が秘密情報の疑いを検出したため、保存しません';
  }
}

/**
 * 中継役(本物のトークンを持ち、Anthropic にだけ送る)のコンテナの引数。
 * 外に出られるネットワーク(egress)で動かし、起動後に AI のコンテナと同じ内部ネットワークにもつなぐ。
 * トークンは値をコマンドラインに載せず、環境から渡す(-e CLAUDE_CODE_OAUTH_TOKEN)。
 */
export function buildGatewayArgs({ config, job, gatewayDir = GATEWAY_DIR, user }) {
  if (/[:,\s]/.test(gatewayDir)) throw new Error('中継役のディレクトリのパスに、空白・コロン・カンマは使えません');
  return [
    'run', '-d', '--rm',
    '--name', gatewayName(job.runUuid),
    '--read-only',
    '--cap-drop=ALL',
    '--security-opt', 'no-new-privileges',
    '--pids-limit', '64',
    '--memory', '256m',
    '--user', `${user.uid}:${user.gid}`,
    '--network', config.egressNetwork,
    '-v', `${gatewayDir}:/opt/gateway:ro`,
    '-e', 'CLAUDE_CODE_OAUTH_TOKEN',
    '-e', `GATEWAY_PORT=${GATEWAY_PORT}`,
    '--entrypoint', 'node',
    config.image,
    '/opt/gateway/gateway.mjs',
  ];
}

/** 中継役を起動し、内部ネットワークにつなぎ、受け付けを始めるまで待つ。 */
export async function startGateway(config, job, { execImpl = execFileP, sleep = (ms) => new Promise((r) => setTimeout(r, ms)), tries = 40 } = {}) {
  const name = gatewayName(job.runUuid);
  const env = { PATH: process.env.PATH, HOME: process.env.HOME, DOCKER_HOST: process.env.DOCKER_HOST ?? '', CLAUDE_CODE_OAUTH_TOKEN: config.oauth };
  const args = buildGatewayArgs({ config, job, user: { uid: process.getuid?.() ?? 1000, gid: process.getgid?.() ?? 1000 } });
  await execImpl(config.dockerBin, args, { env });
  try {
    await execImpl(config.dockerBin, ['network', 'connect', config.sandboxNetwork, name], { env: { PATH: process.env.PATH, HOME: process.env.HOME, DOCKER_HOST: process.env.DOCKER_HOST ?? '' } });
    for (let i = 0; i < tries; i++) {
      const { stdout = '', stderr = '' } = await execImpl(config.dockerBin, ['logs', name], { env: { PATH: process.env.PATH, HOME: process.env.HOME, DOCKER_HOST: process.env.DOCKER_HOST ?? '' } });
      if (`${stdout}${stderr}`.includes('中継役を開始しました')) return name;
      await sleep(250);
    }
    throw new Error('中継役が起動しませんでした');
  } catch (e) {
    await stopGateway(config, job, { execImpl });
    throw e;
  }
}

/** 中継役を止める(失敗しても実行の結果には影響させない)。 */
export async function stopGateway(config, job, { execImpl = execFileP } = {}) {
  try {
    await execImpl(config.dockerBin, ['rm', '-f', gatewayName(job.runUuid)], { env: { PATH: process.env.PATH, HOME: process.env.HOME, DOCKER_HOST: process.env.DOCKER_HOST ?? '' } });
  } catch { /* すでに止まっている */ }
}

/**
 * claude をサンドボックスで動かす。
 * @param {{ shouldStop: () => string|null, meter?: UsageMeter }} opts shouldStop は 1 秒ごとに呼ばれ、止める理由を返す
 */
export async function runSandbox(config, job, workspace, opts) {
  const { gatewayImpl = { start: startGateway, stop: stopGateway } } = opts;
  try {
    await gatewayImpl.start(config, job);
  } catch (e) {
    return { code: -1, killedBy: null, meter: opts.meter ?? new UsageMeter(), stderr: `中継役の起動に失敗: ${e.message}` };
  }
  try {
    return await runContainer(config, job, workspace, opts);
  } finally {
    await gatewayImpl.stop(config, job);
  }
}

function runContainer(config, job, workspace, { shouldStop, meter = new UsageMeter(), spawnImpl = spawn, tickMs = 1000 }) {
  const args = assertSafeArgs(buildDockerArgs({
    config, job, workspace, hooksDir: HOOKS_DIR,
    user: { uid: process.getuid?.() ?? 1000, gid: process.getgid?.() ?? 1000 },
  }));
  const startedAt = Date.now();
  const name = containerName(job.runUuid);
  return new Promise((resolve) => {
    const child = spawnImpl(config.dockerBin, args, {
      // 本物のトークンは、AI のコンテナには渡さない(中継役だけが持つ)
      env: { PATH: process.env.PATH, HOME: process.env.HOME, DOCKER_HOST: process.env.DOCKER_HOST ?? '' },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let buf = '';
    let stderr = '';
    let killedBy = null;
    const kill = (why) => {
      if (killedBy) return;
      killedBy = why;
      // コンテナごと確実に止める(docker run の子プロセスの kill だけでは止まらないことがある)
      spawnImpl(config.dockerBin, ['kill', name], { stdio: 'ignore' }).on('error', () => {});
      setTimeout(() => child.kill('SIGKILL'), 5000).unref?.();
    };
    child.stdin.on('error', () => {});
    child.stdin.end(job.prompt); // プロンプトは標準入力で渡す(プロセス一覧に出さない)
    child.stdout.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) { meter.feed(buf.slice(0, i)); buf = buf.slice(i + 1); }
      if (buf.length > 5_000_000) buf = ''; // 改行の無い巨大な出力は捨てる
    });
    child.stderr.on('data', (d) => { stderr = (stderr + d).slice(-4000); });
    const timer = setInterval(() => {
      const elapsedMin = (Date.now() - startedAt) / 60000;
      if (elapsedMin >= job.caps.maxMinutes) return kill('per_run_time');
      if (meter.weighted() >= job.caps.maxWeightedTokens) return kill('per_run_tokens');
      if (meter.turns > job.caps.maxTurns + 5) return kill('per_run_turns');
      const why = shouldStop();
      if (why) kill(why);
    }, tickMs);
    timer.unref?.();
    child.on('error', (e) => { clearInterval(timer); resolve({ code: -1, killedBy, meter, stderr: `起動に失敗: ${e.message}` }); });
    child.on('close', (code) => {
      clearInterval(timer);
      if (buf) meter.feed(buf);
      resolve({ code, killedBy, meter, stderr: redact(stderr, [config.oauth, config.githubToken, config.runnerToken, config.cfAccess?.secret]) });
    });
  });
}
