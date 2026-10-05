#!/bin/bash
# Semi-automatic paste check in TextEdit.
#
# Needs a debug app bundle:   pnpm tauri build --debug --bundles app
# and, once per rebuild, Accessibility permission for that Utter.app
# (System Settings → Privacy & Security → Accessibility) plus Automation
# permission for TextEdit when macOS asks.
#
# Usage: scripts/paste-matrix.sh [path/to/Utter.app]
# Switch the keyboard layout (RU / EN) yourself and run the script once per layout.
set -u
APP="${1:-src-tauri/target/debug/bundle/macos/Utter.app}"
[ -d "$APP" ] || { echo "No app bundle at $APP"; exit 2; }

cases=(
  "Hello, world."
  "Привет, мир. Как дела?"
  "Emoji 😀👍🏽 and symbols: € ½ — «quotes»"
  $'Line one\nLine two\n\nParagraph two'
  "A long sentence that is definitely longer than twenty characters, to exercise chunked typing."
)

pass=0; fail=0
for method in paste type; do
  for text in "${cases[@]}"; do
    osascript -e 'tell application "TextEdit" to activate' \
              -e 'tell application "TextEdit" to make new document' >/dev/null
    sleep 0.5
    open -W -n "$APP" --args --debug-deliver "$text" --method "$method" --delay 1
    sleep 0.5
    got=$(osascript -e 'tell application "TextEdit" to get text of document 1')
    # Shift+Return in TextEdit inserts U+2028 LINE SEPARATOR; treat it as a newline.
    got=$(printf '%s' "$got" | perl -CS -pe 's/\x{2028}/\n/g')
    if [ "$got" == "$text" ]; then
      echo "PASS  [$method] ${text:0:40}"; pass=$((pass+1))
    else
      echo "FAIL  [$method] expected: ${text:0:60}"; echo "             got: ${got:0:60}"; fail=$((fail+1))
    fi
    osascript -e 'tell application "TextEdit" to close document 1 saving no' >/dev/null
  done
done
echo "--- $pass passed, $fail failed"
[ "$fail" -eq 0 ]
