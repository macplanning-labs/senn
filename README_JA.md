[English](./README.md) | 日本語

# SENN — プロジェクト管理ツール

> **Public Beta（公開ベータ版）** — 使えますが、不具合や足りない機能が残っています。アップデートで仕様が変わることがあります。**アップデートの前にデータをバックアップしてください**（新しい版に上げたデータベースは、古い版に戻せません）。以前の版は [タグ](https://github.com/macplanning-labs/senn/tags) として残しています。たとえば `v0.1.1` はアクセス制御の再設計の前の最後の版で、`git checkout v0.1.1` で取り出すか、`.env` に `SENN_IMAGE_TAG=sha-6ebc8da` と書くと、その版のビルド済みイメージで起動します。

チケット管理・チーム(マルチチーム対応)・ガントチャート・Wiki・通知を備えたプロジェクト管理ツールです。バックエンドはRust(Axum)、フロントエンドはReact(TypeScript)で構築されています。

## アーキテクチャ

UI は **React SPA** です。以前はダッシュボード・チケット・ガント・Wiki等の一部画面をAskamaでサーバー描画していましたが、現在はすべてSPAに置き換わっています。サーバー描画で残っているのはログイン前の認証画面（`rust/templates/auth/`：ログイン・MFA・パスワードリセット・WebAuthn）のみで、SPAバンドル読み込み前に表示されます。

### バックエンド (`rust/`)

Clean Architecture に沿った3層構成:

```
rust/src/
├── domain/          # ドメインモデル・ドメインサービス(ビジネスロジック)
├── infrastructure/  # リポジトリ実装(PostgreSQL, sqlx)、メール送信など
├── presentation/     # HTTPハンドラ(JSON API + ログイン前の認証画面)、ミドルウェア
├── routes.rs         # ルーティング定義
├── config.rs          # 環境変数からの設定読み込み
└── main.rs            # エントリポイント
```

`rust/templates/auth/` にログイン前の認証画面用Askamaテンプレートがあります。それ以外はすべてSPAが担います。
認証は `rust/auth-core/`（JWT / TOTP / WebAuthn）。

### フロントエンド (`frontend/`)

Vite + React + TypeScript の SPA。API クライアントは OpenAPI から [orval](https://orval.dev/) で生成（`npm run generate:api`）。

SPA 内の UI 複雑度の使い分け:

- **マスタ / 設定**（`features/settings/` など）— 軽量な CRUD フォーム
- **チケット / カンバン / Git 連携** — リッチなインタラクティブ UI

## セットアップ

### クイックスタート（Docker・推奨）

Rust / Node / PostgreSQL のホストへの個別インストールは不要です。

ターミナル（macOS / Linux、Windows は WSL2）で、次の5行を実行します。

```bash
git clone https://github.com/macplanning-labs/senn.git
cd senn
cp .env.example .env
sed -i.bak "s|^DB_PASSWORD=.*|DB_PASSWORD=$(openssl rand -hex 24)|; s|^JWT_SECRET_KEY=.*|JWT_SECRET_KEY=$(openssl rand -base64 48 | tr -d '\n')|" .env && rm .env.bak
docker compose up
```

- 3行目で設定ファイル（`.env`）を作ります。
- 4行目で `DB_PASSWORD` と `JWT_SECRET_KEY` をランダムな値で自動的に書き込みます。`.env.example` では両方を**意図的に空**にしてあり、どちらかが空のままだと `docker compose` は分かりやすいメッセージで即停止します。
- 5行目で起動します。**初回はイメージのダウンロードに数分かかります**（回線による）。このターミナルは閉じないでください（閉じると SENN が止まります）。

`sed` や `openssl` が使えない環境（Windows など）では、`.env` をエディタで開き、`DB_PASSWORD=` に英数字のみの文字列（URLに埋め込まれるため）、`JWT_SECRET_KEY=` に32文字以上のランダムな文字列を設定してから、`docker compose up` を実行します。

ブラウザで http://localhost:8151 を開きます。初回起動時にテーブルは自動作成されます（`RUST_RUN_MIGRATIONS=true`）。初期シードユーザーは同梱していないため、http://localhost:8151/register の画面上の「新規登録」から最初のアカウントを作成し、http://localhost:8151/login からログインしてください。

初回の使い方: **新規登録**（登録すると自動でログインされます）→ サイドバーの「チーム」の右の **＋** でチームを作成 → **チケットを作成**（チームは選択済み）。

#### メンバーを増やす

自由に登録できるのは**最初の1人**だけです（サーバーを立ち上げた人）。2人目からは**招待**で入ります。

1. チームのメンバー設定の **招待** で、相手のメールアドレスを入れて **招待を送る** を押します。
2. メールの設定が無いとき（既定）は、メールの代わりにリンクが表示されます。そのリンクを相手に渡してください（1回だけ使えます。有効期限は7日）。
3. 相手はリンクを開き、ユーザー名とパスワードを決めると、チームに入れます。

変えたいときは、`.env` に次を書いて再起動します。
- `SENN_INTERNAL_EMAIL_DOMAINS=example.com` — このドメインのメールアドレスの人は、メールでアドレスを確認したうえで、自分で登録できます。**先にメール送信を設定してください**（`.env` の `EMAIL_*`）。メールの設定が無いと、確認用のリンクが登録画面に表示されるため、誰でもそのドメインのアドレスを名乗れてしまいます。
- `SENN_REGISTRATION_MODE=open` — サーバーに届く人は誰でも登録できます。社内ネットワークなど、閉じた環境でだけ使ってください。

停止は、ターミナルで `Ctrl+C` を**1回**押してから `docker compose down`（データは残ります）。再開は `docker compose up`（`--build` は不要）です。

データベースをリセットしてやり直す場合:

```bash
docker compose down -v
docker compose up
```

### ソースからビルドしたい方

プリビルトイメージをダウンロードするのではなく、このリポジトリのソースコードから Docker イメージをビルドしたい場合は、以下のコマンドを実行してください:

```bash
docker compose up --build
```

ソースコードから両方のイメージ（backend と web）をビルドします。初回のビルドは 10〜15分かかります（CPU や回線による）。ソースコードは GitHub 上で公開されています。

### ダウンロードが遅いとき

既定では、`docker compose up` は `ghcr.io/macplanning-labs` からプリビルトイメージをダウンロードします。ダウンロードが遅い、または利用できない場合は、次の2つの方法があります:

1. **ソースからビルド**: `docker compose up --build` を使用してください。
2. **別のレジストリを使う**: `.env` に `SENN_IMAGE_REGISTRY` を追加し、組織のレジストリやミラーを指定します。例:
   ```text
   SENN_IMAGE_REGISTRY=registry.example.com/macplanning-labs
   ```
   これで `docker compose` がイメージを取得する場所が変わります。

### 手動セットアップ（Dockerを使わない場合）

#### 前提

- Rust 1.90以上
- Node.js 20以上
- PostgreSQL 16

#### 手順

```bash
cp .env.example .env
# .env を編集し、DATABASE_URL 等を環境に合わせて設定

cd rust
cargo run
```

```bash
cd frontend
npm install
npm run dev
```

#### データベースのセットアップ

`rust/migrations/20260701000000_initial_schema.sql` が全テーブルを作成するベースラインマイグレーションです(本番スキーマから`pg_dump --schema-only`で抽出し、Django管理テーブル等を除去して統合したもの)。空のPostgreSQLを用意した状態で `cargo run`(`RUST_RUN_MIGRATIONS=true`)を実行すれば、このマイグレーションが自動適用され、テーブルが作成されます。

```bash
createdb senn
cp .env.example .env  # DATABASE_URL（例: postgresql://USER@localhost/senn）等を環境に合わせて編集
cd rust
cargo run
```

## テスト

ローカル CI 相当（[CONTRIBUTING.md](./CONTRIBUTING.md) / メンテナ用 `run_local_ci.sh` と同じ）:

```bash
cd rust
export SQLX_OFFLINE=true
cargo check --workspace
cargo test --workspace

cd ../frontend
npm ci
npm run build
npm test
```

## ライセンス

[MIT](./LICENSE)
