# Live dictation — decisions taken during execution

Collected from the execution ledgers of `2026-11-04-live-dictation.md` (Tasks 0–3) and `2026-11-06-live-draft-island.md` (all tasks), 2026-11-07. Each line: the decision — why — what it costs if wrong. The ledgers themselves were scratch files and are deleted.

## Rulings

- Task 3: Ruling: the field leaves via AnimatePresence `exit` (not a `leaving` prop); the timer stays in its slot frozen and fades with `timerLeaveTarget()` (slot is never removed), so the test checks slot opacity 0 instead of removal — keeps "the slot exists in every state".
- Task 3: Ruling: long-draft test asserts `scrollTop === scrollHeight` (the browser clamps; jsdom does not); scrolling uses CSS `scroll-behavior: smooth`.
- Task 3: Ruling: the field sits inside a wrapper with the island's own animated clip-path — the burst capture showed a fading field hanging below the shrinking island; test `the field is clipped by the island's shape`.
- Task 3: Ruling: after the window, "Готово" plays at the Drop's usual island place (left of the pair), as in every Done — spec's «в центр» read as its island place.
- Task 4: Ruling: a Live API error before release does not report at once: the runner keeps the window's draft, ignores further audio and returns `Fallback` on release (the batch path needs the whole recording); a failed connect likewise waits for release/cancel.
- Task 4: Ruling: throttle/translator times use `tokio::time::Instant::now().into_std()` so paused-clock tests drive them.
- Task 4: Ruling: `error_of("timeout")` → `ProviderError::Timeout`, any other reason → `BadResponse(reason)` (Task 5 maps «location is not supported» → region_blocked).
- Task 5: Ruling: `Deps.live` is `Option<Arc<dyn LiveConnector>>` (plan: plain Arc) — tests and non-Gemini setups keep the batch path with `None`.
- Task 5: Ruling: live also requires `paragraphs` off (paragraphs split the whole text by meaning, as `check_pieces` already rules) and a recorder that reports its format via `peek(0)`.
- Task 5: Ruling: `live_done(Final)` re-enters through `Input::JobDone(id, …)` so ordering, usage and history go through the one existing path; `Fallback` reuses the id via a new `run_job` (split out of `spawn_job`).
- Task 5: Ruling: usage counts a live dictation as 1 + text requests in `JobResult.requests` (the extra ones land in the speech-request counter) — cost if wrong: the counters label text requests as speech requests.
- Task 5: Ruling: the runner clears the island's draft only on cancel; the orchestrator clears it after showing the result (`finish`) and on Esc — so a mid-speech failure keeps the window until the batch text is pasted (test `mid_speech_drop_falls_back_keeping_window`).
- Task 5: Ruling: `ProviderError::Timeout` now shows `slow_provider` everywhere (was `network`); `BadResponse` with «location is not supported» → `region_blocked`.
- Task 0: Ruling: dictation with cleanup off (no LLM step) keeps one transcription request, no routes — every route would be the same gemini-3.5-transcribe call — cost if wrong: no backup for cleanup-off users when that model is slow.
- Task 0: Ruling: a Direct route runs `pipeline::run_audio_only` (no in-route fallback to a plain transcript); the next route is the fallback — cost if wrong: on the last route a 429 gives an error instead of a degraded transcript.
- Task 0: Ruling: "nothing heard" from a route is an answer and ends the race — cost if wrong: a glitchy empty reply shows "nothing heard" instead of trying a backup.
- Task 0: Ruling: RouteHealth lives in `Arc<Mutex<_>>` on the orchestrator (shared by jobs and pieces, survives provider rebuilds), not a plain field — pieces run in spawned tasks.
- Task 0: Ruling: Gemini route clients share one reqwest pool (`HttpProviders.gemini_http`) so a hedge starts on a warm connection — beyond the plan; cost if wrong: none known.
- Task 0: Ruling: test `failed_piece_falls_back_to_one_request` now runs on Groq — on Gemini the backup route rescues the piece, which is the new intended behaviour.
- Task 0: Ruling: tokio `test-util` added as a dev-dependency for `start_paused` tests.
- Task 1: Ruling: server-message fixtures (`tests-data/live-*.json`) are reconstructed from the documented Live API shape — the 2026-11-04 prototypes and their captures were deleted — cost if wrong: field names differ; Task 2's live test against the real server must confirm them.
- Task 1: Ruling: `language_code` reuses `gemini_language_code` (de → de-DE, zh → cmn-Hans-CN) and passes unknown codes as is — plan listed only ru/en — cost if wrong: Live API rejects a locale the batch API accepts.
- Task 1: Ruling: Chunker keeps one stateful sinc resampler (no seams every 100 ms) and trims its delay, so 0.35 s at 48 kHz gives 2 chunks on push and 2 400 samples on flush (plan expected 3 + 800) — cost if wrong: ~1 ms extra lag.
- Task 1: Ruling: `parse_server` also maps a JSON `error` object to `LiveEvent::Error`; close-frame reasons are Task 2's.
- Task 2: Ruling: `connect` returns once the socket is open and `setup` sent (handshake past the 3 s deadline → `ProviderError::Timeout`); a missing `setupComplete` by the same deadline arrives as `LiveEvent::Error("timeout")` — the plan wants audio queued before Ready, which needs the session before Ready — cost if wrong: Task 6 must treat that error as a fallback reason.
- Task 2: Ruling: `finish()` marks the end with an empty chunk on the audio channel (keeps the plan's `UnboundedSender<Vec<i16>>` and orders the end after queued audio) — cost if wrong: none; an empty chunk is never audio.
- Task 2: Ruling: binary frames are parsed as JSON too (Live API sends server JSON in binary frames).
- Task 2: Ruling: no backup live model — gemini-3.1-flash-live-preview rejects responseModalities TEXT («not supported by the model»); a live-stream failure falls back to the batch path with hedging (Task 0) — cost if wrong: none; batch path covers it.
- Task 3: Ruling: translations are coalesced, at most one request per 4 s while speaking (plan's deviation from spec §5.6, free-tier 5–15 RPM) — fix §5.6 in Task 8.
- Task 3: Ruling: `SegmentTranslator::finish` returns `(text, requests)` instead of `text` — usage must count every translation request — cost if wrong: one signature.
- Task 3: Ruling: a failed (not only stuck) translation request also returns its pieces to the queue; a retry still waits for the 4 s spacing.
- Task 3: Ruling: a piece that never had a draft shows its final text in the draft (otherwise the field would miss it until release).
- Task 0: Ruling: GEMINI_ROUTES = [Direct 3.5-flash-lite, Direct 3.1-flash-lite, Direct 3.6-flash] — generous quotas first, transcribe (25 RPD) dropped from routes — cost if wrong: two Lite models could be slow together; 3.6-flash (20/day) is the last resort.
- Task 4: Ruling: the IM logs to ~/Library/Logs/app.utter.desktop/im.log (never message text) — Step 3 needs a readable log.
- Task 4: Ruling: reading runs on its own Thread, writes on a serial queue — one queue for both would block sends forever.
- Task 4: Ruling: F19 is swallowed on keyDown and keyUp; menu icon = app icon at 16 px (Resources/UtterIM.tiff).
- Task 4: Ruling: the release bundle was not rebuilt (would kill the user's running Utter and re-ask Keychain); verified on the debug bundle.
- Task 5: Ruling: `DraftField`, `Tis`, `ImLink`, `NoDraftField` and the channel protocol live in cross-platform `output/ime/mod.rs`/`channel.rs`; FFI modules (`tis`, `peer`, `install`) are macOS-only — the plan's `#[cfg(macos)] pub mod ime` would hide `NoDraftField` from other platforms.
- Task 5: Ruling: `install(bundled: &Path)` takes the path (lib.rs passes resource_dir/UtterIM.app) instead of `&AppHandle` — testable; returns whether it copied.
- Task 5: Ruling: reinstall also when the executable bytes differ at the same CFBundleVersion (dev rebuilds), then `killall UtterIM` so the new copy serves fields.
- Task 5: Ruling: warm-up runs only when the IM has not connected 1.5 s after start; `end` restores the saved layout only if the current one is still "Utter" (the user may have picked one by hand).
- Task 5: Ruling: Step 5's live check uses a debug-eval-only hook (`<data>/debug-im.txt` → begin/draft per word/commit/end, logged) instead of an `#[ignore]` test — a test binary is unsigned, so the IM would reject it.
- Task 5: Ruling: TIS calls run off the main thread (dictation thread, warm-up thread) — cost if wrong: switching misbehaves; the live check shows it.
- Task 5: Ruling: IM id → `app.utter.inputmethod.live`, bundle file → `UtterLive.app` (spec §5.3 id differs; fix in Task 8) — cost if wrong: one rename.
- Task 5: Ruling: `install` never fails on enabling; macOS 15 enables third-party input methods only by the user's hand, so Utter needs a one-time «add Utter in Keyboard settings» step (UI for it: Task 7/8 scope) — cost if wrong: an extra onboarding step.

