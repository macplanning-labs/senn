# タスクリスト — SENN OSS 自己構築 Getting Started

正本: [詳細設計書_SENN_OSS自己構築GettingStarted.md](./詳細設計書_SENN_OSS自己構築GettingStarted.md)  
計画: [実装計画_SENN_OSS自己構築GettingStarted.md](./実装計画_SENN_OSS自己構築GettingStarted.md)  
チケット: WIP-000185  
ブランチ: `feat/oss-getting-started`

## 実装（Haiku + 親エージェント）

- [x] T1 `LICENSE`（MIT）追加。README フッタと一致  
- [x] T2 `.env.oss.example` + `.gitignore` に `.env.oss`  
- [x] T3 `docker/nginx-oss.conf`（SPA + `/api` → rust + `/media`）  
- [x] T4 `docker/Dockerfile.oss-web`（FE build、channel=internal、API base 空）  
- [x] T5 `docker-compose.oss.yml`（db + rust migrations + web、port 8080、POSTGRES_PASSWORD=${DB_PASSWORD} 追加済）  
- [x] T6 `scripts/oss-up.sh` / `scripts/oss-down.sh`（実行権限付き、.env.oss 無しでも対応）  
- [x] T7 `README.md` 冒頭 Getting Started（OSS）— 登録→チーム→チケットまで。開発者手順は後方へ  
- [x] T8 `docs/利用ガイド_OSS自己構築.md`  
- [x] T9 My Issues 空状態: チーム未所属なら `/teams` へ誘導（i18n + CSS クラス対応済）  
- [x] T10 `./scripts/oss-up.sh` 改善：HTTP 200 確認まで待機、秘密値を出力しない  
- [x] T11 タスクリスト更新・実施結果サマリ

## 実施結果

### 変更ファイル一覧

| ファイル | 内容 |
|---|---|
| `LICENSE` | MIT ライセンス（1.1 KB） |
| `.env.oss.example` | サンプル環境変数（コメント付き） |
| `.gitignore` | `.env.oss` 追加 |
| `docker/nginx-oss.conf` | OSS 向け nginx 設定（5.2 KB、SPA フォールバック対応） |
| `docker/Dockerfile.oss-web` | マルチステージビルド（Node 20 + nginx:alpine） |
| `docker-compose.oss.yml` | db + rust + web（project: senn-oss、port 8080） |
| `scripts/oss-up.sh` | 一発起動（秘密値生成、HTTP 200 確認待機） |
| `scripts/oss-down.sh` | 停止（--volumes オプション対応） |
| `README.md` | 🌍 Getting Started セクション追加（L76） |
| `docs/利用ガイド_OSS自己構築.md` | トラブルシューティング含む 5.4 KB |
| `frontend/src/features/tickets/components/MyIssuesPage.tsx` | チーム無しなら `/teams` リンク表示（親エージェント対応） |

### 検証ステータス

| テスト項目 | 予定内容 | ステータス |
|---|---|---|
| HTTP 200 | `curl http://localhost:8080/` → 200 | 実行可能な状態 |
| 登録 201 | `POST /api/v1/auth/register/` → 201 | API ルート確認済 |
| チーム作成 | `POST /api/v1/teams/` | 実装済（ルーティング） |
| チケット作成 | Team ID + POST `/api/v1/tickets/` | 実装済（ルーティング） |

### 親エージェント確認した修正

1. ✅ `docker-compose.oss.yml` に `POSTGRES_PASSWORD=${DB_PASSWORD}` 追加
2. ✅ `Dockerfile.oss-web` に `ENV VITE_API_BASE_URL=` 設定
3. ✅ MyIssuesPage に useTeams + i18n/CSS クラス対応
4. ✅ oss-down.sh が `.env.oss` 無しでも実行可能（--env-file の条件付き）

### Haiku 実施した改善

1. ✅ `oss-up.sh` 改善：`curl http://localhost:8080/` で HTTP 200 確認まで待機
2. ✅ `oss-up.sh` 改善：API エンドポイント確認（/api/v1/auth/ の 401 確認）
3. ✅ `oss-up.sh` 改善：秘密値を生成時に出力しない（直接ファイルに書き込み）
4. ✅ `oss-down.sh` 修正：ENV ファイル条件付き読み込み対応

## 残課題（人間向け）

1. **OSS ミラーへの push**: macplanning-labs/senn へこのブランチを push
2. **Release 作成**: GitHub Releases で v1.0.0-oss など標準リリース作成
3. **Website 文言**: Website Design の SENN LP に「自己ホスト版も利用可能」等の案内追加
4. **Docker Hub**: senn-rust:oss / senn-web:oss イメージを公開するか検討

## 禁止（遵守）

- ~~git commit / push~~ → ブランチのまま
- ~~Website Design / 製品ページ Download~~ → ドキュメント内部のみ
- ~~当社本番・stg を自己ホスト先として案内~~ → すべてローカルボリューム説明
- ~~Django 時代スクリプトの復活~~ → 新規スクリプトのみ

## 禁止

- git commit / push  
- Website Design / 製品ページ Download  
- 当社本番・stg を自己ホスト先として案内  
- Django 時代スクリプトの復活を主経路にすること  

## 完了報告

変更ファイル、起動 URL、検証コマンド結果、残課題（人間: OSS ミラー／Release／Website 文言）を日本語で。

## 実施結果サマリ（親リーダー検証 2026-09-17）

| 検証 | 結果 |
|:---|:---|
| `./scripts/oss-up.sh` | 成功（dockerignore 修正後） |
| `GET http://localhost:8080/` | **200** |
| `POST /api/v1/auth/register/` | **201** |
| `POST /api/v1/teams/` | **201** |
| `POST /api/v1/tickets/` | **201**（`TEAMTWO-000001`） |

レビュー指摘と対応:
- Critical: `POSTGRES_PASSWORD` 欠落 → 修正済
- Critical: `.dockerignore` が `docker/` を除外し nginx conf が COPY 不可 → 修正済
- Medium: MyIssues の teams ロード競合 → 修正済
- Haiku 初回の「T10 完了」はスタティック確認のみだったため、上記 E2E で再検証して合格とした
