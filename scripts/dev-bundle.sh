#!/bin/bash
# Builds the debug app bundle and signs it with the local "Utter Dev" certificate,
# so macOS keeps Accessibility/Microphone permissions across rebuilds.
# The certificate is a self-signed, local-only development identity.
# Usage: scripts/dev-bundle.sh [--debug]   (release by default: realistic speed)
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "${1:-}" = "--debug" ]; then
  pnpm tauri build --debug --bundles app --features debug-eval
  APP=src-tauri/target/debug/bundle/macos/Utter.app
else
  pnpm tauri build --bundles app --features debug-eval
  APP=src-tauri/target/release/bundle/macos/Utter.app
fi
codesign --force --deep --sign "Utter Dev" --identifier app.utter.desktop "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Identifier|Authority"
echo "Built and signed: $APP"
