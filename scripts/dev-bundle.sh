#!/bin/bash
# Builds the debug app bundle and signs it with the local "Utter Dev" certificate,
# so macOS keeps Accessibility/Microphone permissions across rebuilds.
# The certificate is a self-signed, local-only development identity.
# Usage: scripts/dev-bundle.sh [--debug]   (release by default: realistic speed)
set -euo pipefail
cd "$(dirname "$0")/.."
# The input method is bundled as Contents/Resources/UtterLive.app (tauri.conf.json).
scripts/build-im.sh
if [ "${1:-}" = "--debug" ]; then
  pnpm tauri build --debug --bundles app --features debug-eval
  APP=src-tauri/target/debug/bundle/macos/Utter.app
else
  pnpm tauri build --bundles app --features debug-eval
  APP=src-tauri/target/release/bundle/macos/Utter.app
fi
# Not --deep: the input method keeps its own identifier (app.utter.inputmethod.live).
codesign --force --sign "Utter Dev" --identifier app.utter.desktop "$APP"
codesign --verify --deep --strict "$APP"
codesign -dvv "$APP" 2>&1 | grep -E "Identifier|Authority"
codesign -dv "$APP/Contents/Resources/UtterLive.app" 2>&1 | grep -E "Identifier"
echo "Built and signed: $APP"
