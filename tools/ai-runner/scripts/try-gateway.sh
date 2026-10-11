#!/usr/bin/env bash
# 中継役と、あなたの Claude のトークンで、Claude Code が動くかを試す(SENN は使わない。1 問だけ質問する)。
#   使い方: CLAUDE_CODE_OAUTH_TOKEN を環境に入れて実行する(値は画面に出さない)
#     例) ( source ~/.senn-ai-runner/claude.env && ./scripts/try-gateway.sh )
#   使うもの: サンドボックスのイメージ(docker build 済み)。ネットワークは、試験用に別名で作って、終わったら消す。
#   動いている本番のランナーには触れない(名前がすべて別)。
set -uo pipefail
IMAGE="${SANDBOX_IMAGE:-senn-ai-sandbox:latest}"
ID="$(date +%s | tail -c 9)"
NET_IN="senn-ai-try-internal-$ID"; NET_OUT="senn-ai-try-egress-$ID"; GW="senn-ai-gw-try$(printf '%04d' $((RANDOM % 10000)))"
GWDIR="$(cd "$(dirname "$0")/.." && pwd)/gateway"
WS="$(mktemp -d "$HOME/.senn-ai-try.XXXXXX")"

[ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}" ] || { echo "CLAUDE_CODE_OAUTH_TOKEN が設定されていません(claude setup-token で作ったトークン)" >&2; exit 2; }
docker image inspect "$IMAGE" >/dev/null 2>&1 || { echo "イメージ $IMAGE がありません。tools/ai-runner/docker で docker build -t $IMAGE . を実行してください" >&2; exit 2; }

cleanup(){ docker rm -f "$GW" >/dev/null 2>&1; docker network rm "$NET_IN" "$NET_OUT" >/dev/null 2>&1; rm -rf "$WS"; }
trap cleanup EXIT

docker network create --internal "$NET_IN" >/dev/null && docker network create "$NET_OUT" >/dev/null
docker run -d --rm --name "$GW" --read-only --cap-drop=ALL --security-opt no-new-privileges --user "$(id -u):$(id -g)" \
  --network "$NET_OUT" -v "$GWDIR:/opt/gateway:ro" -e CLAUDE_CODE_OAUTH_TOKEN -e GATEWAY_PORT=8080 \
  --entrypoint node "$IMAGE" /opt/gateway/gateway.mjs >/dev/null || { echo "中継役を起動できませんでした" >&2; exit 1; }
docker network connect "$NET_IN" "$GW"
sleep 2

echo "== Claude Code に質問します(中継役を通して。AI のコンテナには偽のトークンだけを渡します)"
answer="$(echo '1+1 はいくつですか。数字だけを答えてください。' | docker run --rm -i --read-only --cap-drop=ALL --security-opt no-new-privileges \
  --network "$NET_IN" --tmpfs /tmp --tmpfs /home/sandbox:rw,mode=1777 --user "$(id -u):$(id -g)" -v "$WS:/workspace:rw" -w /workspace \
  -e HOME=/home/sandbox -e CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-senn-gateway-placeholder-not-a-real-token \
  -e "ANTHROPIC_BASE_URL=http://$GW:8080" -e DISABLE_AUTOUPDATER=1 -e DISABLE_TELEMETRY=1 -e CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
  "$IMAGE" timeout 180 claude -p --max-turns 1 2>&1 | tail -3)"
echo "答え: $answer"
echo "== 中継役のログ(メソッド・パス・状態だけ。「拒否」があれば、その宛先を許可するか判断が要ります)"
docker logs "$GW" 2>&1 | tail -20
if echo "$answer" | grep -q '2'; then echo "結果: 動きました"; else echo "結果: 期待した答えが返りませんでした(上のログを確認してください)"; exit 1; fi
