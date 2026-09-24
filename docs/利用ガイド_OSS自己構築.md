# 利用ガイド — SENN OSS 自己構築

SENN を自身の環境でセルフホストする際のガイドです。

## インストール・起動

### 必要な環境

- Docker / Docker Desktop / Colima
- git
- bash

### 起動手順

```bash
# 1. リポジトリをクローン
git clone https://github.com/macplanning-labs/senn.git
cd senn

# 2. ワンコマンドで起動
./scripts/oss-up.sh
```

起動中に自動的に `.env.oss` が生成されます（初回のみ）。

ブラウザで **http://localhost:8151** にアクセスしてください。

## 初回セットアップ

### 1. ユーザー登録

- **Username**: 任意（日本語可）
- **Email**: 有効なメールアドレス
- **Password**: 8文字以上

### 2. チーム作成（必須）

⚠️ **SENN ではチケットを作成する前にチーム を作成する必要があります。**

1. 左サイドバーの **Teams** をクリック（見当たらない場合は ⌘K で検索）
2. **+ New Team** をクリック
3. チーム名を入力して作成

### 3. チケット作成

1. チーム詳細ページから **New Ticket** をクリック
2. チケットのタイトル・説明・優先度などを入力

## トラブルシューティング

### ポート 8151 がすでに使用されている

別のアプリケーションが使用中です。以下のコマンドで確認できます：

```bash
lsof -i :8151
```

もしくは、`docker-compose.oss.yml` を編集して別のポート（例: 8081）に変更してください：

```yaml
services:
  web:
    ports:
      - "8081:80"  # 8151 から 8081 に変更
```

### データベースの接続エラー

```bash
docker compose -f docker-compose.oss.yml --env-file .env.oss logs db
```

ログを確認してください。

### ブラウザが「このサイトに接続できません」と表示される

- SENN がまだ起動中である可能性があります。数秒待ってから再度アクセスしてください
- Docker コンテナが起動しているか確認：
  ```bash
  docker compose -f docker-compose.oss.yml --env-file .env.oss ps
  ```

### ログイン後、真っ白な画面が表示される

ブラウザキャッシュをクリアしてみてください（Ctrl+Shift+Delete または Cmd+Shift+Delete）。

## データ保存場所

すべてのデータは Docker ボリュームに保存されます：

- **データベース**: `senn-oss-pgdata` ボリューム
- **ファイル添付**: `senn-oss-media` ボリューム

これらは自分のマシン上にのみ存在し、当社サーバーと通信しません。

## データのバックアップ

### ボリュームのバックアップ

```bash
# データベースのダンプ
docker compose -f docker-compose.oss.yml --env-file .env.oss exec db \
  pg_dump -U senn senn > backup.sql

# メディアファイルのバックアップ
docker run --rm -v senn-oss-media:/data -v $(pwd):/backup \
  alpine tar czf /backup/media-backup.tar.gz /data
```

## スクリプトの役割

| スクリプト | 役割 |
|:---|:---|
| `scripts/oss-up.sh` | 自己ホスト起動（compose + 疎通確認） |
| `scripts/oss-down.sh` | 停止（データ保持） |
| `scripts/oss-down.sh --volumes` | **アンインストール**（コンテナ停止＋ボリューム削除） |

本リポジトリ（OSS 自己ホスト利用者向け）には `scripts/oss-sync.sh` は用意していない。最新コードの取り込みは `git pull` のあと `oss-down.sh` → `oss-up.sh` で再起動する。

社内メンテナ向けの OSS 展開・公開 mirror 同期は **OSSP** リポジトリ（`/Users/yutaka/workspace/OSSP`）で運用する。`scripts/oss-sync.sh` と公開用作業コピー `oss-checkout/` は OSSP 側にある（旧 `senn-oss-extract` パスは廃止）。本リポ（senn）へ `oss-sync.sh` をコピーしない。

## 停止・削除（アンインストール）


### 停止（データ保持）

```bash
./scripts/oss-down.sh
```

再度 `./scripts/oss-up.sh` で起動できます。

### 完全削除

```bash
./scripts/oss-down.sh --volumes
```

⚠️ **注意**: すべてのデータが削除されます。必要に応じてバックアップしてください。

## 環境変数のカスタマイズ

`.env.oss` ファイルで以下の設定をカスタマイズできます：

| 変数 | デフォルト | 説明 |
|---|---|---|
| `DB_PASSWORD` | ランダム生成 | PostgreSQL パスワード |
| `JWT_SECRET_KEY` | ランダム生成 | JWT署名鍵(必須。32文字以上。未設定・既定値のままでは起動しません) |
| `POSTGRES_DB` | `senn` | データベース名 |
| `POSTGRES_USER` | `senn` | データベースユーザー |

編集後は `./scripts/oss-down.sh && ./scripts/oss-up.sh` で再起動してください。

## ネットワーク設定

SENN は `http://localhost:8151` でのみリッスンします。

他のマシンからアクセスする場合は、`docker-compose.oss.yml` で以下を変更してください：

```yaml
services:
  web:
    ports:
      - "0.0.0.0:8151:80"  # localhost のみから全インターフェースに変更
```

## よくある質問

### Q: SENN をアップデートするには？

```bash
# 新しいバージョンをフェッチ
git pull

# 再起動
./scripts/oss-down.sh
./scripts/oss-up.sh
```

新しいマイグレーションが自動的に実行されます。

### Q: ファイアウォール越しに使用できる？

はい。ルーターやファイアウォール設定で 8151 番ポートを許可し、自分の IP アドレスから接続してください。

ただし、本来オフライン・プライベートな用途を想定しているため、インターネット経由での使用はセキュリティに関する十分な理解が必要です。

### Q: マルチユーザーで使用できる？

はい。各ユーザーが独立したアカウントを作成して使用できます。チームを使ってアクセス制御することができます。

### Q: 本番環境での使用は？

セルフホスト版は開発・評価用です。本番環境での使用を予定されている場合は、当社までお問い合わせください。

## サポート

問題が発生した場合は、GitHub Issues でお報告ください。

https://github.com/macplanning-labs/senn/issues
