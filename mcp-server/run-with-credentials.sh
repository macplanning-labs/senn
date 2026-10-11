#!/usr/bin/env bash
# SENN MCP — credentials.env を読んで node を起動する。
# 秘密はこのスクリプトに書かない。値は ~/.config/senn-ai-agent/credentials.env(権限 600)か、環境変数で渡す。
#
#   SENN_BASE_URL                  必須。あなたの SENN の URL(例: https://senn.example.com)
#   SENN_AI_API_KEY                任意。無ければ、MCP の senn_connect_account でこの端末を接続する
#   SENN_CF_ACCESS_CLIENT_ID       任意。SENN の前に Cloudflare Access を置いている場合だけ(SECRET と対で)
#   SENN_CF_ACCESS_CLIENT_SECRET

set -euo pipefail

NEW_CREDENTIALS="${HOME}/.config/senn-ai-agent/credentials.env"
OLD_CREDENTIALS="${HOME}/.config/wip-ai-agent/credentials.env"

CREDENTIALS=""
if [[ -f "$NEW_CREDENTIALS" ]]; then
  CREDENTIALS="$NEW_CREDENTIALS"
elif [[ -f "$OLD_CREDENTIALS" ]]; then
  CREDENTIALS="$OLD_CREDENTIALS"
fi

if [[ -n "$CREDENTIALS" ]]; then
  set -a
  # shellcheck source=/dev/null
  source "$CREDENTIALS"
  set +a
fi

# SENN_* が無ければ旧 WIP_* をコピー（プロセス内のみ）
export SENN_CF_ACCESS_CLIENT_ID="${SENN_CF_ACCESS_CLIENT_ID:-${WIP_CF_ACCESS_CLIENT_ID:-}}"
export SENN_CF_ACCESS_CLIENT_SECRET="${SENN_CF_ACCESS_CLIENT_SECRET:-${WIP_CF_ACCESS_CLIENT_SECRET:-}}"
export SENN_AI_API_KEY="${SENN_AI_API_KEY:-${WIP_AI_API_KEY:-}}"
export SENN_BASE_URL="${SENN_BASE_URL:-${WIP_BASE_URL:-}}"

# テスト用の値なら起動しない。
# テストが credentials.env を fake-client-id などで上書きすると、再起動したときに 403 だけが返り、
# 原因が分からなくなる。ここで止めて理由を出す。値そのものは表示しない。
# 判定は mcp-server/src/credentials-guard.ts と同じ（直すときは両方そろえる）。
is_placeholder() {
  local lower re_prefix re_shape
  lower="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  re_prefix='^(fake|test|dummy|example|sample|placeholder|changeme|change-me|your|xxx)([-_.]|$)'
  re_shape='^(<.*>|\.\.\.|\*+)$'
  [[ "$lower" =~ $re_prefix ]] && return 0
  [[ "$lower" =~ $re_shape ]] && return 0
  return 1
}

problems=()
if [[ -z "${SENN_BASE_URL//[[:space:]]/}" ]]; then
  problems+=("SENN_BASE_URL（未設定。あなたの SENN の URL を設定してください）")
fi
for name in SENN_CF_ACCESS_CLIENT_ID SENN_CF_ACCESS_CLIENT_SECRET SENN_AI_API_KEY; do
  value="${!name}"
  if [[ -n "${value//[[:space:]]/}" ]] && is_placeholder "$value"; then
    problems+=("$name（テスト用の値）")
  fi
done
cf_id="${SENN_CF_ACCESS_CLIENT_ID//[[:space:]]/}"
cf_secret="${SENN_CF_ACCESS_CLIENT_SECRET//[[:space:]]/}"
if [[ -n "$cf_id" && -z "$cf_secret" ]]; then
  problems+=("SENN_CF_ACCESS_CLIENT_SECRET（空。ID と対で設定してください）")
elif [[ -z "$cf_id" && -n "$cf_secret" ]]; then
  problems+=("SENN_CF_ACCESS_CLIENT_ID（空。SECRET と対で設定してください）")
fi

if (( ${#problems[@]} > 0 )); then
  {
    echo "SENN MCP を起動しません: 設定の値が使えません。"
    echo "  読み込み元: ${CREDENTIALS:-環境変数のみ（credentials.env なし）}"
    for p in "${problems[@]}"; do echo "  - $p"; done
    echo "テスト用の値と出た場合は、credentials.env がテストで上書きされた可能性があります。"
    echo "本物の値に戻してから、アプリを再起動してください。"
  } >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")" && pwd)"
exec node "$ROOT/dist/index.js" "$@"