## Final review (fresh reviewer, 2026-11-07): fixed

- old draft of a released dictation reaching the next recording's island — draft owner seq (orchestrator) + drafts tagged by recording on the page (`visibleDraft`); late_draft_of_previous_dictation_is_dropped RED→GREEN, visibleDraft tests; suite 335/159.
- failed live connection disabled the pieces path for long dictations — `LiveHandle::failed()`; pieces resume, batch provider prewarmed, submit takes the batch path with its pieces; failed_live_resumes_pieces RED→GREEN.
- a panic in the live runner or in finish_text stalled the ordered queue — both supervised (runner → Fallback "crashed", finish_text → failed JobDone); crash_in_runner_falls_back, crash_in_finish_text_does_not_stall_queue RED→GREEN.
- (re-graded Minor #8 → Important: a recording lost) failed live Translate kept no audio for Retry — `save_audio` on any provider error; failed_live_translate_keeps_audio_for_retry RED→GREEN.
- the page received the whole draft 10×/s on long dictations — only the last 800 characters (word boundary) travel; long_draft_sends_only_its_tail RED→GREEN.

## Deferred minors

- an error after release with final pieces already in still takes the batch path (run.rs) instead of using them.
- `overlay://draft` is emitted directly while states go through run_on_main_thread — order relies on Tauri running main-thread closures promptly.
- no test for pressing again during the 0.3 s field exit.
- no plain-transcript last resort after all three routes fail (Ruling Task 0 #2); cleanup-off batch path still single-model (25 RPD).
- usage counts live text requests twice (stt_requests and llm_requests) and the Live stream as one speech request.
- `total_ms` for a live fallback excludes the live wait (run_job resets `submitted`).
- finish_text ignores RouteHealth order; `Unfinished` may count one request too many; a failed resampler streams unresampled audio; the larger overlay window catches clicks over 440×230 while an error with an action is shown.

## Measured

- Task 6: live round 1 (5 dictations): live path on every one; setup 339–383 ms; first interim 1.3–1.8 s after the key; release→delivered 1.53–1.61 s — always the 1.5 s FINAL_WAIT: Google sends the final piece ~0.4 s after the stream end but never `turnComplete`. User: everything grey while speaking (Google keeps the whole piece interim until a pause).
- Task 6: live round 2 (after fixes): user «всё супер работает»; one dictation in the log: setup 350 ms, first interim 1 370 ms, release→final 545 ms (was ~1 530), release→delivered 1 432 ms with cleanup on (cleanup of the single piece only after release, ~0.9 s). Budget ≤ 1 000 ms not met with cleanup — offered: start cleanup from the settled draft (risk: last words). Pending (user returns in the evening): long 30–40 s with video, Translate, Esc, VPN off, idle CPU, docs.
- Task 6: round 3 (video 60 fps, Notion): long 34 s dictation — window open, scroll, exit all smooth by eye; frame analysis: one skipped frame mid-way through every window opening (~150 ms in, when the field starts to show); exit smooth (the size jump in the numbers was a measuring artefact of the blurred grey field). release→final 1.51 s on 2 of 3 dictations: released after a pause → everything final before release, nothing more comes.
- Task 6: round 4 (user video): quiet-release fix works — release→final 587/710 ms (was 1.5 s); delivered 1 162 / 1 538 ms with cleanup. Pre-mounting the field did NOT remove the skipped frame (still frame ~9 of the window opening = the field's fade start).
- Task 0: live — routes_live: all 3 routes OK (dictation 1.4/2.2/1.8 s, translate 1.3/3.3/1.8 s); app: 2 user dictations won by route 0, total 1.37 s / 1.22 s; third press not in log.
- Task 2: live — gemini-3.5-transcribe-live, key in header accepted; socket 200 ms, ready 460 ms, first interim 0.88–0.93 s, release→segment 558 ms (VERBATIM) / 419 ms (SMART, again «по продам», «е» for «ё»). Server shapes match the fixtures.
- Task 3: live — translate_models_latency: flash-lite median 646 ms 5/5; 3.6-flash 1040/1355 ms then 429 «20 requests per day on Free Tier»; 3.7/3.8-flash reject THINKING_LEVEL_MINIMAL; 3.8-flash 5 RPM. GEMINI_TRANSLATE_MODEL = gemini-3.5-flash-lite.
- Task 0 follow-up (2026-11-05, backup_candidates): 3.5-flash audio 3.2 s ok, text → 429 naming gemini-3.6-flash (shares its 20 RPD quota); 3.1-flash-lite audio 2.1 s, text 8.2 / 2.1 s, text correct; 2.5-flash and 2.5-flash-lite 404 «no longer available to new users»; flash-latest rejects minimal thinking, default → 503 after 11 s; 3-flash-preview audio 3.0 s, text 1.1 / 1.7 s, correct. Daily limits of 3.1-flash-lite and 3-flash-preview unknown — asked the user for the AI Studio rate-limit page.
- Task 0 rework (2026-11-05, user's AI Studio screenshot, free tier): 3.5/3.1 Flash Lite 15 RPM / 500 RPD; 3 Flash, 3.6 Flash, 3.8 Flash 5 RPM / 20 RPD; 3.5 Transcribe 3 RPM / 25 RPD; 3.5 Transcribe Live unlimited RPM/RPD, 20K TPM; Gemma 4 26B/31B 30 RPM / 14.4K RPD.
