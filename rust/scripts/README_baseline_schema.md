# ベースラインマイグレーションの再生成手順

`migrations/*_initial_schema.sql` は本番DBのスキーマから生成したベースライン
マイグレーションです。新しいテーブルを本番に直接追加した場合など、再生成が
必要になったときの手順をまとめます。

## 1. スキーマ抽出(pg_dump)

本番のPostgreSQLコンテナから、データを含まない「スキーマ(DDL)のみ」を、
owner/権限(ACL)情報を除いて抽出する。

```bash
docker exec -e PGPASSWORD='<パスワード>' <コンテナ名> \
  pg_dump -U <ユーザー名> -d <DB名> --schema-only --no-owner --no-privileges \
  > schema_raw.sql
```

- `--no-owner`: `OWNER TO ...` 文を出力しない(環境依存・OSS公開に不要)
- `--no-privileges`: `GRANT`/`REVOKE` 文を出力しない(同上)

コンテナの実際の接続情報が不明な場合は、まず環境変数を確認する。

```bash
docker exec <コンテナ名> env | grep -i postgres
```

## 2. クリーンアップ

`clean_schema_dump.py` が以下を自動で行う。

1. Django/django-axes由来の管理テーブル(`django_migrations`, `django_content_type`,
   `django_admin_log`, `django_session`, `auth_group*`, `auth_permission`,
   `axes_*`, `accounts_user_groups`, `accounts_user_user_permissions`)に関する
   `CREATE TABLE` / `ALTER TABLE` / `CREATE INDEX` / `CREATE SEQUENCE` /
   `ADD CONSTRAINT` をブロック単位で除去(FK依存も含めて連鎖的に除去される)
2. `_sqlx_migrations` テーブル定義を除去(sqlxが自前で作成するため)
3. `\restrict` / `\unrestrict`(pg_dump 16+が埋め込むpsql専用メタコマンド、
   sqlxのmigratorがSQLとして解釈できずエラーになる)を除去
4. `SELECT pg_catalog.set_config('search_path', '', false);`(セッション全体の
   search_pathを空にし、sqlx自身の`_sqlx_migrations`への非修飾クエリを
   壊す)を除去

除外対象テーブルを追加/変更したい場合は、スクリプト冒頭の `EXCLUDE_TABLES`
を編集する。除外対象への外部キー参照が他のテーブルに残っていないか、
事前に確認すること(下記コマンド)。

```bash
grep -nE "REFERENCES (public\.)?(除外テーブル名1|除外テーブル名2|...)" schema_raw.sql
```

実行:

```bash
python3 clean_schema_dump.py schema_raw.sql schema_cleaned.sql
```

## 3. マイグレーションとして配置

sqlxはファイル名先頭のタイムスタンプ順に実行するため、既存の増分マイグレー
ションより確実に先に実行させたい場合は、既存最古のマイグレーションより前の
タイムスタンプを使う。

```bash
cp schema_cleaned.sql migrations/YYYYMMDDHHMMSS_initial_schema.sql
```

**注意**: 既存の増分マイグレーションの変更内容が、抽出したスキーマに既に
反映されている場合(本番で適用済みのマイグレーションの場合)、そのまま両方
残すと `CREATE TABLE`/`ADD COLUMN` の重複エラーになる。該当する増分マイグ
レーションは削除し、ベースライン1本に統合すること。

## 4. ローカル検証

新規開発者と同じ状況を再現するため、空のPostgreSQLコンテナに対して
アプリ本体(またはsqlx-cli)でマイグレーションを実行し、エラーなく完了する
ことを確認する。

```bash
docker run -d --name schema-verify -e POSTGRES_USER=wip -e POSTGRES_PASSWORD=wip \
  -e POSTGRES_DB=wip -p 55432:5432 postgres:16-alpine

# アプリ本体での検証(実際の起動経路と同じなので最も確実)
DATABASE_URL="postgres://wip:wip@localhost:55432/wip" \
RUST_RUN_MIGRATIONS=true \
DJANGO_SECRET_KEY=test_secret_key_for_verification_only \
cargo run

# テーブル数の確認
docker exec schema-verify psql -U wip -d wip -t \
  -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public';"

# 後片付け
docker rm -f schema-verify
```

`sqlx-cli`(`sqlx migrate run`)とアプリ本体のsqlxクレートでバージョンが
異なる場合、`sqlx-cli`側だけ挙動が違うことがあるため、**アプリ本体での
検証を優先する**こと(実際のユーザー体験に一致するため)。
