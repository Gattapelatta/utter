# Utter

Free, open-source voice-to-text for macOS (Windows later): hold a hotkey, speak, and the text — or its English translation in a business/casual style — is pasted into the focused app. Tauri 2 app: Rust core in `src-tauri/src`, React UI in `src`.

**Start of every session:** read `docs/superpowers/HANDOFF.md` — current status, decisions made along the way, deferred issues, and how to begin the next plan. The spec is `docs/superpowers/specs/2026-10-06-utter-design.md`; plans live in `docs/superpowers/plans/`.

## Working with the user

- Communicate in Russian. The user is not a programmer: explain in plain words, keep jargon out, give exact steps when they must click something.
- The user cares about the product being genuinely good (speed, reliability), not just looking good. Measure before claiming; report honestly what was and was not verified.
- Workflow is superpowers: brainstorming → spec → writing-plans → executing-plans (the user chose Native execution) → final fresh review → finishing-a-development-branch. TDD for every change.
- Commit and `git push` after every task (remote: `Gattapelatta/utter`, private until release). Work on a `feat/*` branch; merge to `main` only with the user's OK.
- In design iterations change only what the user asked for; never alter an approved shape/size as a side effect. Show visual options with the `visualize` widget before building.
- You cannot see the user's screen: `screencapture` returns only the wallpaper (no Screen Recording permission) and `osascript` keystrokes are blocked. Verify UI behaviour with logs/instrumentation in the real app, and ask the user for short live checks.

## Commands

```bash
export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0
cargo test --manifest-path src-tauri/Cargo.toml --lib      # Rust tests
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets   # keep at 0 warnings
pnpm test                                                   # UI tests (Vitest)
pkill -x utter; scripts/dev-bundle.sh                       # signed release build for the user
open src-tauri/target/release/bundle/macos/Utter.app
```

- `scripts/dev-bundle.sh` signs with the local "Utter Dev" certificate so macOS permissions survive rebuilds — always use it for builds the user tests.
- App log: `~/Library/Logs/app.utter.desktop/utter.log`. `timing:` lines show per-stage latency (`show→pill painted`, `start→first sample`, `audio→text`, `delivered … total_ms`, `overlay frames while listening`); `piece sent while speaking`, `overlay placed … notch=`, `job failed`, `http error` explain the rest.
- Pill design reference (approved by the user): `docs/design/drop-prototype.html`; spec §4.2/§5.4 override it where they differ (size/position under the notch, arcs, clouds).
- The built-in browser pane does not run `requestAnimationFrame` while hidden — it cannot judge animations; use Vitest (jsdom) for motion logic and the real app for feel.
- `#[ignore]` tests touch the real Keychain, microphone, or network with the user's API key — run them only with the user's consent.

## Architecture notes that are easy to get wrong

- core-graphics 0.24 event tap: returning `None` from the callback passes the event through; swallow by turning it into a Null event (`hotkey::macos::swallow`).
- Every key event Utter posts carries `SYNTHETIC_MARK` in `EVENT_SOURCE_USER_DATA`; the tap ignores marked events.
- `std::time::Instant` does not advance during sleep on macOS; wake detection uses wall time.
- Never hold a `std::sync::Mutex` guard while calling something that locks the same mutex (a deadlock of this kind was fixed in the orchestrator).
- Provider endpoints and model names live only in `src-tauri/src/providers/defaults.rs`.
- Stopping the event tap must also disable it and invalidate its mach port (`hotkey::macos`): a stale enabled tap stalled the whole keyboard after many sleep/wake restarts.
- Motion cannot interpolate between transform strings with different function lists (e.g. `scaleX() scaleY()` → `none` animates to scale 0): keep one template (`overlay/drop/islandMotion.ts`).
- The top ~32 pt of the screen under the MacBook camera notch are not pixels: nothing in the island may be drawn there (the Drop sits lower, arcs are limited to ±30°, clouds are in the island's left wing).
- The Drop's per-frame logic is a pure model (`overlay/drop/dropModel.ts`) driving SVG attributes via refs — no React re-render per frame.
