#!/bin/bash
# uninstall.sh — 常駐サービスの登録を解除する(設定ファイルは消さない)。
set -u
LABEL=com.macplanning.senn-ai-runner
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null && echo "停止しました" || echo "登録されていません"
rm -f "$HOME/Library/LaunchAgents/$LABEL.plist"
echo "登録を解除しました(ネットワークは、必要なら docker network rm senn-ai-internal senn-ai-egress で消します)"
