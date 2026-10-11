# SENN AI ランナー

SENN のチケット画面の「AIに対応させる」を、**あなたのマシン**で、**あなたの Claude Pro / Max** を使って実行するプログラムです。
SENN に依頼が入ると、ランナーが取りに来て、隔離した Docker の中で Claude Code(`claude -p`)を動かし、
結果(コミット)と使用量を SENN に報告します。依存パッケージはありません(Node.js 20 以上と Docker だけ)。

## 使う前に知っておいてほしいこと

- **Anthropic 社にデータが送られます。** AI に作業を頼むと、チケットの内容(説明・コメント・投稿者名・リンク)と、
  対象のリポジトリのコードが、このランナーに設定した Claude のアカウントを通じて Anthropic 社に送られます。
  データの扱いは、そのアカウントの Anthropic の規約と設定に従います。社外秘のコードや、外部への送信が制限されている
  データを扱う場合は、使う前に確認してください。SENN の画面でも、有効にするときと実行するときに確認を求めます。
- **使うのは、あなたの Claude のサブスクリプションです**(`claude setup-token` で作るトークン)。従量課金の API キーは使いません
  (`ANTHROPIC_API_KEY` が設定されていると、起動を拒否します)。
- **AI の変更は、必ず人がレビューしてからマージしてください。** チケットに仕込まれた指示に AI が従う可能性
  (プロンプトインジェクション)は、ゼロにはできません。下の「脅威モデル」を読んでください。

## 仕組み(要点)

1. ランナーが SENN に **自分から** 定期的に問い合わせる(外から入ってくる通信はない)。
2. 依頼は署名つき。ランナーは、署名・プロンプトのハッシュ・リポジトリの許可リストを確認し、上限を**自分の絶対上限との小さいほう**に丸める。
3. リポジトリを作業用に取得する(`.git` の本体は、AI から見えない場所に置く)。
4. 実行ごとに、**中継役**(`gateway/gateway.mjs`)と、AI を動かす**使い捨てのコンテナ**を起動する(下の「隔離」)。
5. 30 秒ごとに使用量を SENN に送る。SENN が「止めて」と返したら、コンテナごと止める。
6. 終わったら、**AI の外側で**変更を点検し(秘密情報・CI やフックなどの変更)、問題がなければコミットする。
7. 着手・完了・失敗の報告は、ランナーが行う(**AI は SENN の鍵を持たない**)。マージ・本番反映は人が行う。

## 隔離(コンテナの設定)

```
 [AI のコンテナ] ──内部ネットワーク(外に出られない)──> [中継役] ──> api.anthropic.com
   偽のトークンだけ                                      本物のトークン
```

- **本物の Claude のトークンは、中継役だけが持つ。** AI のコンテナには偽のトークンだけを渡す(AI が読み出しても何にも使えない)。
  中継役は、AI から来た認証情報を捨てて本物のトークンに差し替え、**固定の宛先(api.anthropic.com)にだけ**送る。
- 中継役は、**Anthropic のサーバーで動く道具(Web の取得・検索、MCP の接続、コードの実行など)を使う要求を断る**。
  AI が操られて、Anthropic のサーバー経由でデータを外へ送らせる抜け道になるため。許可していない宛先への要求も断る。
- AI のコンテナ: 読み取り専用のルート / ケーパビリティ全削除 / 特権昇格なし / メモリ・CPU・プロセス数の制限 / ホストと同じ一般ユーザー /
  **マウントは作業ディレクトリ(読み書き)と hooks(読み取り専用)の 2 つだけ** / docker.sock・ホストのネットワーク・特権は渡さない /
  通信は内部ネットワークの中継役だけ / プロンプトは標準入力で渡す。
- Claude の権限は `acceptEdits` で、`--dangerously-skip-permissions` は使わない。`git push`・`curl`・`WebFetch` などは拒否リストに入っている。
- コンテナ内の読み込みは、大きなファイルを丸ごと読まないフック(`hooks/read-guard.mjs`)で、トークンの浪費を防ぐ。
- ランナーは起動時に、内部ネットワークが外に出られないこと、中継役と AI のコンテナ以外がつながっていないことを確かめ、満たさなければ起動しない。

## 設置手順

必要なもの: Node.js 20 以上 / Docker / Claude Pro または Max / (push するときだけ)GitHub のトークン(対象リポジトリだけ・内容の書き込みだけができる細かい権限のもの)。

1. **SENN 側**(システム管理者が行う。最初に登録した人がシステム管理者になります)
   - 管理画面「AI 自動実行」で、上限(予算など)を保存し、**AI が作業してよいリポジトリ**を登録する
   - 同じ画面でランナーを追加し、**トークンと署名用の秘密**を控える(この 1 回しか表示されません)
