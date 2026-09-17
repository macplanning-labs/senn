# Haiku — OSS Getting Started WIP-000185

## 対象

`/Users/yutaka/workspace/プロジェクト管理/senn`  
ブランチ: `feat/oss-getting-started`（作業中のまま）

## 必読

- `docs/worklogs/haiku_prompt_SENN_OSS自己構築GettingStarted.md`（本ファイル）
- `docs/タスクリスト_SENN_OSS自己構築GettingStarted.md`
- `docs/詳細設計書_SENN_OSS自己構築GettingStarted.md`
- `docs/実装計画_SENN_OSS自己構築GettingStarted.md`
- 参考: `docker/nginx.conf`（プロキシ設定の踏襲）、`rust/Dockerfile`、`.env.example`（キー名のみ参考。実値禁止）

## 実装

タスク T1〜T11 をすべて実施。途中でユーザー確認しない。

特に:

1. MIT `LICENSE`
2. 一発起動: `docker-compose.oss.yml` + `scripts/oss-up.sh`
3. README 最短経路に **チーム作成必須** を明記
4. My Issues 空＋チーム無しなら `/teams` リンク（最小差分）
5. `oss-up` 後の検証（curl register まで必須。team/ticket は API で可能なら）

`.env.oss` 生成時の秘密はログに出さない。

## 禁止

- commit / push / Website / Release published
- 192.168 や senn-app を自己ホスト手順の接続先に書くこと

## 完了報告

日本語。ファイル一覧、検証結果、残（ミラー・Website）。
