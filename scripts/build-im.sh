#!/bin/bash
# Builds Utter's input method (macos/UtterIM) and signs it with the local "Utter Dev"
# certificate. Output: macos/UtterIM/build/UtterLive.app (bundled into Utter.app/Contents/Resources).
set -euo pipefail
cd "$(dirname "$0")/../macos/UtterIM"
APP=build/UtterLive.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp Info.plist "$APP/Contents/Info.plist"
cp Resources/* "$APP/Contents/Resources/"
swiftc -O -module-name UtterIM -target arm64-apple-macos13.0 \
  -framework Cocoa -framework InputMethodKit -framework Security \
  Sources/*.swift -o "$APP/Contents/MacOS/UtterIM"
codesign --force --sign "Utter Dev" --identifier app.utter.inputmethod.live "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Identifier|Authority"
echo "Built and signed: macos/UtterIM/$APP"
