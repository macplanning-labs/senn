<title>WIP</title>

# WIP — プロジェクト管理ツール

チケット管理・ガントチャート・Wiki・通知を備えたプロジェクト管理ツールです。バックエンドはRust(Axum)、フロントエンドはReact(TypeScript)で構築されています。

## アーキテクチャ

バックエンド(`rust/`)はClean Architectureに沿って3層に分かれています。

```
rust/src/
├── domain/          # ドメインモデル・ドメインサービス(ビジネスロジック)
├── infrastructure/  # リポジトリ実装(PostgreSQL, sqlx)、メール送信など
├── presentation/     # HTTPハンドラ、ミドルウェア
├── routes.rs         # ルーティング定義
├── config.rs          # 環境変数からの設定読み込み
└── main.rs            # エントリポイント
```

認証まわりは `rust/auth-core/` に独立クレートとして切り出されており、JWT発行・検証、TOTP、WebAuthn(パスキー)を扱います。

フロントエンド(`frontend/`)はVite + React + TypeScript。APIクライアントはOpenAPIスキーマから[orval](https://orval.dev/)で自動生成しています(`npm run generate:api`)。

## セットアップ

### 前提

- Rust 1.90以上
- Node.js 20以上
- PostgreSQL 16

### 手順

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

### データベースのセットアップ

`rust/migrations/20260701000000_initial_schema.sql` が全テーブルを作成するベースラインマイグレーションです(本番スキーマから`pg_dump --schema-only`で抽出し、Django管理テーブル等を除去して統合したもの)。空のPostgreSQLを用意した状態で `cargo run`(`RUST_RUN_MIGRATIONS=true`)を実行すれば、このマイグレーションが自動適用され、テーブルが作成されます。

```bash
createdb wip
cp .env.example .env  # DATABASE_URL 等を環境に合わせて編集
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
