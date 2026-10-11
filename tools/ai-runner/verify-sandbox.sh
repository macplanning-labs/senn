#!/usr/bin/env bash
# サンドボックスの隔離を、実際のコンテナで確認する(ランナーを設置した直後と、イメージを更新したあとに実行)。
# 確認すること: ① 内部ネットワークから直接は外に出られない ② 中継役のほかに、外へ出る出口(プロキシなど)が無い
#              ③ 読み取り専用・特権なし ④ docker.sock が無い ⑤ 作業ディレクトリ以外のホストのファイルが見えない
#              ⑥ 名前解決・ホスト・メタデータを使った持ち出しの経路が無い(環境によって違うため、設置先ごとに確かめる)
#              ⑦ 中継役: AI のコンテナは中継役を通してだけ Anthropic に届き、本物のトークンは見えず、
#                 サーバー側の道具の指定は断られる(偽のトークンで確かめる。Anthropic からは 401 が返るのが正常)
set -u
IMAGE="${SANDBOX_IMAGE:-senn-ai-sandbox:latest}"
NET="${SANDBOX_NETWORK:-senn-ai-internal}"
EGRESS="${SANDBOX_EGRESS_NETWORK:-senn-ai-egress}"
GWDIR="$(cd "$(dirname "$0")" && pwd)/gateway"
WS="$(mktemp -d "$HOME/.senn-ai-verify.XXXXXX")"; echo "secret-marker" > "$WS/in-workspace.txt"
fail=0
ok(){ echo "  OK   $1"; }
ng(){ echo "  NG   $1"; fail=1; }
run(){ docker run --rm --read-only --cap-drop=ALL --security-opt no-new-privileges --network "$NET" \
        --tmpfs /tmp --user "$(id -u):$(id -g)" -v "$WS:/workspace:rw" -w /workspace "$@"; }

echo "① 直接の外部通信(プロキシを使わない)は失敗するはず"
if run --entrypoint sh "$IMAGE" -c 'node -e "require(\"https\").get(\"https://example.com\",r=>process.exit(0)).on(\"error\",()=>process.exit(1))"' >/dev/null 2>&1; then ng "直接 example.com に到達できた"; else ok "直接の通信は遮断された"; fi

echo "② 中継役のほかに、外へ出る出口が無い"
others="$(docker network inspect --format '{{range .Containers}}{{.Name}} {{end}}' "$NET" 2>/dev/null | tr ' ' '\n' | grep -vE '^(senn-ai-gw-[0-9a-f-]{8}|senn-ai-[0-9a-f-]{8})?$' || true)"
[ -z "$others" ] && ok "内部ネットワークには、ほかのコンテナがつながっていない" || ng "内部ネットワークに、ほかのコンテナがつながっている($others。以前の版のプロキシなら docker rm -f senn-ai-proxy)"
run --entrypoint sh "$IMAGE" -c 'getent hosts proxy' >/dev/null 2>&1 && ng "プロキシ(proxy)に届く(中継役を通らずに外へ出られる)" || ok "プロキシ(proxy)には届かない"

echo "③ 読み取り専用・特権なし"
run --entrypoint sh "$IMAGE" -c 'touch /etc/x' >/dev/null 2>&1 && ng "ルートファイルシステムに書けた" || ok "ルートは読み取り専用"
run --entrypoint sh "$IMAGE" -c 'grep -q "^CapEff:\s*0000000000000000" /proc/self/status' >/dev/null 2>&1 && ok "ケーパビリティなし" || ng "ケーパビリティが残っている"

echo "④⑤ docker.sock とホストのファイル"
run --entrypoint sh "$IMAGE" -c 'test -e /var/run/docker.sock' >/dev/null 2>&1 && ng "docker.sock が見える" || ok "docker.sock なし"
run --entrypoint sh "$IMAGE" -c 'test -e /Users || test -e /home/'"$USER"'/.ssh || test -e /root/.ssh' >/dev/null 2>&1 && ng "ホストのディレクトリが見える" || ok "ホストのファイルは見えない"
run --entrypoint sh "$IMAGE" -c 'cat /workspace/in-workspace.txt' 2>/dev/null | grep -q secret-marker && ok "作業ディレクトリだけが見える" || ng "作業ディレクトリが見えない"

