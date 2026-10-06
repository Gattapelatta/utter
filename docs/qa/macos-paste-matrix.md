# macOS paste matrix

Spec §9.3. Fill one row per app × layout × record mode × hotkey × mode.
Result: ✅ inserted correctly · ⚠️ copied only (expected for the case) · ❌ failure (open a bug, add a regression test).

## Automated (scripts/paste-matrix.sh)

| Layout | Paste | Type | Date | Notes |
|---|---|---|---|---|
| EN | 5/5 ✅ | 5/5 ✅ | 2026-10-07 | two consecutive runs; latin, Cyrillic, emoji, multi-line, long text |
| RU | | | | |

## Manual

| App | Layout | Record mode | Hotkey | Mode | Result | Notes |
|---|---|---|---|---|---|---|
| TextEdit | EN | Hold | Right Option | Dictation | | |
| TextEdit | RU | Toggle | F13 | Translate | | |
| Notes | EN | Hold | Right Option | Dictation | | |
| Notes | RU | Toggle | F13 | Translate | | |
| Safari (textarea) | EN | Hold | Right Option | Dictation | | |
| Safari (textarea) | RU | Toggle | F13 | Translate | | |
| Chrome (textarea) | EN | Hold | Right Option | Dictation | | |
| Chrome (textarea) | RU | Toggle | F13 | Translate | | |
| Telegram | EN | Hold | Right Option | Translate | | |
| Telegram | RU | Toggle | F13 | Dictation | | |
| Slack | EN | Hold | Right Option | Translate | | |
| Slack | RU | Toggle | F13 | Dictation | | |
| VS Code | EN | Hold | Right Option | Dictation | | |
| VS Code | RU | Toggle | F13 | Translate | | |
| Terminal | EN | Hold | Right Option | Dictation | | |
| Terminal | RU | Toggle | F13 | Translate | | |

## Stress

| Check | Result | Notes |
|---|---|---|
| 100 dictations in a row, no restart | | |
| 20 presses shorter than 0.3 s | | |
| 3 recordings queued during processing (order kept) | | |
| Sleep → wake → dictation | | |
| Unplug headphones while recording | | |

## Budgets (spec §8.4)

Measured 2026-10-07 on the user's Mac (release build, Gemini one-request path, app log `timing:` lines).

| Metric | Target | Measured |
|---|---|---|
| KeyDown → overlay visible | < 50 ms | |
| KeyDown → first sample | < 100 ms | |
| Release → inserted (2–5 s speech, Gemini) | ≤ 1.5 s p50 | 1.33–1.51 s (3 runs); local prepare 3–7 ms, deliver 2–8 ms |
| Idle CPU / RAM | ~0% / < 80 MB | |
