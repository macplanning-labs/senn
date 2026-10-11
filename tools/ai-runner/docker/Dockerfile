# AI(claude -p)を動かすサンドボックスのイメージ。ランナーのマシン上でビルドする(配布はしない)。
# 含めるもの: Node.js、git、Claude Code。含めないもの: 認証情報・SSH 鍵・docker CLI。
FROM node:22-slim

# 開発に必要な最小限の道具(言語ごとの追加は、利用者がこのファイルに足す)
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates ripgrep \
 && rm -rf /var/lib/apt/lists/*

# Claude Code(バージョンは固定する。更新は、確認してからビルドし直す)
ARG CLAUDE_CODE_VERSION=2.1.296
RUN npm install -g @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION} && npm cache clean --force

# ホストのユーザーで動かす(docker run --user)ため、HOME は実行時に tmpfs を割り当てる
ENV HOME=/home/sandbox DISABLE_AUTOUPDATER=1 DISABLE_TELEMETRY=1
WORKDIR /workspace