echo "⑥ 名前解決・ホスト・メタデータの経路"
[ "$(docker network inspect --format '{{.Internal}}' "$NET" 2>/dev/null)" = "true" ] && ok "ネットワーク $NET は内部ネットワーク" || ng "ネットワーク $NET が内部ネットワークではない(internal: true にする)"
run --entrypoint sh "$IMAGE" -c 'getent hosts example.com' >/dev/null 2>&1 && ng "外部の名前が引ける(DNS で外へ情報を出せる)" || ok "外部の名前は引けない"
run --entrypoint sh "$IMAGE" -c 'getent hosts host.docker.internal' >/dev/null 2>&1 && ng "host.docker.internal が引ける(ホストに届く可能性)" || ok "host.docker.internal は引けない"
run --entrypoint sh "$IMAGE" -c 'node -e "require(\"http\").get(\"http://169.254.169.254/\",()=>process.exit(0)).on(\"error\",()=>process.exit(1)).setTimeout(3000,()=>process.exit(1))"' >/dev/null 2>&1 && ng "メタデータのアドレス(169.254.169.254)に届く" || ok "メタデータのアドレスには届かない"
echo "⑦ 中継役(本物のトークンは中継役だけが持つ)"
GW="senn-ai-gw-verify$$"
if CLAUDE_CODE_OAUTH_TOKEN="sk-ant-oat01-verify-fake-$(date +%s)" docker run -d --rm --name "$GW" --read-only --cap-drop=ALL --security-opt no-new-privileges \
     --user "$(id -u):$(id -g)" --network "$EGRESS" -v "$GWDIR:/opt/gateway:ro" -e CLAUDE_CODE_OAUTH_TOKEN -e GATEWAY_PORT=8080 \
     --entrypoint node "$IMAGE" /opt/gateway/gateway.mjs >/dev/null 2>&1 && docker network connect "$NET" "$GW" >/dev/null 2>&1; then
  sleep 2
  code=$(run --entrypoint node "$IMAGE" -e 'fetch("http://'"$GW"':8080/v1/messages",{method:"POST",headers:{"content-type":"application/json","anthropic-version":"2023-06-01"},body:JSON.stringify({model:"m",max_tokens:1,messages:[{role:"user",content:"x"}]})}).then(r=>console.log(r.status)).catch(()=>console.log("ERR"))' 2>/dev/null)
  [ "$code" = "401" ] && ok "中継役を通して Anthropic に届いた(偽のトークンなので 401)" || ng "中継役を通して Anthropic に届かない($code)"
  code=$(run --entrypoint node "$IMAGE" -e 'fetch("http://'"$GW"':8080/v1/messages",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({model:"m",max_tokens:1,messages:[],tools:[{type:"web_fetch_20250910",name:"web_fetch"}]})}).then(r=>console.log(r.status)).catch(()=>console.log("ERR"))' 2>/dev/null)
  [ "$code" = "403" ] && ok "サーバー側の道具(web_fetch)の指定は断られた" || ng "サーバー側の道具の指定が通った($code)"
  code=$(run --entrypoint node "$IMAGE" -e 'fetch("https://api.anthropic.com/v1/messages",{method:"POST"}).then(r=>console.log(r.status)).catch(()=>console.log("BLOCKED"))' 2>/dev/null)
  [ "$code" = "BLOCKED" ] && ok "AI のコンテナから Anthropic に直接は届かない" || ng "AI のコンテナから Anthropic に直接届いた($code)"
  docker rm -f "$GW" >/dev/null 2>&1
else
  docker rm -f "$GW" >/dev/null 2>&1
  ng "中継役を起動できない(ネットワーク $EGRESS / $NET を確認。tools/ai-runner/docker/setup-networks.sh)"
fi
run --entrypoint sh "$IMAGE" -c 'env | grep -q CLAUDE_CODE_OAUTH_TOKEN' >/dev/null 2>&1 && ng "AI のコンテナにトークンの環境変数がある" || ok "AI のコンテナにトークンの環境変数は無い"

rm -rf "$WS"
[ $fail -eq 0 ] && echo "すべて合格" || { echo "不合格があります。ランナーを使わないでください"; exit 1; }
