# Utter

Free, open-source voice-to-text for macOS (Windows later): hold a hotkey, speak, and the text — or its English translation in a business/casual style — is pasted into the focused app. Tauri 2 app: Rust core in `src-tauri/src`, React UI in `src`.

**Start of every session:** read `docs/superpowers/HANDOFF.md` — current status, decisions made along the way, deferred issues, and how to begin the next plan. The spec is `docs/superpowers/specs/2026-10-06-utter-design.md`; plans live in `docs/superpowers/plans/`.

## Current focus (2026-11-13) — when the user says «продолжаем работу над utter»

1. Read `docs/superpowers/HANDOFF.md` («С чего начать следующую сессию»).
2. Plans 1–3 are done; Utter is **public**, v0.2.1 released; «Капля и ИИ-агенты» is merged to `main` (unreleased).
3. **Fast paste is merged to `main`** (2026-11-13; spec `2026-11-10-fast-paste-design.md` §9, summary `plans/2026-11-11-fast-paste-summary.md`): dictation ≤ 0.97 s; translation speed is closed by the user's decision (free Google only, a wait note instead). Agent drop and fast paste are unreleased — a release only with the user's yes.
4. **Claude Code steps live in the island — merged to `main`, unreleased** (2026-11-13; spec `2026-11-13-agent-steps-live-design.md` §8): Utter keeps the plan itself (`agents/plan.rs`), the island shows the ladder. Deferred minors in the spec §8.
5. Dictation on the user's Mac fails since 2026-11-13 with Google's «User location is not supported» — the VPN (incy) path, not Utter (`gemini_location_probe` from the terminal shows it); the user postponed it.

## Working with the user

