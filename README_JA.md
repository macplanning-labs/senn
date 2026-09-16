[English](./README.md) | 日本語

# SENN — プロジェクト管理ツール

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

```bash
cp .env.example .env
# 初回起動前に DB_PASSWORD と JWT_SECRET_KEY をローカル用の値に書き換える
docker compose up --build
```

ブラウザで http://localhost:8151 を開きます。初回起動時にテーブルは自動作成されます（`RUST_RUN_MIGRATIONS=true`）。初期シードユーザーは同梱していないため、http://localhost:8151/register の画面上の「新規登録」から最初のアカウントを作成し、http://localhost:8151/login からログインしてください。

データベースをリセットしてやり直す場合:

```bash
docker compose down -v
docker compose up --build
```

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

```bash
cd rust
cargo test --workspace
```

## ライセンス

[MIT](./LICENSE)
