#!/bin/bash
# install.sh — ランナーを、ログイン時に自動で起動する常駐サービス(LaunchAgent)として登録する。
# 事前に: ~/.senn-ai-runner/runner.env と claude.env を作り(権限 600)、サンドボックスのイメージをビルドしておく。
#   ./launchd/install.sh            登録して起動
#   ./launchd/install.sh --dry-run  登録せず、点検とファイルの生成内容の確認だけ
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LABEL=com.macplanning.senn-ai-runner
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
NODE="$(command -v node)"; [ -n "$NODE" ] || { echo "node が見つかりません"; exit 1; }

bash "$ROOT/launchd/run-runner.sh" --check
mkdir -p "$HOME/.senn-ai-runner/logs" "$HOME/Library/LaunchAgents"
chmod 700 "$HOME/.senn-ai-runner"

OUT="$PLIST"; [ "${1:-}" = "--dry-run" ] && OUT="$(mktemp)"
sed -e "s#__ROOT__#$ROOT#g" -e "s#__HOME__#$HOME#g" -e "s#__NODE__#$NODE#g" -e "s#__NODE_DIR__#$(dirname "$NODE")#g" \
  "$ROOT/launchd/$LABEL.plist.template" > "$OUT"
plutil -lint "$OUT"
if [ "${1:-}" = "--dry-run" ]; then echo "[dry-run] 生成した内容(登録はしていません):"; cat "$OUT"; rm -f "$OUT"; exit 0; fi

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
echo "登録しました。ログ: $HOME/.senn-ai-runner/logs/runner.log"
echo "状態: launchctl print gui/$(id -u)/$LABEL | head"
echo "停止(一時): touch $HOME/.senn-ai-runner/STOP   /   解除: rm $HOME/.senn-ai-runner/STOP"
echo "登録の解除: $ROOT/launchd/uninstall.sh"
