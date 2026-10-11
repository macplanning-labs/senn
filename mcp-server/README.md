# SENN MCP Server

SENN の AI 用 API(`/api/v1/ai-agent/`)を、Model Context Protocol(MCP)のツールとして使えるようにするサーバーです。
Claude Code・Claude Desktop・Cursor などの MCP クライアントから、チケットの検索・起票・更新、Wiki の読み書き、
AI の作業の記録などを、**あなた自身の SENN** に対して行えます。

あわせて、作業の着手・完了を SENN に記録するコマンド `senn-ai-run` も入っています(下の「AI 実行報告 CLI」)。

## 先に知っておいてほしいこと(データの行き先)

- MCP のツールが返す内容(**チケットの本文・コメント・Wiki のページ**など)は、あなたが使っている AI(Claude なら Anthropic)に送られます。
  AI に見せてよい情報だけを扱うプロジェクトで使ってください。
- AI が操作できるのは、**キーの持ち主本人が SENN の画面で見られる物・できること**だけです(人ごとのキー)。
- Wiki の添付ツールは、この MCP サーバーが動いている端末のファイルを読んで SENN に送ります。秘密のファイルを持ち出されないよう、
  送れるのは「作業中のフォルダ(または `SENN_UPLOAD_ROOTS`)の下」の通常のファイルだけで、`.ssh`・`.env` など「.」で始まるもの・鍵・認証情報らしいものは断ります(下の「設定」)。

## セットアップ

### 1. ビルド

```bash
cd mcp-server
npm ci
npm run build
```

### 2. 接続先を書く

`~/.config/senn-ai-agent/credentials.env` を作り、あなたの SENN の URL を書きます(権限は 600 にしてください)。

```bash
mkdir -p ~/.config/senn-ai-agent && chmod 700 ~/.config/senn-ai-agent
printf 'SENN_BASE_URL=%s\n' 'https://senn.example.com' > ~/.config/senn-ai-agent/credentials.env
chmod 600 ~/.config/senn-ai-agent/credentials.env
```

`https://senn.example.com` は、あなたの SENN のホストに置き換えてください。手元で動かしている場合は `http://localhost:8080` などです。
localhost 以外を `http://`(暗号化なし)で指定すると、警告が出ます。

### 3. MCP クライアントに登録する

秘密の値を MCP クライアントの設定ファイルに書かずに済むよう、起動スクリプト `run-with-credentials.sh` を登録します
(`credentials.env` を読んで起動します)。パスは、この `mcp-server` を置いた場所に合わせてください。

Claude Code:

```bash
claude mcp add senn -- bash /path/to/senn/mcp-server/run-with-credentials.sh
```

