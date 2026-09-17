# 実装計画 — SENN OSS 自己構築 Getting Started

| 項目 | 内容 |
| --- | --- |
| 作成日 | 2026-09-17 |
| チケット | [WIP-000185](https://senn-app.macplanning.com/tickets/WIP-000185/) |
| ブランチ | `feat/oss-getting-started` |
| 方針 | [`方針_SENN対外提供_OSS自己構築.md`](./方針_SENN対外提供_OSS自己構築.md) |

## 1. ゴール

実際に使ってみたい人が、OSS を取得し README を読んで自環境を構築し、SENN で **チケットを1件登録できる**。

成功条件（E2E）:

1. `LICENSE`（MIT）がある  
2. `docker compose -f docker-compose.oss.yml up`（または `./scripts/oss-up.sh`）で API + DB + Web UI が立つ  
3. ブラウザで UI を開き、新規登録できる  
4. チームを作り、チケットを1件作成できる  
5. データは利用者マシン上の Docker ボリュームに残る（当社本番／stg ではない）  
6. README に上記の最短手順がある（開発者向け手順と分離）

## 2. 非目標

- 当社が試用 SaaS をホストすること  
- 製品サイト Download に .dmg を載せること（別方針）  
- Apple 公証・デスクトップ必須化（任意の後続で可）  
- macplanning-labs への強制 push（本リポで完了後、人間がミラー／Release）

## 3. 方式要約

| 要素 | 内容 |
| --- | --- |
| Compose | `docker-compose.oss.yml` — `db` + `rust`（migrations オン）+ `web`（nginx が FE 配信＋ `/api` プロキシ） |
| FE ビルド | OSS 用 Dockerfile で `VITE_APP_CHANNEL=internal`、`VITE_API_BASE_URL` 空（同一オリジン） |
| 秘密 | `.env.oss.example` + `oss-up.sh` が未設定時にランダム生成 |
| 初回 UX | README に「登録 → `/teams` でチーム作成 → チケット」。必要なら My Issues 空状態にチーム作成リンク |

## 4. 正本

- 詳細設計: [`詳細設計書_SENN_OSS自己構築GettingStarted.md`](./詳細設計書_SENN_OSS自己構築GettingStarted.md)  
- タスク: [`タスクリスト_SENN_OSS自己構築GettingStarted.md`](./タスクリスト_SENN_OSS自己構築GettingStarted.md)  
- 利用ガイド: [`利用ガイド_OSS自己構築.md`](./利用ガイド_OSS自己構築.md)（Haiku が作成）
