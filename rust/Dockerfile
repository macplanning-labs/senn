# syntax=docker/dockerfile:1
# ============================================================
# rust/Dockerfile — WIP Rust(Axum)バックエンド
# マルチステージビルド(ビルド用イメージ + 軽量実行用イメージ)
# ビルドコンテキストは rust/ ディレクトリ自身を想定
#   (docker-compose.production.yml / docker-compose.yml で build: ./rust を指定)
# ============================================================

FROM rust:1.90-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY auth-core ./auth-core
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static

# 現状は sqlx::query()/query_as() (実行時チェック版)のみを使用しているため、
# ビルド時のDB接続は不要。将来 query!/query_as! (コンパイル時チェック版)を
# 導入する場合は `cargo sqlx prepare` で .sqlx/ を生成しCOPYを追加すること。
#
# BuildKitのキャッシュマウントで target/ とレジストリキャッシュを永続化し、
# 依存クレートの再コンパイルを避ける(開発標準書 8.1 のDockerビルド時の懸念
# 「イメージが巨大化。ビルドキャッシュ効かない」対策)。
# 以前は「ダミーmain.rsで依存だけ先にビルド→本物のsrcに差し替えて再ビルド」
# という定石パターンを使っていたが、2つの`cargo build --release`が同一の
# コマンド文字列になるためDockerのレイヤーキャッシュが取り違え、ダミーの
# バイナリ(即終了するだけの空プログラム)がそのままイメージに焼き込まれる
# という重大な事故が実際に発生した(ステージングで起動直後にコンテナが
# 無限リスタートするクラッシュループとして発現)。キャッシュマウント方式は
# 常に実ソースからビルドするため、この種の取り違えが原理的に起きない。
# ビルド成果物はキャッシュマウント外の/app/wipにコピーしてイメージに残す
# (キャッシュマウント自体の中身はイメージレイヤーに含まれないため)。
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --release && cp target/release/wip /app/wip

FROM debian:trixie-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/wip /app/wip
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/static /app/static

EXPOSE 8151

CMD ["/app/wip"]
