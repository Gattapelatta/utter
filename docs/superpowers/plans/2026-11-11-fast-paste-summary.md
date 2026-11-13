# Быстрая вставка — итоги плана

План `2026-11-10-fast-paste.md`, спецификация `../specs/2026-11-10-fast-paste-design.md` (цифры живых проверок — §9). Слито в `main` 2026-11-13 с «да» пользователя.

**Итог:** Диктовка с чисткой вставляется ≤ 0,97 с после отпускания (было 2–9 с) — три круга живых проверок, пользователь: «что длинные, что короткие — работают суперклассно». Перевод: обычно 1–3 с, на бесплатном Google бывает дольше; ускорение перевода закрыто решением пользователя (только бесплатный ключ), вместо него — пометка в настройках и README.

## Живые проверки

- Live 1 (2026-11-11 12:59–13:05 UTC, 18 dictations): dictation 12/12 at 0.96 s but short ones raw (guess hit 0/…), translation 0.97–1.45 s, one 9.3 s (budget exhausted → 4 s idle → batch 5.3 s).
- Live 2 (2026-11-11 13:17–13:23 UTC, ~27 dictations): dictation all ≤0.97 s, short ones cleaned 3/10; translation 0.85/1.5/2.1/2.2/3.2 s; budget full 17×; one Live stall (3 s, batch, 3.5 s; Google side).
- Live 3 (2026-11-11 13:36–13:41 UTC, 15 dictations): dictation all ≤0.97 s (user: «суперклассно»); translation 1.07/1.09/1.85/1.98/7.26 s — pieces took 5 s, 4 s limit → batch 3.2 s.

## Исправлено по финальному ревью (свежий проверяющий, opus)

- tail = grey words only (lost words, guesses missed) — growing_tail_without_answer_keeps_every_word RED→GREEN, suite 500/500
- guesses took the end reserve — guess_while_speaking_leaves_the_end_reserve RED→GREEN, suite 500/500
- quick-tap deadline counted from the socket opening (re-graded Minor→Important: breaks the user's 1 s on every quick tap) — quick_tap_limit_counts_from_the_key RED→GREEN, suite 501/501

## Решения по ходу (Ruling)

- Task 2: Ruling: «э…» and «м…» are matched as runs (э-э-э-э, мммм) beyond the listed forms — Google writes longer runs too — cost if wrong: none known (no real word is only э/м/hyphens)
- Task 3: Ruling: at most 4 attempts per request (MAX_ATTEMPTS) — budget alone would let a 503 storm retry every poll — cost if wrong: a piece stays raw after 4 failures
- Task 3: Ruling: timing asserts allow one 10 ms poll step — paused-clock timer order decides the exact ms, not the logic — cost if wrong: none
- Task 4: Ruling: a guess gets no retry while speaking (only the release adds attempts) — it is optional, retries would eat the end reserve — cost if wrong: a stalled guess helps nothing until release
- Task 5: Ruling: finish polls every 10 ms (no wakeup channel) — simplest exact deadline, ≤4 s of polling per dictation — cost if wrong: answer picked up ≤10 ms late
- Task 5: Ruling: after release the tail guess is raced like any request (Translate ladder 1 s) — spec §4.3 needs it for the last phrase — cost if wrong: one extra request
- Task 6: Ruling: tests went RED as compile errors (Finisher::Text shape changed) — the old API could not express the new tests — cost if wrong: none
- Task 6: Ruling: cleanup answering empty for every piece pastes the verbatim text (partial) instead of nothing — never paste nothing for real speech — cost if wrong: «э-э» pasted in a rare case
- Task 6: Ruling: Unfinished after a Translate timeout counts the requests made before finish (end attempts undercounted in usage) — finish returns Err without a count — cost if wrong: usage shows a few requests fewer
- Task 6: Ruling: an exact wake-up at release+limit ends the final-piece wait (the 100 ms tick overshot by up to 100 ms) — measured in cleanup_ends_within_the_dictation_limit (994 ms) — cost if wrong: none
- Task 6: Ruling: finish() runs release() itself when it never ran (quick tap: key up before the socket opened) — found as a flaky orchestrator test, reproduced by finish_without_release_races_like_release RED→GREEN — cost if wrong: none
- Task 7: Ruling: Unfinished in Dictation (should not happen after Task 6) pastes the verbatim text with no request; crash_in_finish_text test moved to Translate — the only mode that still reaches finish_text — cost if wrong: none
- Task 7: Ruling: start_live_passes_budget_and_health test written as live_cleanup_uses_the_shared_budget (passed at once: the wiring landed in Task 6 to compile) — cost if wrong: none
- Final: Ruling: spec §5.2 listed guesses under End; guesses while speaking now use Speech, only the release/finish guess uses End — the reserve exists for the end, guesses on every pause emptied it — cost if wrong: on a full window no guess while speaking
- Live 1: Ruling: a guess serves its final piece when it is its start minus ≤3 words (cleanup; missing words added as heard) — Google withholds the last word from drafts (bench live_phrase_latency) — cost if wrong: last 1–3 words uncleaned
- Live 1: Ruling: translation makes no guesses (only a tail without final at finish is sent) — they never match and cost 3 requests each — cost if wrong: none measured
- Live 1: Ruling: finish ends at once when nothing runs and nothing can be sent (review minor 5 promoted by the 9.3 s live case) — cost if wrong: a window that frees within the deadline is not waited for
- Live 2: Ruling: no time-based re-send while speaking (only after errors), extras only from release — the budget ran out 17× — cost if wrong: a hung mid-speech request waits for release
- Live 2: Ruling: the healthy backup Lite model takes first attempts when the main model's minute is full — doubles capacity; health check keeps a failing backup out — cost if wrong: pieces on a slow backup until it is punished
- Live 3: Ruling: translation late 2 s (or with nothing sendable) races the whole verbatim text on main, healthy backup and 3.6 Flash; limit 4 → 5 s — the user's goal is no long waits; text beats audio batch — cost if wrong: 3.6 Flash (20/day) used on slow days

## Отложенные мелочи

- `timing: live end ready_ms` counts from finish, not release (use `delivered total_ms` for measuring)
- cleanup answering empty for everything pastes raw with fillers («э-э» alone is pasted)
- a comma stays before a removed filler («Давай, э-э, завтра» → «Давай, завтра»)
- batch-path routes, Fallback jobs and History Retry use the Lite models without budget slots
- agent summary takes a slot before reading the key; a summary 429 does not close the window
- a final piece consumes a guess made for a later tail; interims after release are not re-guessed until finish
- (закрыто в живой проверке 1: «безнадёжные запросы ждут до предела» — теперь `finish` выходит сразу, если отправить нечего)
