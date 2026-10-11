// secrets.mjs — push の前に、差分に秘密情報が混ざっていないかを調べる(AI の外側で行う)。
// 完全ではない(検出漏れはありうる)。gitleaks が入っていれば、あわせて使う(executor.mjs)。

const PATTERNS = [
  ['Anthropic のキー/トークン', /sk-ant-[A-Za-z0-9_-]{16,}/],
  ['SENN のトークン', /\bsrun_[A-Za-z0-9_-]{20,}/],
  ['AWS のアクセスキー', /\b(AKIA|ASIA)[0-9A-Z]{16}\b/],
  ['GitHub のトークン', /\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}\b|github_pat_[A-Za-z0-9_]{30,}/],
  ['Slack のトークン', /\bxox[abprs]-[A-Za-z0-9-]{10,}/],
  ['秘密鍵', /-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----/],
  ['Google の API キー', /\bAIza[0-9A-Za-z_-]{35}\b/],
  ['Stripe の秘密鍵', /\b[sr]k_live_[0-9A-Za-z]{16,}/],
  ['認証情報つき URL', /https?:\/\/[^\s/:@]+:[^\s/@]{6,}@[^\s/]+/],
  ['パスワード/秘密の代入', /(?:password|passwd|secret|api[_-]?key|token)\s*[:=]\s*['"][^'"\s]{16,}['"]/i],
];

const SENSITIVE_FILE_RE = /(^|\/)(\.env(\..*)?|id_rsa|id_ed25519|.*\.pem|.*\.p12|.*\.pfx|credentials(\.json)?|\.npmrc|\.netrc)$/;
const EXAMPLE_FILE_RE = /\.(example|sample|template)$/;

/**
 * 既知の秘密(ランナー自身のトークン類)の、そのままの形と、よくある符号化(base64・base64url・16進)。
 * AI がトークンを符号化して書き出すと、形式で探す PATTERNS では見つからないため、値そのもので探す。
 * base64 は、前に何バイトあるかで文字の区切りがずれるので、3 通りのずれについて「必ず現れる中心部分」を使う。
 */
export function knownSecretForms(secrets = []) {
  const forms = new Set();
  for (const s of secrets) {
    if (!s || s.length < 8) continue;
    forms.add(s);
    const buf = Buffer.from(s, 'utf8');
    forms.add(buf.toString('hex'));
    for (let shift = 0; shift < 3; shift++) {
      for (const enc of ['base64', 'base64url']) {
        const b = Buffer.concat([Buffer.alloc(shift), buf]).toString(enc).replace(/=+$/, '');
        const core = b.slice(shift === 0 ? 0 : 4, -4);
        if (core.length >= 12) forms.add(core);
      }
    }
  }
  return [...forms];
}

/** 文字列に、既知の秘密(または符号化した形)が含まれるか。 */
export function containsKnownSecret(text, secrets = []) {
  const t = String(text);
  return knownSecretForms(secrets).some((f) => t.includes(f));
}

/** 追加された行(差分の + 行)と、変更されたファイル名を調べる。knownSecrets は、値そのもので探す秘密。 */
export function scanDiff(diffText, changedFiles = [], knownSecrets = []) {
  const findings = [];
  for (const f of changedFiles) {
    if (SENSITIVE_FILE_RE.test(f) && !EXAMPLE_FILE_RE.test(f)) findings.push({ kind: '秘密を含みうるファイル', where: f });
  }
  let file = '';
  for (const line of diffText.split('\n')) {
    if (line.startsWith('+++ ')) { file = line.slice(4).replace(/^b\//, ''); continue; }
    if (!line.startsWith('+')) continue;
    for (const [kind, re] of PATTERNS) {
      if (re.test(line)) findings.push({ kind, where: file });
    }
  }
  if (containsKnownSecret(diffText, knownSecrets)) findings.push({ kind: 'ランナーの秘密(符号化を含む)', where: '差分' });
  return findings;
}

/**
 * 変更してはいけない場所。ここを変えると、AI の変更が自動で実行される経路になる(CI・フック・エディタの自動実行)か、
 * AI への指示・検査の設定を書き換えられる(CLAUDE.md・AGENTS.md・gitleaks の設定・git の属性)。
 */
const PROTECTED_RE = /^(\.github\/|\.husky\/|\.gitlab-ci\.yml$|\.circleci\/|\.git\/|\.claude\/|CLAUDE\.md$|AGENTS\.md$|\.gitmodules$|\.gitattributes$|\.gitleaks\.toml$|\.gitleaksignore$|\.mcp\.json$|\.vscode\/|\.devcontainer\/|\.pre-commit-config\.ya?ml$|lefthook\.ya?ml$|\.lefthook\.ya?ml$)/;
export function protectedPaths(changedFiles) {
  return changedFiles.filter((f) => PROTECTED_RE.test(f));
}

/** ログ・報告に出す文字列から、既知の形式のトークンを伏せる。 */
export function redact(text, extraSecrets = []) {
  let out = String(text);
  for (const [, re] of PATTERNS.slice(0, 5)) out = out.replace(new RegExp(re.source, 'g' + (re.flags.includes('i') ? 'i' : '')), '[伏せ字]');
  for (const f of knownSecretForms(extraSecrets)) out = out.split(f).join('[伏せ字]');
  return out;
}
