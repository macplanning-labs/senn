// config.mjs — 環境変数から設定を作る。危険な設定は、ここで起動を拒否する。
import path from 'node:path';
import os from 'node:os';

/** 絶対上限の既定値。SENN から来た上限と比べて、小さいほうを使う。 */
export const DEFAULT_CEILINGS = Object.freeze({
  maxTurns: 400,
  maxMinutes: 180,
  maxWeightedTokens: 30_000_000,
});

const REPO_RE = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

function int(env, name, def, { min = 1, max = Number.MAX_SAFE_INTEGER } = {}) {
  const raw = env[name];
  if (raw === undefined || raw === '') return def;
  const n = Number(raw);
  if (!Number.isInteger(n) || n < min || n > max) {
    throw new Error(`${name} は ${min}〜${max} の整数で指定してください(指定値: ${raw})`);
  }
  return n;
}

/** "9-21" → {start:9,end:21}。未指定なら null(常時)。 */
export function parseActiveHours(raw) {
  if (!raw) return null;
  const m = /^(\d{1,2})-(\d{1,2})$/.exec(raw.trim());
  if (!m) throw new Error('ACTIVE_HOURS は "9-21" の形式で指定してください');
  const start = Number(m[1]);
  const end = Number(m[2]);
  if (start > 24 || end > 24 || start === end) throw new Error('ACTIVE_HOURS の時刻が不正です');
  return { start, end };
}

/** 現在(ローカル時刻)が実行してよい時間帯か。start > end は日またぎ(例 22-6)。 */
export function isWithinActiveHours(hours, date = new Date()) {
  if (!hours) return true;
  const h = date.getHours();
  return hours.start < hours.end ? h >= hours.start && h < hours.end : h >= hours.start || h < hours.end;
}

export function parseConfig(env = process.env) {
  const errors = [];
  const need = (name) => {
    const v = env[name];
    if (!v) errors.push(`${name} が未設定です`);
    return v ?? '';
  };

  const senn = need('SENN_URL').replace(/\/+$/, '');
  const runnerToken = need('RUNNER_TOKEN');
  const signingSecret = need('RUNNER_SIGNING_SECRET');
  const oauth = need('CLAUDE_CODE_OAUTH_TOKEN');
  const allowedRepos = (env.ALLOWED_REPOS ?? '').split(',').map((s) => s.trim()).filter(Boolean);
  if (allowedRepos.length === 0) errors.push('ALLOWED_REPOS(許可するリポジトリ。owner/name をカンマ区切り)が未設定です');
  for (const r of allowedRepos) if (!REPO_RE.test(r)) errors.push(`ALLOWED_REPOS に不正な値があります: ${r}`);

  // 通信は HTTPS のみ。ローカル開発(localhost)だけ、明示した場合に http を許す。
  if (senn) {
    let u;
    try { u = new URL(senn); } catch { errors.push('SENN_URL が URL として不正です'); }
    if (u && u.protocol !== 'https:') {
      const local = ['localhost', '127.0.0.1', '[::1]'].includes(u.hostname);
      if (!(u.protocol === 'http:' && local && env.ALLOW_INSECURE_LOCALHOST === '1')) {
        errors.push('SENN_URL は https:// で指定してください(localhost の試験だけ ALLOW_INSECURE_LOCALHOST=1 で http を許可)');
      }
    }
  }
  if (runnerToken && !runnerToken.startsWith('srun_')) errors.push('RUNNER_TOKEN の形式が違います(srun_ で始まります)');

  // 従量課金の API キーは使わない(サブスクリプションの枠だけで動かす)。
  if (env.ANTHROPIC_API_KEY && env.ALLOW_API_KEY !== '1') {
    errors.push('ANTHROPIC_API_KEY が設定されています。従量課金を避けるため、起動を拒否します(設定を外してください)');
  }
  if (env.ANTHROPIC_AUTH_TOKEN || env.ANTHROPIC_BASE_URL) {
    if (env.ALLOW_API_KEY !== '1') errors.push('ANTHROPIC_AUTH_TOKEN / ANTHROPIC_BASE_URL が設定されています。起動を拒否します');
  }

  // Cloudflare Access のサービストークン(SENN が Access で守られているときだけ。2 つそろえて指定する)
  const cfId = env.CF_ACCESS_CLIENT_ID ?? '';
  const cfSecret = env.CF_ACCESS_CLIENT_SECRET ?? '';
  if (Boolean(cfId) !== Boolean(cfSecret)) errors.push('CF_ACCESS_CLIENT_ID と CF_ACCESS_CLIENT_SECRET は、2 つそろえて指定してください');

  let activeHours = null;
  try { activeHours = parseActiveHours(env.ACTIVE_HOURS); } catch (e) { errors.push(e.message); }

  const ceilings = {
    maxTurns: int(env, 'CEILING_MAX_TURNS', DEFAULT_CEILINGS.maxTurns),
    maxMinutes: int(env, 'CEILING_MAX_MINUTES', DEFAULT_CEILINGS.maxMinutes),
    maxWeightedTokens: int(env, 'CEILING_MAX_WEIGHTED_TOKENS', DEFAULT_CEILINGS.maxWeightedTokens),
  };

  if (errors.length) {
    const err = new Error(`設定エラー:\n - ${errors.join('\n - ')}`);
    err.configErrors = errors;
    throw err;
  }

  const home = env.RUNNER_HOME || path.join(os.homedir(), '.senn-ai-runner');
  return Object.freeze({
    senn, runnerToken, signingSecret, oauth, cfAccess: cfId ? { id: cfId, secret: cfSecret } : null, allowedRepos, activeHours, ceilings,
    home,
    workRoot: path.join(home, 'work'),
    stopFile: env.STOP_FILE || path.join(home, 'STOP'),
    // 安全モード(既定 ON): ブランチの push をしない。ローカルのコミットまでにとどめる。
    safeMode: env.SAFE_MODE !== '0',
    githubToken: env.GITHUB_TOKEN || '',
    image: env.SANDBOX_IMAGE || 'senn-ai-sandbox:latest',
    sandboxNetwork: env.SANDBOX_NETWORK || 'senn-ai-internal',
    // 中継役が Anthropic に出ていくためのネットワーク(外に出られるもの)
    egressNetwork: env.SANDBOX_EGRESS_NETWORK || 'senn-ai-egress',
    model: env.CLAUDE_MODEL || '',
    pollMs: int(env, 'POLL_SECONDS', 10, { min: 2, max: 300 }) * 1000,
    heartbeatMs: int(env, 'HEARTBEAT_SECONDS', 30, { min: 5, max: 60 }) * 1000,
    memory: env.SANDBOX_MEMORY || '4g',
    cpus: env.SANDBOX_CPUS || '2',
    pids: int(env, 'SANDBOX_PIDS', 512),
    dockerBin: env.DOCKER_BIN || 'docker',
    gitBin: env.GIT_BIN || 'git',
  });
}
