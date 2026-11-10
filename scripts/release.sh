#!/bin/bash
# Releases Utter for Mac from this Mac: checks → universal build → "Utter Dev" signature → .dmg + update
# archive (signed with ~/.tauri/utter.key) → self-check → version commit + tag → GitHub release.
# The version is committed only after everything is built and checked (final review I2).
# Usage: scripts/release.sh <version> [--dry-run] [--allow-branch]
#   --dry-run       build and check everything in release-out/v<version>/, change and publish nothing
#   --allow-branch  only with --dry-run: try it from a feature branch
# Developer ID later: set APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID (not done yet).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0

if [ $# -lt 1 ]; then echo "Как запускать: scripts/release.sh 0.2.0 [--dry-run]"; exit 2; fi
VERSION_ARG="$1"; shift
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
VERSION=$(node -e 'import("./scripts/release/version.ts").then(m => console.log(m.parseVersion(process.argv[1]))).catch(e => { console.error(e.message); process.exit(2) })' "$VERSION_ARG")
OUT="release-out/v$VERSION"
NOTES="docs/releases/$VERSION.md"
VERSION_FILES=(src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock package.json)
BUMPED=0; COMMITTED=0; MOUNT=""; CHECK=""

cleanup() {
  [ -n "$MOUNT" ] && hdiutil detach -quiet "$MOUNT" 2>/dev/null || true
  [ -n "$CHECK" ] && rm -rf "$CHECK"
  if [ "$BUMPED" = 1 ] && [ "$COMMITTED" = 0 ]; then
    git checkout -- "${VERSION_FILES[@]}" 2>/dev/null || true
    echo "Версия в файлах возвращена как была — можно запускать снова."
  fi
}
trap cleanup EXIT

step "Проверки перед выпуском $VERSION"
node scripts/release/main.ts facts "$VERSION" $ALLOW
step "Тесты"
cargo test --manifest-path src-tauri/Cargo.toml --lib --quiet
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --quiet -- -D warnings
pnpm test --silent

if [ "$DRY" = 0 ]; then
  step "Версия $VERSION (сохранится после проверки файлов)"
  BUMPED=1
  node scripts/release/main.ts bump "$VERSION"
  cargo update --manifest-path src-tauri/Cargo.toml --package utter --quiet
fi

step "Сборка для всех Mac (Apple Silicon + Intel)"
# Tauri bundles every binary of the crate, but for a universal build it glues (lipo) only the main
# one: utter-agent must already be in the universal folder when the bundler copies it.
for T in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --manifest-path src-tauri/Cargo.toml --release --bin utter-agent --target "$T" --quiet
done
mkdir -p src-tauri/target/universal-apple-darwin/release
lipo -create -output src-tauri/target/universal-apple-darwin/release/utter-agent \
  src-tauri/target/aarch64-apple-darwin/release/utter-agent src-tauri/target/x86_64-apple-darwin/release/utter-agent
pnpm tauri build --target universal-apple-darwin --bundles app
BUNDLE=src-tauri/target/universal-apple-darwin/release/bundle/macos
APP="$BUNDLE/Utter.app"

step "Подпись «Utter Dev»"
codesign --force --deep --sign "Utter Dev" --identifier app.utter.desktop "$APP"

step "Файлы выпуска в $OUT"
rm -rf "$OUT"; mkdir -p "$OUT/dmg"
ditto "$APP" "$OUT/dmg/Utter.app"
ln -s /Applications "$OUT/dmg/Applications"
DMG="$OUT/Utter_${VERSION}_universal.dmg"
hdiutil create -quiet -volname "Utter" -srcfolder "$OUT/dmg" -format UDZO -ov "$DMG"
rm -rf "$OUT/dmg"
# No AppleDouble "._*" entries: the installed Utter's updater would fail on them (final review C1).
COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs -czf "$OUT/Utter.app.tar.gz" -C "$BUNDLE" Utter.app
pnpm tauri signer sign --private-key-path "$HOME/.tauri/utter.key" --password "" "$OUT/Utter.app.tar.gz" > /dev/null
node scripts/release/main.ts latest "$VERSION" "$OUT/Utter.app.tar.gz.sig" "$NOTES" "$OUT/latest.json"

step "Самопроверка файлов"
CHECK=$(mktemp -d)
tar --no-mac-metadata -xzf "$OUT/Utter.app.tar.gz" -C "$CHECK"
node scripts/release/main.ts verify "$CHECK/Utter.app" "$OUT/Utter.app.tar.gz" "$OUT/Utter.app.tar.gz.sig"
MOUNT=$(mktemp -d)
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$DMG"
node scripts/release/main.ts verify "$MOUNT/Utter.app" "$OUT/Utter.app.tar.gz" "$OUT/Utter.app.tar.gz.sig"
hdiutil detach -quiet "$MOUNT"; MOUNT=""

if [ "$DRY" = 1 ]; then
  step "Пробный выпуск готов — ничего не изменено и не опубликовано"
  ls -lh "$OUT"
  exit 0
fi

step "Версия $VERSION — коммит и метка"
git add "${VERSION_FILES[@]}"
git commit -q -m "release: v$VERSION"
COMMITTED=1
git tag "v$VERSION"

step "Публикация на GitHub"
if ! git push -q --atomic origin main "v$VERSION"; then
  echo "Не удалось отправить на GitHub (сеть?). Файлы целы в $OUT. Когда сеть вернётся:"
  echo "  git push --atomic origin main v$VERSION"
  echo "  gh release create v$VERSION $OUT/* --title \"Utter $VERSION\" --notes-file $NOTES"
  exit 1
fi
if ! gh release create "v$VERSION" "$DMG" "$OUT/Utter.app.tar.gz" "$OUT/latest.json" --title "Utter $VERSION" --notes-file "$NOTES"; then
  echo "Метка уже на GitHub, а выпуск не создался. Посмотрите: gh release view v$VERSION"
  echo "  выпуска нет:      gh release create v$VERSION $DMG $OUT/Utter.app.tar.gz $OUT/latest.json --title \"Utter $VERSION\" --notes-file $NOTES"
  echo "  есть черновик:    gh release upload v$VERSION $DMG $OUT/Utter.app.tar.gz $OUT/latest.json --clobber && gh release edit v$VERSION --draft=false"
  exit 1
fi
echo "Готово: https://github.com/Gattapelatta/utter/releases/tag/v$VERSION"