2. **サンドボックスを用意する**(ランナーのマシンで):
   ```bash
   cd tools/ai-runner/docker
   docker build -t senn-ai-sandbox:latest .
   ./setup-networks.sh            # 内部ネットワークと、中継役の出口のネットワーク
   cd .. && ./verify-sandbox.sh   # 隔離と中継役の確認。「すべて合格」になること
   ```
   手順 3 のトークンができたら、`( export CLAUDE_CODE_OAUTH_TOKEN=... ; ./scripts/try-gateway.sh )` で、
   中継役を通して Claude Code が動くことを確かめられます(SENN は使わず、1 問だけ質問します)。
3. **Claude のトークンを作る**: Claude Code がインストールされたマシンで `claude setup-token` を実行し、
   表示されたトークンを控える(Pro / Max のアカウントでログインして作る。**API キーは使わない**)。
   ランナー専用のアカウント・サブスクリプションを使うと、万一のときの影響を小さくできます。
4. **環境変数を設定して起動する**(値はファイルに保存し、権限を 600 にする。チャットやチケットに貼らない):
   ```bash
   export SENN_URL=https://あなたの SENN の URL
   export RUNNER_TOKEN=srun_...
   export RUNNER_SIGNING_SECRET=...
   export CLAUDE_CODE_OAUTH_TOKEN=...          # 手順 3 のトークン
   export ALLOWED_REPOS=owner/repo1,owner/repo2 # 手順 1 で登録したものと同じに
   node src/runner.mjs
   ```
5. SENN の管理画面でランナーが「接続中」になることを確認する。プロジェクトの設定で「AI 自動実行」を有効にし
   (Anthropic に送られることの確認が必要)、チケットの「AIに対応させる」を押す。

**最初は安全モード(既定)のまま使う**: ブランチの push をせず、作業ディレクトリ(`~/.senn-ai-runner/work/<run>/`)のコミットまでにとどめる。
内容を確認して問題なければ、`SAFE_MODE=0` と `GITHUB_TOKEN` を設定して push を有効にする。
push を有効にするときは、**秘密を使う CI(GitHub Actions など)が `ai/**` のブランチで動かない**ようにしてください
(AI が書いたコードが、CI の秘密つきで実行されるのを防ぐため)。

## 常駐させる

### macOS(launchd)

ログイン時に自動で起動し、落ちたら再起動する(連続の再起動は 30 秒空ける)。Docker が動いているログイン中のユーザーで動かす。

```bash
mkdir -p ~/.senn-ai-runner && chmod 700 ~/.senn-ai-runner
cp launchd/runner.env.example ~/.senn-ai-runner/runner.env && chmod 600 ~/.senn-ai-runner/runner.env   # 値を埋める
# CLAUDE_CODE_OAUTH_TOKEN は ~/.senn-ai-runner/claude.env に export 行で保存(権限 600)
./launchd/install.sh --dry-run   # 点検と、生成する内容の確認だけ
./launchd/install.sh             # 登録して起動
```

- ログ: `~/.senn-ai-runner/logs/runner.log` / 一時停止: `touch ~/.senn-ai-runner/STOP`(解除は削除)/ 登録の解除: `./launchd/uninstall.sh`
- **コードを更新したとき**: 常駐のサービスは、`~/.senn-ai-runner/app/` にコピーしたコードで動く。リポジトリを更新したら、コピーし直して、サービスを再登録する:
  ```bash
  rsync -a --delete --exclude node_modules --exclude test tools/ai-runner/ ~/.senn-ai-runner/app/ && ~/.senn-ai-runner/app/launchd/install.sh
  ```
- 起動のたびに、設定ファイルの権限(600)・設定の検証・Docker の起動・サンドボックスのイメージとネットワークを確認する。問題があれば起動せず、ログに理由を出す。

### Linux など

`node src/runner.mjs` を、環境変数を設定したうえで、systemd のユーザーサービスなどで動かしてください
(`Restart=on-failure`、`EnvironmentFile=` に権限 600 のファイルを指定)。

## 以前の版から更新するとき

以前の版は、許可リスト付きのプロキシ(`senn-ai-proxy`)を使っていました。今の版は中継役を使うので、プロキシは**取り除いてください**
(内部ネットワークに残っていると、中継役を通らずに外へ出られるため、ランナーは起動しません):

```bash
docker rm -f senn-ai-proxy
cd tools/ai-runner/docker && docker build -t senn-ai-sandbox:latest . && ./setup-networks.sh && cd .. && ./verify-sandbox.sh
```

## 設定(環境変数)

