#!/bin/bash
# set-env.sh — 秘密の値を、画面に出さずに ~/.senn-ai-runner/runner.env へ保存する(チャットに貼らない・シェルの履歴に残さない)。
#   使い方: ./launchd/set-env.sh RUNNER_TOKEN [RUNNER_SIGNING_SECRET ...]
# 値は、入力中に表示されません(貼り付けて Enter)。既に同じ名前の行があれば置き換えます。
set -eu
FILE="${RUNNER_HOME:-$HOME/.senn-ai-runner}/runner.env"
[ "$#" -ge 1 ] || { echo "使い方: $0 NAME [NAME ...]"; exit 1; }
mkdir -p "$(dirname "$FILE")"; chmod 700 "$(dirname "$FILE")"
touch "$FILE"; chmod 600 "$FILE"
for NAME in "$@"; do
  case "$NAME" in *[!A-Z0-9_]*|"") echo "名前が不正です: $NAME"; exit 1;; esac
  printf '%s の値を貼り付けて Enter(表示されません): ' "$NAME"
  IFS= read -rs VALUE; echo
  [ -n "$VALUE" ] || { echo "空の値は保存しません"; exit 1; }
  case "$VALUE" in *[[:space:]\'\"\$\`\\]*) echo "空白や引用符を含む値は保存できません(貼り付けを確認してください)"; exit 1;; esac
  TMP="$(mktemp)"; chmod 600 "$TMP"
  grep -v "^export $NAME=" "$FILE" > "$TMP" || true
  printf 'export %s=%s\n' "$NAME" "$VALUE" >> "$TMP"
  mv "$TMP" "$FILE"; chmod 600 "$FILE"
  echo "保存しました: $NAME(${#VALUE} 文字)"
done
