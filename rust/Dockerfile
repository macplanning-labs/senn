# syntax=docker/dockerfile:1
# ============================================================
# rust/Dockerfile — SENN Rust(Axum)バックエンド
# マルチステージビルド(ビルド用イメージ + 軽量実行用イメージ)
# ビルドコンテキストは rust/ ディレクトリ自身を想定
#   (docker-compose.production.yml / docker-compose.yml で build: ./rust を指定)
# ============================================================

# ------------------------------------------------------------
# KB知識エンジン用のルール処理系 Scryer Prolog(単体バイナリ)。
# バージョンは固定し --locked で依存も固定する。builder と独立したステージなので、
# SENN 本体のソース変更ではこのレイヤーは再ビルドされない(キャッシュが効く)。
# 実行時は SENN_KB_ENGINE=rules のときだけ、判定ごとに起動して終了する(常駐しない)。
# ------------------------------------------------------------
FROM rust:1.90-slim AS scryer

ARG SCRYER_VERSION=0.10.0

# libffi-sys が同梱 libffi を autotools でビルドするため make/autoconf/automake/libtool が必要
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev make autoconf automake libtool && \
    rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install scryer-prolog --locked --version "${SCRYER_VERSION}" --root /opt/scryer && \
    strip /opt/scryer/bin/scryer-prolog

FROM rust:1.90-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY auth-core ./auth-core
COPY google-auth ./google-auth
COPY cf-access-jwt ./cf-access-jwt
COPY migrations ./migrations
COPY kb_rules ./kb_rules
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
# ビルド成果物はキャッシュマウント外の/app/sennにコピーしてイメージに残す
# (キャッシュマウント自体の中身はイメージレイヤーに含まれないため)。
#
# 反映するコミットの SHA を、実行ファイルに焼き込む(DEMO-000143)。
# 2026-10-10、stg で、ソースが変わったのに、前のビルドの実行ファイルがそのまま使われ、
# 新しい SHA のタグだけが付いたイメージが反映された(AI の開始日の修正 #526 が入っていなかった)。
# 次の 2 つを狙う。
#   1. SHA が変わるたびに、この RUN の層が必ず作り直され、cargo も、環境変数の変化(option_env!)で
#      senn のクレートを必ず再コンパイルする(古い実行ファイルが使い回されない)。
#   2. 反映のスクリプトが、イメージ・稼働中のコンテナに `/app/senn --build-sha` を実行し、反映した SHA と
#      一致するかを確かめられる(一致しなければ、反映を止める・自動で戻す)。
# ARG はこの RUN の直前に置く(依存クレートのビルドや、COPY の層のキャッシュを、SHA で無効にしないため)。
ARG GIT_SHA=unknown
ENV SENN_BUILD_SHA=${GIT_SHA}
RUN --mount=type=cache,target=/app/target,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo build --release && cp target/release/senn /app/senn

FROM debian:trixie-slim AS runtime

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 postgresql-client && \
    rm -rf /var/lib/apt/lists/*

COPY --from=scryer /opt/scryer/bin/scryer-prolog /usr/local/bin/scryer-prolog
COPY --from=builder /app/senn /app/senn
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/static /app/static

EXPOSE 8151

CMD ["/app/senn"]
