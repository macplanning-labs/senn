# 詳細設計書 — SENN OSS 自己構築 Getting Started

- 計画: [`実装計画_SENN_OSS自己構築GettingStarted.md`](./実装計画_SENN_OSS自己構築GettingStarted.md)

## 1. ランタイム構成

```
Browser → http://localhost:8080 (nginx:web)
            ├─ /          → frontend/dist (SPA)
            ├─ /assets/   → 静的
            ├─ /api/      → rust:8151
            └─ /media/    → volume
         rust → postgres:5432
```

同一オリジンのためブラウザ CORS 追加は不要（デスクトップ tauri 向け CORS は既存のまま）。

## 2. ファイル

| パス | 役割 |
| --- | --- |
| `LICENSE` | MIT（README 記載と一致） |
| `docker-compose.oss.yml` | OSS 自己ホスト専用。project name `senn-oss` |
| `docker/Dockerfile.oss-web` | FE build + nginx イメージ |
| `docker/nginx-oss.conf` | 本番 `nginx.conf` を OSS 向けに簡略（cloudflared 前提コメント削除可、ルーティング同等） |
| `.env.oss.example` | `DB_PASSWORD` / `SESSION_SECRET` / `DJANGO_SECRET_KEY` / `POSTGRES_*` |
| `scripts/oss-up.sh` | example から `.env.oss` 生成（既存があれば尊重）、`compose up --build -d`、URL 表示 |
| `scripts/oss-down.sh` | 停止（ボリューム削除はオプションフラグ） |
| `README.md` | 冒頭に **Getting Started（OSS）**。開発者 Quick Start は下位へ |
| `docs/利用ガイド_OSS自己構築.md` | トラブルシュート・データ置き場・チーム必須の説明 |

## 3. Compose サービス要件

### db

- `postgres:16-alpine`
- volume `senn-oss-pgdata`
- healthcheck 必須

### rust

- `build: ./rust`（既存 Dockerfile）
- `RUST_RUN_MIGRATIONS=true`（または既存と同等のマイグレーション起動）
- `DATABASE_URL` / `SESSION_SECRET` / `DJANGO_SECRET_KEY` / `PORT=8151`
- db healthy 待ち
- ポートは内部のみでも可（外部公開は nginx 経由）。デバッグ用に `8151:8151` を付けてもよい

### web

- `docker/Dockerfile.oss-web`
- `8080:80`
- rust に依存
- media volume 共有

## 4. 初回ユーザー導線（必須ドキュメント）

1. 開く: `http://localhost:8080`  
2. 新規登録（ユーザー名 / メール / パスワード 8+）  
3. **チーム作成**（`/teams`。サイドバーに無い場合は URL 直打ちまたは ⌘K）  
4. チーム詳細からチケット作成  

My Issues 空状態に「先にチームを作成」リンクがあれば尚よい（タスクに含める）。

## 5. セキュリティ／名より実

- `.env.oss` を gitignore（`.env.oss.example` のみ追跡）  
- ドキュメントに当社本番 URL を「自己ホスト先」として書かない  
- 秘密を README に実値で書かない  

## 6. 検証

`scripts/oss-up.sh` 後:

- `curl -sS -o /dev/null -w '%{http_code}' http://localhost:8080/` → 200  
- `POST /api/v1/auth/register/` → 201  
- （可能なら）UI または API で team + ticket 作成  

Haiku はローカル Docker で可能な範囲を実行し結果を報告。
