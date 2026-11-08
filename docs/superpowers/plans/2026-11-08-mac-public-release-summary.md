# Plan 3 — public Mac release: summary (2026-11-08)

Spec `docs/superpowers/specs/2026-11-07-mac-public-release-design.md`, plan `2026-11-07-mac-public-release.md`. Executed natively (executing-plans); the git-ignored ledger is deleted, so its decisions live here.

## Outcome

- **v0.2.0** (2026-11-08, tag `v0.2.0`): first public release. The user installed it from the page via Safari into /Applications (Gatekeeper «Open Anyway»), dictation works.
- **Repository public** 2026-11-08 with the user's yes (history scanned first: only synthetic test keys).
- **v0.2.1** (tag `v0.2.1`): `LSMinimumSystemVersion` 13.0 (was 10.13 while the README says 13+), enforced by the release self-check. **Live auto-update 0.2.0 → 0.2.1 passed**: found, installed, restarted; same signature (leaf 5439704…), dictation and paste work after it (3 dictations, 1.1–1.4 s).
- Release: `scripts/release.sh <version> [--dry-run] [--allow-branch]` on `main`, from this Mac; notes in `docs/releases/<version>.md`; files in `release-out/v<version>/` (git-ignored).
- Certificate and updater key: both backed up by the user (where — only in the assistant's private memory, not in this public repo).

## README (redesigned with the user 2026-11-08)

- `README.md` is Russian (most visitors from Russia), `README.en.md` English; text in the user's own conversational voice.
- Top: language switch (grey/white pictures, `scripts/tools/label.swift`; GitHub paints text links blue), one hero picture, a dark download button (`scripts/tools/button.swift`, text as glyph outlines).
- Hero = **animated PNG** (`docs/media/hero-{ru,en}.png`): island recorded without a backdrop (`record-overlay.swift --no-backdrop`, ScreenCaptureKit, overlay window only) on the left of the user's gradient (`docs/design/hero-background.jpg`), the word «Utter» on the right in Helvetica Neue Medium with overlapping letters whose contours show over each other (`scripts/tools/hero.swift`), encoded by `scripts/tools/frames2anim.py` (Pillow).
- Measured by screen-recording Safari: a 60 fps animated WebP played at ~¼ speed with 4 s freezes; the APNG loops at the file's own 7.18 s. GIF (256 colours, 0.75×) looked blurry. Floating text beside an image broke on GitHub (a heading rule crossed the image) — hence one picture.
- `<picture>` light/dark variants follow the viewer's macOS appearance on GitHub, not GitHub's own theme: on a light-mode Mac with dark GitHub the «current language» label shows dark. The user chose to leave it.

## Rulings made during execution

- Task 2: launching the app from the .dmg moved to the install-as-a-new-person step (avoids an extra Keychain prompt) — cost if wrong: a dmg launch problem found later.
- Task 3: no new `Activity` state; `tray::current_status` reads the tray's AppStatus — cost if wrong: without a tray the update installs at once.
- Task 3: the tray shows no «will install after dictation» item; About shows the waiting text — cost if wrong: the tray says nothing for a few seconds.
- Task 4: Android/iOS icons and 64×64 removed (no mobile targets).
- Task 5: recordings come from Utter's overlay window only (ScreenCaptureKit), never a screen region — a region would leak whatever is on the user's screen.
- Final review: release files in `release-out/` (Vite wipes `dist/`).
- Final review: declined lines (Developer ID, Windows, Keychain prompt per update, updater key without password, Intel/macOS 13–14 runtime, CONTRIBUTING, public test keys) — user decisions or out of scope.
- README: animated PNG instead of WebP/GIF (measured above) — cost: ~2.6 MB per hero image.

## Deferred minors (from the final review)

- A recording started in the ~1 s between the second idle poll and `app.restart()` is cut; the download is lost if Utter quits while waiting (alternative: install first, wait for idle, then restart).
- `latest.json` version/url/signature not re-checked against the files (built from the same constants).
- CI: no `permissions: contents: read`; actions pinned by tag, not SHA.
- ~~No `minimumSystemVersion`~~ — fixed in 0.2.1.

## Not verified

Intel Macs and macOS 13–14 at runtime (built universal, tested on Apple Silicon, macOS 15). Every update asks once for Keychain access (self-signed «Utter Dev»; goes away with a Developer ID).
