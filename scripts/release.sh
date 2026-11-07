#!/bin/bash
# Releases Utter for Mac from this Mac: checks → version → universal build → "Utter Dev" signature →
# .dmg + update archive (signed with ~/.tauri/utter.key) → self-check → GitHub release.
# Usage: scripts/release.sh <version> [--dry-run] [--allow-branch]
#   --dry-run       build and check everything in dist/v<version>/, change and publish nothing
#   --allow-branch  only with --dry-run: try it from a feature branch
# Developer ID later: set APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID (not done yet).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0

VERSION_ARG="${1:-}"; shift || true
DRY=0; ALLOW=""
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    --allow-branch) ALLOW="--allow-branch" ;;
    *) echo "Неизвестный параметр: $a"; exit 2 ;;
  esac
done
if [ -n "$ALLOW" ] && [ "$DRY" = 0 ]; then echo "--allow-branch — только вместе с --dry-run"; exit 2; fi
if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "Подпись Developer ID ещё не настроена в этом скрипте (План 3: только подготовка)."; exit 2
fi

step() { printf '\n\033[1m▸ %s\033[0m\n' "$1"; }
VERSION=$(node -e 'import("./scripts/release/version.ts").then(m => console.log(m.parseVersion(process.argv[1])))' "$VERSION_ARG")
OUT="dist/v$VERSION"
NOTES="docs/releases/$VERSION.md"

step "Проверки перед выпуском $VERSION"
node scripts/release/main.ts facts "$VERSION" $ALLOW
step "Тесты"
cargo test --manifest-path src-tauri/Cargo.toml --lib --quiet
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --quiet -- -D warnings
pnpm test --silent

if [ "$DRY" = 0 ]; then
  step "Версия $VERSION"
  node scripts/release/main.ts bump "$VERSION"
  cargo update --manifest-path src-tauri/Cargo.toml --package utter --quiet
  git add src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock package.json
  git commit -q -m "release: v$VERSION"
fi

step "Сборка для всех Mac (Apple Silicon + Intel)"
pnpm tauri build --target universal-apple-darwin --bundles app
BUNDLE=src-tauri/target/universal-apple-darwin/release/bundle/macos
APP="$BUNDLE/Utter.app"

step "Подпись «Utter Dev»"
codesign --force --deep --sign "Utter Dev" --identifier app.utter.desktop "$APP"

step "Файлы выпуска в $OUT"
rm -rf "$OUT"; mkdir -p "$OUT/dmg"
cp -R "$APP" "$OUT/dmg/"
ln -s /Applications "$OUT/dmg/Applications"
DMG="$OUT/Utter_${VERSION}_universal.dmg"
hdiutil create -quiet -volname "Utter" -srcfolder "$OUT/dmg" -format UDZO -ov "$DMG"
rm -rf "$OUT/dmg"
tar -czf "$OUT/Utter.app.tar.gz" -C "$BUNDLE" Utter.app
pnpm tauri signer sign --private-key-path "$HOME/.tauri/utter.key" --password "" "$OUT/Utter.app.tar.gz" > /dev/null
node scripts/release/main.ts latest "$VERSION" "$OUT/Utter.app.tar.gz.sig" "$NOTES" "$OUT/latest.json"

step "Самопроверка файлов"
CHECK=$(mktemp -d)
tar -xzf "$OUT/Utter.app.tar.gz" -C "$CHECK"
node scripts/release/main.ts verify "$CHECK/Utter.app" "$OUT/Utter.app.tar.gz" "$OUT/Utter.app.tar.gz.sig"
MOUNT=$(mktemp -d)
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$DMG"
node scripts/release/main.ts verify "$MOUNT/Utter.app" "$OUT/Utter.app.tar.gz" "$OUT/Utter.app.tar.gz.sig" || { hdiutil detach -quiet "$MOUNT"; exit 1; }
hdiutil detach -quiet "$MOUNT"
rm -rf "$CHECK"

if [ "$DRY" = 1 ]; then
  step "Пробный выпуск готов — ничего не опубликовано"
  ls -lh "$OUT"
  exit 0
fi

step "Публикация на GitHub"
git tag "v$VERSION"
git push -q origin main "v$VERSION"
gh release create "v$VERSION" "$DMG" "$OUT/Utter.app.tar.gz" "$OUT/latest.json" \
  --title "Utter $VERSION" --notes-file "$NOTES"
echo "Готово: https://github.com/Gattapelatta/utter/releases/tag/v$VERSION"
