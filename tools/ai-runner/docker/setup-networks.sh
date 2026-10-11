#!/usr/bin/env bash
# サンドボックス用のネットワークを用意する(何度実行しても同じ結果になる)。
#   senn-ai-internal : 外に出られない内部ネットワーク(AI のコンテナと、実行ごとの中継役が参加する)
#   senn-ai-egress   : 外に出られるネットワーク(中継役だけが参加し、api.anthropic.com に送る)
# 以前の版のプロキシ(senn-ai-proxy)は使わない。内部ネットワークに残っていると、中継役を通らずに外へ出られるため、
# ランナーは起動を止める。下の案内に従って取り除く。
set -euo pipefail
INTERNAL="${SANDBOX_NETWORK:-senn-ai-internal}"
EGRESS="${SANDBOX_EGRESS_NETWORK:-senn-ai-egress}"

if docker network inspect "$INTERNAL" >/dev/null 2>&1; then
  [ "$(docker network inspect --format '{{.Internal}}' "$INTERNAL")" = "true" ] \
    || { echo "ネットワーク $INTERNAL は外に出られる設定です。docker network rm $INTERNAL のあと、もう一度実行してください" >&2; exit 1; }
  echo "ネットワーク $INTERNAL はあります(内部ネットワーク)"
else
  docker network create --internal "$INTERNAL" >/dev/null && echo "ネットワーク $INTERNAL を作りました(内部ネットワーク)"
fi
if docker network inspect "$EGRESS" >/dev/null 2>&1; then
  echo "ネットワーク $EGRESS はあります"
else
  docker network create "$EGRESS" >/dev/null && echo "ネットワーク $EGRESS を作りました"
fi

others="$(docker network inspect --format '{{range .Containers}}{{.Name}} {{end}}' "$INTERNAL" | tr ' ' '\n' | grep -vE '^(senn-ai-gw-[0-9a-f-]{8}|senn-ai-[0-9a-f-]{8})?$' || true)"
if [ -n "$others" ]; then
  echo "内部ネットワーク $INTERNAL に、中継役と AI のコンテナ以外がつながっています: $others" >&2
  echo "以前の版のプロキシなら、取り除いてください: docker rm -f senn-ai-proxy" >&2
  exit 1
fi
