// sandbox.mjs — AI(claude -p)を動かす Docker コンテナの引数を作る。
// 原則: 見せるのは「作業ディレクトリ 1 つ」だけ。docker.sock・ホストのネットワーク・特権は渡さない。
// 通信は、内部ネットワークから中継役(gateway/gateway.mjs)へだけ。本物の Claude のトークンは中継役だけが持つ。

/** AI に許すツール。push・ネットワーク取得系は許さない(コンテナにも認証情報がない)。 */
export const ALLOWED_TOOLS = ['Read', 'Edit', 'Write', 'Glob', 'Grep', 'Bash'];
export const DISALLOWED_TOOLS = [
  'WebFetch', 'WebSearch',
  'Bash(git push:*)', 'Bash(git remote:*)', 'Bash(git config:*)',
  'Bash(curl:*)', 'Bash(wget:*)', 'Bash(ssh:*)', 'Bash(scp:*)', 'Bash(nc:*)', 'Bash(sudo:*)',
];

/** claude に渡すフック設定(読み込みの上限を守らせる)。 */
export function claudeSettings() {
  return {
    hooks: {
      PreToolUse: [
        { matcher: 'Read', hooks: [{ type: 'command', command: 'node /opt/ai-runner/hooks/read-guard.mjs' }] },
      ],
    },
  };
}

export function containerName(runUuid) {
  return `senn-ai-${runUuid.slice(0, 8)}`;
}

/** 実行ごとの中継役のコンテナ名(内部ネットワークでの名前も同じ) */
export function gatewayName(runUuid) {
  return `senn-ai-gw-${runUuid.slice(0, 8)}`;
}

/**
 * AI のコンテナに渡す、偽のトークン。Claude Code は、これを使って中継役に送る。
 * 中継役が本物のトークンに差し替えるので、AI が読み出しても何にも使えない。
 */
export const PLACEHOLDER_TOKEN = 'sk-ant-oat01-senn-gateway-placeholder-not-a-real-token';
export const GATEWAY_PORT = 8080;

/**
 * @param {object} p
 * @param {object} p.config  parseConfig の結果
 * @param {object} p.job     verifyJob の結果(上限は丸め済み)
 * @param {string} p.workspace  ホスト側の作業ディレクトリ(絶対パス)
 * @param {string} p.hooksDir   ホスト側の hooks ディレクトリ(絶対パス)
 * @param {{uid:number,gid:number}} p.user
 */
export function buildDockerArgs({ config, job, workspace, hooksDir, user }) {
  if (!workspace.startsWith('/')) throw new Error('workspace は絶対パスで指定してください');
  if (/[:,\s]/.test(workspace) || /[:,\s]/.test(hooksDir)) throw new Error('作業ディレクトリのパスに、空白・コロン・カンマは使えません');
  const args = [
    'run', '--rm', '-i',
    '--name', containerName(job.runUuid),
    '--read-only',
    '--cap-drop=ALL',
    '--security-opt', 'no-new-privileges',
    '--pids-limit', String(config.pids),
    '--memory', config.memory,
    '--memory-swap', config.memory,
    '--cpus', String(config.cpus),
    '--user', `${user.uid}:${user.gid}`,
    '--tmpfs', '/tmp:rw,size=512m',
    '--tmpfs', '/home/sandbox:rw,mode=1777,size=1g',
    '--network', config.sandboxNetwork,
    '-v', `${workspace}:/workspace:rw`,
    '-v', `${hooksDir}:/opt/ai-runner/hooks:ro`,
    '-w', '/workspace',
    '-e', 'HOME=/home/sandbox',
    // 本物のトークンは渡さない。偽のトークンで中継役に送る(中継役が本物に差し替える)
    '-e', `CLAUDE_CODE_OAUTH_TOKEN=${PLACEHOLDER_TOKEN}`,
    '-e', `ANTHROPIC_BASE_URL=http://${gatewayName(job.runUuid)}:${GATEWAY_PORT}`,
    '-e', 'DISABLE_AUTOUPDATER=1',
    '-e', 'DISABLE_TELEMETRY=1',
    '-e', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1', // api.anthropic.com 以外へは通信しない(プロキシもそれ以外は拒否)
  ];
  args.push(
    config.image,
    'claude', '-p',
    '--output-format', 'stream-json', '--verbose',
    '--max-turns', String(job.caps.maxTurns),
    '--permission-mode', 'acceptEdits',
    '--allowedTools', ALLOWED_TOOLS.join(','),
    '--disallowedTools', DISALLOWED_TOOLS.join(','),
    '--settings', JSON.stringify(claudeSettings()),
  );
  if (config.model) args.push('--model', config.model);
  return args;
}

/** 安全のための自己点検: 組み立てた引数に、禁止されたものが入っていないか。 */
export function assertSafeArgs(args) {
  const bannedFlags = ['--privileged', '--pid', '--ipc', '--uts', '--userns', '--cgroupns', '--dangerously-skip-permissions', '--cap-add', '--device', '--volumes-from', '--security-opt=seccomp=unconfined'];
  for (const a of args) {
    for (const f of bannedFlags) if (a === f || a.startsWith(`${f}=`)) throw new Error(`禁止された引数が含まれています: ${f}`);
    if (a.includes('docker.sock')) throw new Error('禁止された引数が含まれています: docker.sock');
  }
  args.forEach((a, i) => {
    if ((a === '--network' || a === '--net') && args[i + 1] === 'host') throw new Error('禁止された引数が含まれています: --network host');
    if (a === '--network=host' || a === '--net=host') throw new Error('禁止された引数が含まれています: --network=host');
  });
  if (!args.includes('--read-only') || !args.includes('--cap-drop=ALL')) throw new Error('必須の隔離の引数がありません');
  const mounts = args.filter((a, i) => args[i - 1] === '-v');
  if (mounts.length !== 2 || !mounts[0].endsWith(':/workspace:rw') || !mounts[1].endsWith(':ro')) throw new Error('マウントは、作業ディレクトリと hooks(読み取り専用)の 2 つだけです');
  // トークンは偽のものだけ。本物のトークン(や、環境から引き継ぐ指定)は渡さない
  args.forEach((a, i) => {
    if (args[i - 1] !== '-e') return;
    if (a === 'CLAUDE_CODE_OAUTH_TOKEN' || a === 'ANTHROPIC_API_KEY' || a === 'ANTHROPIC_AUTH_TOKEN') throw new Error(`環境から引き継ぐ認証情報は渡せません: ${a}`);
    if (a.startsWith('CLAUDE_CODE_OAUTH_TOKEN=') && a !== `CLAUDE_CODE_OAUTH_TOKEN=${PLACEHOLDER_TOKEN}`) throw new Error('本物のトークンを、AI のコンテナに渡してはいけません');
    if (/^(ANTHROPIC_API_KEY|ANTHROPIC_AUTH_TOKEN)=/.test(a)) throw new Error('認証情報を、AI のコンテナに渡してはいけません');
    if (a.startsWith('ANTHROPIC_BASE_URL=') && !/^ANTHROPIC_BASE_URL=http:\/\/senn-ai-gw-[0-9a-f-]{8}:\d+$/.test(a)) throw new Error('送り先は、実行ごとの中継役だけです');
    if (/^(HTTPS?_PROXY|https?_proxy)=/.test(a)) throw new Error('AI のコンテナに、外へ出るプロキシを渡してはいけません');
  });
  if (!args.includes(`CLAUDE_CODE_OAUTH_TOKEN=${PLACEHOLDER_TOKEN}`)) throw new Error('偽のトークンの指定がありません');
  return args;
}