Claude Desktop(`claude_desktop_config.json`)・Cursor(`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "senn": {
      "command": "bash",
      "args": ["/path/to/senn/mcp-server/run-with-credentials.sh"]
    }
  }
}
```

### 4. この端末を接続する(AI 用のキー)

AI 用のキーは、次のどちらかで用意します。

- **端末の接続(おすすめ・コピー不要)**: AI に「SENN に接続して」と頼むと、`senn_connect_account` がリンクを返します。
  リンクを開いて「許可を求める」を押し、SENN のシステム管理者が承認すると、キーがこの端末の `credentials.env` に自動で保存され、すぐ使えます。
  キーはこの端末の中で作られ、SENN には指紋(ハッシュ)だけが送られます。
- **画面で発行する**: SENN の「設定」→「AI」タブの「AI Agent を利用した MCP 操作」で利用を申請し、管理者の承認後に「キーを発行する」。
  表示されたキーを `credentials.env` に `SENN_AI_API_KEY=...` として保存します(キーは一度しか表示されません)。

管理者は「システム管理」→「AI 設定」の「AI Agent を利用した MCP 操作の申請」で、承認・却下します。

## 設定

`credentials.env`(または環境変数)に書ける値です。`.env.example` と同じく、値をリポジトリにコミットしないでください。

| 名前 | 必須 | 内容 |
|---|---|---|
| `SENN_BASE_URL` | 必須 | あなたの SENN の URL |
| `SENN_AI_API_KEY` | 任意 | 人ごとの AI 用のキー。無ければ、端末の接続(`senn_connect_account`)だけが使えます |
| `SENN_CF_ACCESS_CLIENT_ID` / `SENN_CF_ACCESS_CLIENT_SECRET` | 任意 | SENN の前に Cloudflare Access を置き、AI 用 API をサービストークンで守っている場合だけ(2つで1組) |
| `SENN_UPLOAD_ROOTS` | 任意 | Wiki に添付してよいフォルダ(OS のパス区切りで複数可)。未設定なら、MCP サーバーを起動したフォルダ。起動したフォルダが `/` やホームそのもの(Claude Desktop など)のときは、これを設定しないと添付できません |
| `SENN_UPLOAD_MAX_MB` | 任意 | 添付の上限(MB、既定 20) |

値が空・テスト用の値(`fake-...` など)のときは、起動を止めて理由を出します(値そのものは表示しません)。

## 利用可能なツール

このサーバーは以下のカテゴリのツールを提供します：

### 端末の接続
- `senn_connect_account` — この端末を、利用者本人として接続する(リンクを返す。利用者は「許可を求める」を1回押すだけ。管理者が承認すると、キーが自動で保存され、すぐ使える)
- `senn_connect_status` — 接続の状態(未申請・申請中・接続済み・終了)

### プロジェクト
- `senn_list_projects` — 全プロジェクト一覧を取得
- `senn_create_project` — 新規プロジェクトを作成（オーナーチーム指定可）

### チケット
- `senn_list_tickets` — プロジェクト内のチケット一覧を取得（状態・除く状態・ラベル名・Cycle・担当者・更新日時で絞り込み可）
- `senn_get_ticket` — チケット詳細を取得
- `senn_create_ticket` — 新規チケットを作成（状態・ストーリーポイント・Cycle・親・開始日も1回で指定可）
- `senn_bulk_create_tickets` — 複数のチケットを、依存関係・親子ごと1回で登録（計画の中の仮の名前 `ref` で書く。全部か何もしないか。`dry_run` で確認だけ）
- `senn_update_ticket` — チケットを更新（ステータス・優先度・アサインなど。ラベルは名前で `add_labels` / `remove_labels` / `label_names`）
- `senn_delete_ticket` — チケットを削除（論理削除）
- `senn_add_ticket_comment` — チケットにコメントを追加

### 依存関係
- `senn_list_ticket_dependencies` — チケットの依存関係一覧を取得
- `senn_add_ticket_dependency` — チケット間の依存関係を追加（`ticket_key` = 先に終わらせる側、`to_task_key` = 後に続く側。種類は `blocks` / `relates_to`）
- `senn_delete_ticket_dependency` — 依存関係を削除
- `senn_get_project_dependency_graph` — プロジェクト全体の依存関係グラフを取得

### チケット参照リンク
- `senn_list_ticket_links` — チケットの参照リンク一覧を取得
- `senn_add_ticket_link` — 参照リンクを追加
- `senn_delete_ticket_link` — 参照リンクを削除

### Cycle
- `senn_list_cycles` — Cycle 一覧を取得
- `senn_create_cycle` — 新規 Cycle を作成
- `senn_get_cycle` — Cycle 詳細を取得
- `senn_update_cycle` — Cycle を更新
- `senn_delete_cycle` — Cycle を削除

### チーム
- `senn_list_teams` — チーム一覧を取得
- `senn_create_team` — 新規チームを作成（日本語名は `slug` 明示推奨。チケット番号の頭 `prefix` も指定可）
- `senn_update_team` — チームの名前・説明・`prefix` などを変更（チームの管理者の個人キーだけ。作成済みのチケットの番号は変わらない）

### AI の作業記録
AI が着手・完了・失敗を SENN に記録するためのツール(シェルなら下の `senn-ai-run` と同じ)。チケット詳細の「AI 用プロンプトをコピー」から始めたときだけでなく、MCP で直接作業するときも呼ぶと、着手日・レポート・トークンの量が「実行」機能から始めたときと同じように記録される。
- `senn_ai_run_start` — 作業を始める最初の操作として呼ぶ(`ticket_key`、`run_uuid`(省略すると新しく作って返す)、`tool`、`model`)。チケットが進行中になる。同じ `run_uuid` の再呼び出しは何もしない
- `senn_ai_run_finish` — 作業が終わったら呼ぶ(`run_uuid`、`result`=resolved/blocked/aborted、`summary`、使ったトークンの量 `input_tokens` / `output_tokens` / `cache_read_tokens` / `cache_creation_tokens` / `usd_equivalent`)。resolved でもチケットは closed にならない(マージ・本番反映は人が行う)
- `senn_ai_run_error` — 失敗したときに呼ぶ(`run_uuid`、`detail`)。チケットに「AI連携エラー」が付き、担当者に通知される

### Wiki
- `senn_list_wiki_pages` — Wiki ページ一覧を取得（本文を含む全件。多いときは search を使う）
- `senn_search_wiki_pages` — Wiki ページを検索・一覧（本文なしの軽量版。search/category/page）
- `senn_get_wiki_page` — Wiki ページを1件取得（本文はMarkdown）
- `senn_list_wiki_revisions` — Wiki ページの変更履歴を取得
- `senn_create_wiki_page` — Wiki ページを作成（書き込み権限が必要）
- `senn_update_wiki_page` — Wiki ページを更新（変更コメント付き。書き込み権限が必要）
- `senn_delete_wiki_page` — Wiki ページを削除（書き込み権限が必要）

> Wiki の作成・更新・削除は、**人ごとのAI用APIキー**でのみ可能です。キーの持ち主本人の権限（プロジェクト／チームのメンバー）で画面と同じ規則が適用されます。
>
> `project_prefix` を省略すると、**全体 Wiki**（どのプロジェクト・チームにも属さないページ）が対象になります（検索・取得・作成・更新・削除・履歴）。全体 Wiki のページを削除できるのは、作成者かシステム管理者だけです。
- `senn_list_wiki_attachments` — Wiki ページの添付ファイル一覧を取得
- `senn_upload_wiki_attachment` — Wiki ページに添付ファイルをアップロード(送れるファイルの制限は「設定」の `SENN_UPLOAD_ROOTS`)
- `senn_replace_wiki_attachment` — Wiki ページの添付ファイルを置換(同上)
- `senn_delete_wiki_attachment` — Wiki ページの添付ファイルを削除

## 開発

### ビルド

```bash
npm run build
```

### 開発モード（ファイル変更時に自動リビルド）

```bash
npm run dev
```

### テスト

```bash
npm test
```

- テストは必ず `test/_isolated-home.mjs` を最初に import する。HOME を一時フォルダに差し替え、本物の `~/.config/senn-ai-agent` などが変わっていたらテストを失敗させる（テストが本物の `credentials.env` を偽の値で上書きしないように）。
- 認証情報がテスト用の値（`fake-` など）なら、MCP サーバー・senn-ai-run・`run-with-credentials.sh` は起動を止めて理由を出す。

### 単体起動（テスト用）

```bash
npm start
```

## トラブルシューティング

### 「接続先の SENN が設定されていません」と出た場合

`~/.config/senn-ai-agent/credentials.env` に `SENN_BASE_URL=` が書かれているか確認してください。

### ツールが認証エラー(HTTP 401・403)を返す場合

AI 用のキーがまだ無いか、失効しています。AI に「SENN に接続して」と頼み、端末の接続をやり直してください。

### 「Cloudflare Access に止められました」と出た場合

SENN の前に Cloudflare Access を置いている場合だけ起きます。`SENN_CF_ACCESS_CLIENT_ID` / `SENN_CF_ACCESS_CLIENT_SECRET` が正しいか確認してください。

### ビルドエラーが出た場合

```bash
rm -rf node_modules dist
npm install
npm run build
```

でリセットしてみてください。

## AI 実行報告 CLI `senn-ai-run`

Claude Code や AI エージェントが作業の着手・完了・失敗を SENN に報告するための CLI です。

### コマンド

```bash
# 作業開始を報告
senn-ai-run start <ticket_key> [--run <uuid>] [--tool claude_code] [--model <model>] [--trigger manual|webhook|api]

# 作業完了を報告
senn-ai-run finish <run_uuid> --result resolved|blocked|aborted [--summary <text>] [--summary-file <path>]

# エラーを報告
senn-ai-run error <run_uuid> --detail <text> [--ticket <ticket_key>]
```

### 例

```bash
# 新規実行を開始
senn-ai-run start PROJ-000123
# 出力: {"run_uuid":"...", "ticket_key":"...", "ticket_status":"..."}

# 実行を完了
senn-ai-run finish abc-123-def --result resolved --summary "実装完了"

# エラーを報告
senn-ai-run error abc-123-def --detail "ネットワークエラーで中断"
```

### 認証

- **ローカルでの実行**：`~/.config/senn-ai-agent/credentials.env` から自動読み込み(MCP サーバーと同じファイル)
- **クラウド実行**：`CLAUDE_CODE_REMOTE_SESSION_ID` が設定されている場合、プロキシにより認証ヘッダーが自動付与される（CLI は環境変数を付けない）
- **環境変数設定**：明示的に以下を設定することも可能
  - `SENN_BASE_URL`(必須)
  - `SENN_AI_API_KEY`
  - `SENN_CF_ACCESS_CLIENT_ID` / `SENN_CF_ACCESS_CLIENT_SECRET`(Cloudflare Access を使う場合だけ)

### 終了コード

- `0`：成功
- `1`：ネットワーク・HTTP エラー（リトライ後も失敗）
- `2`：引数エラー
