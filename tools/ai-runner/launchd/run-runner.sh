#!/bin/bash
# run-runner.sh — launchd から呼ばれる、ランナーの起動用ラッパー。
# 設定ファイル(~/.senn-ai-runner/runner.env と claude.env)を読み、サンドボックスの土台(プロキシ)を起動してから、ランナーを動かす。
# `--check` を付けると、起動せずに、設定だけを点検する。
set -eu
HOME_DIR="${RUNNER_HOME:-$HOME/.senn-ai-runner}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECK=0; [ "${1:-}" = "--check" ] && CHECK=1

fail() { echo "[ランナー] $1" >&2; exit 1; }

# 秘密を含むファイルは、自分以外が読めないこと(権限 600)
for f in "$HOME_DIR/runner.env" "$HOME_DIR/claude.env"; do
  [ -f "$f" ] || fail "設定ファイルがありません: $f"
  perm="$(stat -f '%Lp' "$f")"
  [ "$perm" = "600" ] || fail "$f の権限が $perm です。chmod 600 してください"
done

# 起動環境から、従量課金につながる設定を外してから、設定を読む(ランナー自身も拒否するが、二重にする)
unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
set -a
# shellcheck disable=SC1090
. "$HOME_DIR/runner.env"
# shellcheck disable=SC1090
. "$HOME_DIR/claude.env"
set +a

NODE="${NODE_BIN:-$(command -v node || true)}"
[ -n "$NODE" ] || fail "node が見つかりません"
"$NODE" -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 20 ? 0 : 1)' || fail "Node.js 20 以上が必要です"

# 設定の検証だけ(不正なら、ここで止まる)
"$NODE" -e "import('$ROOT/src/config.mjs').then(m=>{m.parseConfig();}).catch(e=>{console.error(e.message);process.exit(1)})" || fail "設定が不正です"

if [ "$CHECK" = "1" ]; then echo "[ランナー] 設定の点検: OK"; exit 0; fi

# Docker が使えるまで待つ(ログイン直後は、Docker Desktop の起動が遅い)
# (`docker info` は、Docker のプラグインを調べる処理が、常駐のサービスの環境では止まることがあるため使わない)
for i in $(seq 1 60); do docker ps -q >/dev/null 2>&1 && break; sleep 5; [ "$i" = 60 ] && fail "Docker が起動しません"; done

# サンドボックスのイメージとネットワーク(中継役は、実行のたびにランナーが起動する)
docker image inspect "${SANDBOX_IMAGE:-senn-ai-sandbox:latest}" >/dev/null 2>&1 || fail "サンドボックスのイメージがありません(docker build が必要)"
bash "$ROOT/docker/setup-networks.sh" >/dev/null || fail "サンドボックスのネットワークに問題があります。ターミナルで $ROOT/docker/setup-networks.sh を実行して、案内に従ってください"

exec "$NODE" "$ROOT/src/runner.mjs"
