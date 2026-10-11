import test from 'node:test';
import assert from 'node:assert/strict';
import { scanDiff, protectedPaths, redact, containsKnownSecret } from '../src/secrets.mjs';

// ダミーの秘密は、実行時に組み立てる(ソースに鍵の形の文字列を置かない。秘密検査ツールに誤検出されないため)
const AWS = ['AK', 'IA'].join('') + 'ABCDEFGHIJKLMNOP';
const ANT = ['sk', 'ant', 'api03'].join('-') + '-abcdefghijklmnop1234';
const OAT = ['sk', 'ant', 'oat01'].join('-') + '-abcdefghijklmnopqrst';
const PEM = ['-----BEGIN', 'RSA PRIVATE KEY-----'].join(' ');
const GH = ['gh', 'p_'].join('') + 'a'.repeat(36);

const add = (file, line) => `+++ b/${file}\n+${line}\n`;

test('代表的な秘密を検出する', () => {
  const cases = [
    `const k = "${ANT}"`,
    `AWS=${AWS}`,
    `gh=${GH}`,
    PEM,
    `url = https://user:${'super' + 'secret'}@example.com/x`,
    'password = "abcdefghijklmnopqrstuvwxyz"',
    'srun_' + 'b'.repeat(43),
  ];
  for (const c of cases) assert.ok(scanDiff(add('src/a.ts', c)).length > 0, c);
});
test('普通のコードは誤検出しない', () => {
  const ok = ['const password = getPassword();', 'let token = null;', 'fn main() { println!("hi"); }', '// sk- は説明'];
  for (const c of ok) assert.equal(scanDiff(add('src/a.ts', c)).length, 0, c);
});
test('削除された行(-)の秘密は対象外、追加行だけ見る', () => {
  assert.equal(scanDiff('+++ b/a\n-' + AWS + '\n').length, 0);
});
test('秘密を含みうるファイル名を検出(example は除く)', () => {
  assert.equal(scanDiff('', ['.env']).length, 1);
  assert.equal(scanDiff('', ['config/id_rsa']).length, 1);
  assert.equal(scanDiff('', ['a/key.pem']).length, 1);
  assert.equal(scanDiff('', ['.env.example']).length, 0);
  assert.equal(scanDiff('', ['src/main.rs']).length, 0);
});
test('保護された場所(CI・フック・設定)の変更を検出する', () => {
  assert.deepEqual(protectedPaths(['src/a.ts', '.github/workflows/ci.yml', '.husky/pre-push', 'CLAUDE.md', '.claude/settings.json']),
    ['.github/workflows/ci.yml', '.husky/pre-push', 'CLAUDE.md', '.claude/settings.json']);
  assert.deepEqual(protectedPaths(['docs/CLAUDE.md.bak', 'src/.github.ts']), []);
});
test('伏せ字: 既知の形式と、指定した値', () => {
  const t = redact(`エラー ${OAT} と mytokenvalue123`, ['mytokenvalue123']);
  assert.ok(!t.includes('sk-ant-oat01') && !t.includes('mytokenvalue123'));
});

// --- 2026-10-11 の点検(AI に仕込まれた指示で、ランナーの秘密を持ち出させる攻撃)への対策 ---
const SECRET = 'oat01-' + 'Q'.repeat(10) + 'abcdefghijklmnopqrstuvwxyz0123456789';

test('伏せ字: ランナーの秘密を base64・base64url・16進にしても伏せる(前に何文字あっても)', () => {
  for (const prefix of ['', 'x', 'xy', 'token: ']) {
    for (const enc of ['base64', 'base64url', 'hex']) {
      const encoded = Buffer.from(prefix + SECRET).toString(enc);
      const out = redact(`要約: ${encoded} 以上`, [SECRET]);
      assert.ok(out.includes('[伏せ字]'), `${enc} prefix=${JSON.stringify(prefix)}: ${out}`);
      assert.ok(!containsKnownSecret(out, [SECRET]), `${enc} prefix=${JSON.stringify(prefix)} が残っている`);
    }
  }
});

test('差分に、ランナーの秘密を符号化して書いても検出する', () => {
  const diff = `+++ b/notes.txt\n+${Buffer.from('k=' + SECRET).toString('base64')}\n`;
  assert.ok(scanDiff(diff, ['notes.txt'], [SECRET]).some((f) => /ランナーの秘密/.test(f.kind)));
  assert.equal(scanDiff('+++ b/a.js\n+const x = 1;\n', ['a.js'], [SECRET]).length, 0);
});

test('保護された場所: AI への指示・検査の設定・自動実行の設定も変えさせない', () => {
  const files = ['AGENTS.md', '.gitattributes', '.gitleaks.toml', '.gitleaksignore', '.mcp.json', '.vscode/tasks.json',
    '.devcontainer/devcontainer.json', '.pre-commit-config.yaml', 'lefthook.yml'];
  assert.deepEqual(protectedPaths(files), files);
  assert.deepEqual(protectedPaths(['src/app.ts', 'docs/AGENTS-guide.md']), []);
});
