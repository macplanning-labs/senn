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

### ⚠ 既知の制約: データベーススキーマ

`rust/migrations/` にあるsqlxマイグレーションは**増分マイグレーションのみ**で、`tickets`・`users`・`projects`などの土台となるテーブルを作成するベーススキーマを含んでいません(このリポジトリの前身であるDjangoアプリのマイグレーションがベーススキーマを担っていたため)。そのため、現時点では**空のPostgreSQLに対して`cargo run`しても正常に起動しません**。

新規に立ち上げる場合は、以下のいずれかの対応が必要です(未整備・要コントリビューション):

1. 既存の稼働環境から `pg_dump --schema-only` でベーススキーマを取得し、単一の初期マイグレーションとして `rust/migrations/` に追加する
2. または、ベーススキーマをRustのマイグレーションとして書き起こす

## テスト

```bash
cd rust
cargo test --workspace
```

## ライセンス

[MIT](./LICENSE)
