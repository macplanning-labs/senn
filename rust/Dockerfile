# ============================================================
# rust/Dockerfile — WIP Rust(Axum)バックエンド
# マルチステージビルド(ビルド用イメージ + 軽量実行用イメージ)
# ビルドコンテキストは rust/ ディレクトリ自身を想定
#   (docker-compose.nas.yml / docker-compose.yml で build: ./rust を指定)
# ============================================================

FROM rust:1.90-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

# 依存クレートだけを先にビルドしてDockerレイヤーキャッシュに載せる
# (開発標準書 8.1 のDockerビルド時の懸念「イメージが巨大化。ビルドキャッシュ効かない」対策)。
# Cargo.toml/Cargo.lockが変わらない限り、ソースだけの変更ではこの層は再実行されない。
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static

# 現状は sqlx::query()/query_as() (実行時チェック版)のみを使用しているため、
# ビルド時のDB接続は不要。将来 query!/query_as! (コンパイル時チェック版)を
# 導入する場合は `cargo sqlx prepare` で .sqlx/ を生成しCOPYを追加すること。
RUN cargo build --release

FROM debian:trixie-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/wip /app/wip
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/static /app/static

EXPOSE 8151

CMD ["/app/wip"]