- Communicate in Russian. The user is not a programmer: explain in plain words, keep jargon out, give exact steps when they must click something.
- The user cares about the product being genuinely good (speed, reliability), not just looking good. Measure before claiming; report honestly what was and was not verified.
- Workflow is superpowers: brainstorming → spec → writing-plans → executing-plans (the user chose Native execution) → final fresh review → finishing-a-development-branch. TDD for every change.
- Commit and `git push` after every task (remote: `Gattapelatta/utter`, private until release). Work on a `feat/*` branch; merge to `main` only with the user's OK.
- In design iterations change only what the user asked for; never alter an approved shape/size as a side effect. Show visual options with the `visualize` widget before building.
- Screen Recording is granted (2026-10-09): `screencapture -x -o -l <window number>` captures the Utter settings window (window numbers via `CGWindowListCopyWindowInfo`; only windows on the current Space). `osascript` keystrokes are still blocked.
- Dev builds (`scripts/dev-bundle.sh`) have the `debug-eval` feature: write JS to `~/Library/Application Support/app.utter.desktop/debug-eval.js` and it runs once in the settings page (click buttons, read state, report via `window.__TAURI_INTERNALS__.invoke("ui_log", {level, message})` → app log). Never enable this feature for releases.
- jsdom has no layout and the hidden browser pane neither paints nor runs rAF: layout/visual bugs (e.g. a zero-height wrapper that clipped onboarding to a black window) only show in the real window — check it with screencapture.

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
- Settings window: one `FloatingDrop` for the whole window flies between `[data-drop-slot]` places (logo / onboarding / about) on the compositor (WAAPI + spring sampled to `linear()`); it holds still (`frozen`) while flying and takes off 60 ms after a page mounts — both measured fixes for skipped frames. Sections open with `reveal.ts` (title from a mask, rows cascade). The page zooms with the window (`settings_window::zoom_for`).
- Motion smoothness is judged by video, not by rAF timestamps: record `screencapture -v -V 3 -R <window rect>` and track the Drop per frame (a `+0` step followed by a double step = a missed frame).
- The built-in browser pane does not run `requestAnimationFrame` while hidden — it cannot judge animations; use Vitest (jsdom) for motion logic and the real app for feel.
- Every rebuild makes macOS ask for Keychain access (self-signed cdhash; user decided to keep it until a Developer ID in Plan 3): warn the user, wait until `SecurityAgent` disappears, then `open Utter.app` again to show the settings window.
- Releases: `scripts/release.sh <version>` on `main` (`--dry-run --allow-branch` builds and checks on a branch, publishes nothing); notes first in `docs/releases/<version>.md`. Publishing and merging only with the user's yes. The user's own Utter now runs from `/Applications` (the released build); dev builds from `target/release` replace it while running.
- README visuals: `scripts/tools/record-overlay.swift` (ScreenCaptureKit, overlay window only — never a screen region), `hero.swift`, `frames2anim.py`, `button.swift`, `label.swift`. Animations for the README are animated PNG: Safari plays a 60 fps animated WebP at ~¼ speed (measured).
- Never kill processes by a loose pattern (`pkill -f …` once closed every app on the user's Mac): only `pkill -x utter` / `killall UtterIM`; any system process only with the user's OK.
- Text Input Sources: a process caches the source list at its first TIS query; `TISEnableInputSource`/`TISDisableInputSource` return 0 without effect for third-party input methods on macOS 15.
- `#[ignore]` tests touch the real Keychain, microphone, or network with the user's API key — run them only with the user's consent.

## Architecture notes that are easy to get wrong

- core-graphics 0.24 event tap: returning `None` from the callback passes the event through; swallow by turning it into a Null event (`hotkey::macos::swallow`).
- Every key event Utter posts carries `SYNTHETIC_MARK` in `EVENT_SOURCE_USER_DATA`; the tap ignores marked events.
- `std::time::Instant` does not advance during sleep on macOS; wake detection uses wall time.
- Never hold a `std::sync::Mutex` guard while calling something that locks the same mutex (a deadlock of this kind was fixed in the orchestrator).
- Provider endpoints and model names live only in `src-tauri/src/providers/defaults.rs`.
- Stopping the event tap must also disable it and invalidate its mach port (`hotkey::macos`): a stale enabled tap stalled the whole keyboard after many sleep/wake restarts.
- Motion cannot interpolate between transform strings with different function lists (e.g. `scaleX() scaleY()` → `none` animates to scale 0): keep one template (`overlay/drop/islandMotion.ts`). This includes `transitionEnd: { transform: "none" }` — the next animation then starts from scale 0 (the island "vanished" on fold-back).
- The top ~32 pt of the screen under the MacBook camera notch are not pixels: nothing in the island may be drawn there (the Drop sits lower, arcs are limited to ±30°, clouds are in the island's left wing).
- Gemini Live (`gemini-3.5-transcribe-live`, WebSocket `BidiGenerateContent`): send `setup` first and audio only after `setupComplete` (buffer before); interim text arrives in `serverContent.interimInputTranscription` (the whole current piece, until a pause), finals in `serverContent.inputTranscription` (~0.4 s after `audioStreamEnd`); it sends **no `turnComplete`** after the stream ends; `SMART` mode cleans fillers but once wrote «по продам» for «по продажам»; `VERBATIM` is exact. Without the user's VPN Google answers «User location is not supported».
- Live processing (`live/process.rs`, spec 2026-11-10): pieces go at once within a shared per-model budget (`providers/budget.rs`, 14/min, 3 kept for the end; agents share it); Dictation with cleanup pastes ≤ 950 ms after release, unanswered pieces as heard minus filler sounds (`pipeline/fillers.rs`); Google's drafts lack the last word, so a guess may miss ≤ 3 final words; translation makes no guesses and races the whole text on three models after 2 s (limit 5 s, then the batch path). No re-sends while speaking — they emptied the budget.
- Live path: every Gemini dictation without paragraphs goes live, also while earlier jobs are processed (user's decision 2026-11-07, checked live) — the job id is taken at release so the ordered queue keeps insertion order; a draft window is closed only by its own dictation (`draft_owner`, `live_ids` in the orchestrator), and the page drops drafts tagged with an earlier recording (`overlay/visibleDraft.ts`).
- The live draft is shown in the island (420 × 206 window), not in the app's field: an input method worked (fields connected in 11–16 ms) but macOS 15 enables third-party input methods only by the user's hand and lists them in the language switcher (rejected by the user 2026-11-06). Removing an *enabled* input method's bundle leaves a stale entry that keeps mapping its path to the old id — the user must remove it in Settings first.
- Dev builds: `<app data>/debug-draft.txt` with a phrase plays recording → draft window → processing → done on the real island (for `screencapture -l` checks without a dictation).
- macOS on-device speech recognition (`SFSpeechRecognizer`, Russian supported) fails with «Siri and Dictation are disabled» unless system Dictation is on — rejected for Utter.
- Synthetic key presses from a helper process do not reach Utter's hotkey tap: any live test that needs a dictation needs the user's hands. Each rebuild of a helper app re-asks Keychain access (cdhash); Accessibility survives if it is signed with "Utter Dev" and keeps its bundle id; apps in `/private/tmp` cannot be added to Accessibility — put helpers in `~/Applications`.
- The Drop's per-frame logic is a pure model (`overlay/drop/dropModel.ts`) driving SVG attributes via refs — no React re-render per frame.
- Starting an opacity/transform animation on an element without its own layer mid-animation costs a frame in WebKit (draft window, 2026-11-07): give such elements `will-change` up front.
- WebKit does not hand a `linear(…)` easing to the compositor: it resolves it on the main thread every frame (measured 2 % CPU vs 0.01 % for the same spring as keyframes). For anything that runs while idle, give springs as sampled keyframes (`overlay/drop/restMotion.ts`).
- Agents: Claude Code hooks (`~/.claude/settings.json`) fire live, also in an open session; `UserPromptSubmit` stdout becomes context, so `utter-agent` prints only the permission decision. Tauri bundles every `[[bin]]` into `Contents/MacOS`, but a universal build lipos only the main binary — `scripts/release.sh` lipos `utter-agent` itself.
- The overlay window must not take clicks where a full-screen app shows its menu and title bar (the top strip, and the open island lying over it): macOS then never hides them. It takes clicks only over the hit zones the island page reports (`agent_hit_zones`) and never in the top 36 pt (`agents::service::takes_clicks`).
- Hover over the island without the user's hand: Utter polls the cursor position, so `CGWarpMouseCursorPosition` to the ears (overlay window centre x, y ≈ 12) opens it — put the pointer back afterwards. The desktop app calls `wrap_up` at the end of every turn; Utter closes the plan only for a `wrap_up` without `ask` (a question is a pause) — check the ladder mid-turn.
- The overlay window stays on screen while idle (the Drop in the notch, §4.2a): `app/overlay_rest.rs` decides it; it is ordered out only when the ears are off (setting, full-screen app, locked, display asleep). Synthetic key presses from a helper do not reach Utter — a dictation for a video check needs the user.
