# Utter

Free, open-source voice-to-text for macOS (Windows later): hold a hotkey, speak, and the text — or its English translation in a business/casual style — is pasted into the focused app. Tauri 2 app: Rust core in `src-tauri/src`, React UI in `src`.

**Start of every session:** read `docs/superpowers/HANDOFF.md` — current status, decisions made along the way, deferred issues, and how to begin the next plan. The spec is `docs/superpowers/specs/2026-10-06-utter-design.md`; plans live in `docs/superpowers/plans/`.

## Working with the user

- Communicate in Russian. The user is not a programmer: explain in plain words, keep jargon out, give exact steps when they must click something.
- The user cares about the product being genuinely good (speed, reliability), not just looking good. Measure before claiming; report honestly what was and was not verified.
- Workflow is superpowers: brainstorming → spec → writing-plans → executing-plans (the user chose Native execution) → final fresh review → finishing-a-development-branch. TDD for every change.
- Commit and `git push` after every task (remote: `Gattapelatta/utter`, private until release). Work on a `feat/*` branch; merge to `main` only with the user's OK.

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
- App log: `~/Library/Logs/app.utter.desktop/utter.log` (`timing:` lines show per-stage latency).
- `#[ignore]` tests touch the real Keychain, microphone, or network with the user's API key — run them only with the user's consent.

## Architecture notes that are easy to get wrong

- core-graphics 0.24 event tap: returning `None` from the callback passes the event through; swallow by turning it into a Null event (`hotkey::macos::swallow`).
- Every key event Utter posts carries `SYNTHETIC_MARK` in `EVENT_SOURCE_USER_DATA`; the tap ignores marked events.
- `std::time::Instant` does not advance during sleep on macOS; wake detection uses wall time.
- Never hold a `std::sync::Mutex` guard while calling something that locks the same mutex (a deadlock of this kind was fixed in the orchestrator).
- Provider endpoints and model names live only in `src-tauri/src/providers/defaults.rs`.