| 名前 | 既定 | 意味 |
|---|---|---|
| `SENN_URL` / `RUNNER_TOKEN` / `RUNNER_SIGNING_SECRET` | 必須 | SENN の URL(https のみ)と、ランナーの資格情報 |
| `CLAUDE_CODE_OAUTH_TOKEN` | 必須 | サブスクリプションのトークン(`claude setup-token`)。中継役だけに渡り、AI のコンテナには渡らない |
| `ALLOWED_REPOS` | 必須 | 実行を許可するリポジトリ(`owner/name`、カンマ区切り)。SENN の管理画面の許可と**両方**が必要 |
| `CF_ACCESS_CLIENT_ID` / `CF_ACCESS_CLIENT_SECRET` | なし | SENN の前に Cloudflare Access を置いているときだけ。`/api/v1/ai-agent/*` 用のサービストークン(2 つそろえる) |
| `SAFE_MODE` | `1` | `0` で push を許可(`GITHUB_TOKEN` が必要) |
| `CEILING_MAX_TURNS` / `_MINUTES` / `_WEIGHTED_TOKENS` | 400 / 180 / 30,000,000 | **このマシン側の絶対上限**。SENN から大きな値が来ても超えない |
| `ACTIVE_HOURS` | 常時 | 実行してよい時間帯(例 `9-21`、`22-6`)。外は依頼を受けない |
| `SANDBOX_MEMORY` / `SANDBOX_CPUS` / `SANDBOX_PIDS` | 4g / 2 / 512 | コンテナの資源の上限 |
| `SANDBOX_NETWORK` / `SANDBOX_EGRESS_NETWORK` | senn-ai-internal / senn-ai-egress | AI のコンテナの内部ネットワーク(外に出られないものに限る)と、中継役の出口 |
| `CLAUDE_MODEL` | (Claude の既定) | 使うモデル。トークンを節約したいときは軽いモデルを指定 |
| `ALLOW_INSECURE_LOCALHOST` | なし | `1` で、localhost に限り http を許可(開発用) |

**止める方法**: ① SENN の管理画面の「緊急停止」(全ランナー・数十秒以内) ② ランナーのマシンで `touch ~/.senn-ai-runner/STOP`(依頼を受けず、実行中のものも止める)
③ プロセスの終了(SIGTERM。実行中のものを止めて報告する)。

**起動の拒否**: `ANTHROPIC_API_KEY`(従量課金)が設定されている、URL が https でない、必須の設定がない、ネットワークが安全でない場合は、起動しません。

## 脅威モデル(正直に)

| 脅威 | 対策 | 残るリスク |
|---|---|---|
| AI の暴走・トークンの浪費 | 1 実行の上限(ターン・時間・トークン)をランナーと SENN の両方で強制 / 使用量の急増で自動の緊急停止 / 予算 / 緊急停止ボタン | 上限までは使う。上限は小さいほうが安全 |
| チケット本文の命令に AI が従う(プロンプトインジェクション) | 本文は「データ」として枠で囲み、規則を優先 / **AI に権限を与えない**(SENN の鍵なし、push の認証情報なし、本物の Claude のトークンなし、通信は中継役だけ) / 変更は AI の外で点検してから保存 | **完全には防げない。** 悪意ある本文で、そのコンテナの中の作業が歪められる可能性はある。だから、変更は必ず人がレビューしてからマージする |
| 秘密情報の持ち出し | 本物の Claude のトークンは中継役だけが持つ / 中継役は宛先を固定し、サーバー側の道具を断る / 最終報告・エラー報告・コミットから、ランナーの秘密を伏せる・止める(符号化した形も) | リポジトリのコードそのものは Anthropic に送られる(仕組み上の前提)。非公開のリポジトリを推奨 |
| ほかのチームのリポジトリでの作業 | 作業先は、SENN のシステム管理者が許可したリポジトリと、ランナーの `ALLOWED_REPOS` の両方に入っているものだけ | 信頼の度合いが違うチームのリポジトリを、1 つのランナーに混ぜない |
| コンテナからの脱出 | 特権・docker.sock・ホストのネットワークなし、ケーパビリティ全削除、読み取り専用 | Docker のカーネル共有の限界。より強い隔離が必要なら、専用のマシン/VM で動かす |
| 作業ディレクトリ経由でホストを攻撃(フックなど) | `.git` の本体を作業ディレクトリの外に置き、ホストの git はフック・設定を無効にして使う | |
| CI や AI への指示を書き換えて、push 後に実行させる | `.github/`・`.husky/`・`CLAUDE.md`・`AGENTS.md`・`.claude/`・`.vscode/`・`.devcontainer/`・gitleaks の設定などの変更は、保存しない | 秘密を使う CI が `ai/**` で動かないように設定する |
| 依頼の改ざん | HMAC 署名・ハッシュ・リポジトリの許可リスト | 署名用の秘密が漏れると偽の依頼が作れる(ランナーの再作成で無効化) |
| 通信の盗聴 | https のみ(localhost の開発を除く) | |

OSS 版では、**安全モードが既定**です。push・マージ・本番反映を人が行う前提は変わりません。

## 開発・テスト

```bash
cd tools/ai-runner
npm test                # 単体テスト(SENN・Docker・claude は不要。偽物で検証。中継役のテストを含む)
./verify-sandbox.sh     # 実際のコンテナで、隔離と中継役を確認する(Docker が必要)
```
