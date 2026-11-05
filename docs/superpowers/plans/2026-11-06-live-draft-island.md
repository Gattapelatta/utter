# Черновик в острове — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** при Gemini черновик речи растёт в острове у чёлки (окно 420 × 206, поле с рамкой, курсив сверху вниз), после отпускания дочищенный/переведённый итог вставляется нынешней доставкой, поле и таймер уходят, Капля радуется; способ ввода удалён.

**Architecture:** Задачи 0–3 плана `2026-11-04-live-dictation.md` готовы (маршруты, `providers/gemini_live.rs`, `live/pcm.rs`, `live/draft.rs`, `live/translate.rs`); этот план заменяет его задачи 4–8. Новый `live/run.rs` ведёт одну диктовку (соединение, звук, черновик, итог) и отдаёт черновик через `OverlaySink::draft`, итог — оркестратору; оркестратор вставляет итог прежним `finish` (доставка, история, «Готово») или уходит в пакетный путь. Остров получает третью форму «окно» в том же шаблоне `clip-path`.

**Tech Stack:** Rust (Tauri 2, tokio), React 19 + Motion, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-06-live-draft-island-design.md` (+ `2026-11-04-live-dictation-design.md` §4.5, §5.1, §5.2, §5.6; `2026-10-06-utter-design.md` §4.2, §4.2a).

## Global Constraints

- Ветка `feat/premium-ui`; commit + `git push` после каждой задачи; сообщения коммитов заканчиваются `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; TDD.
- Окно: остров 420 × 206 pt, скругление снизу 30; Капля 26 pt, центр (37, 43) от левого верхнего угла окна; таймер Manrope 600 13 pt, правый край 24 pt от правого края окна, центр по высоте 43.
- Поле: отступы 14 слева/справа/снизу, высота 128, фон `#1C1C1E`, рамка 1 px `#3A3A3D`, скругление 14, внутренние отступы 10 × 12; текст Manrope 12,5 / 18, курсив; `settled` `#E6E8EA`, `interim` `#8A8F96`; без затухания краёв.
- Черновик в остров — не чаще раза в 100 мс и только при изменении (`DraftThrottle`, `live/draft.rs`).
- Потоковое распознавание — всегда `TranscribeMode::Verbatim`.
- Тайм-ауты: соединение + `setupComplete` ≤ 3 с (`gemini_live::READY_TIMEOUT`); чистовик после отпускания ≤ 1,5 с; дочистка/перевод — `TRANSLATE_TIMEOUT` 3 с.
- Уход поля и таймера: ≈ 0,3 с (opacity → 0, translateY −10, scale 0,96, blur 2 px), одновременно окно → остров 360 × 104 и Капля → центр с «Готово».
- `cargo clippy --all-targets` (и с `--features debug-eval`) — 0; `pnpm test`, `cargo test --lib` зелёные; `#[ignore]`-тесты с ключом — только с согласия пользователя.
- Никаких `pkill -f`/шаблонов: только `pkill -x utter` / `killall UtterIM`.

## Review Focus

1. Очень длинная диктовка (10 мин, сотни строк) → окно остаётся 420 × 206, в поле видна последняя строка, страница не тормозит (тест `long_draft_keeps_last_line_visible`, Task 3).
2. Новое нажатие во время «Готово»/сворачивания после окна → новая запись начинается с привычного острова, старый черновик не всплывает (тест `new_recording_starts_without_old_draft`, Task 3; `draft_cleared_on_every_end`, Task 5).
3. Соединение оборвалось после того, как окно открылось → окно держится до итога пакетного пути, затем обычный финал (тест `mid_speech_drop_falls_back_keeping_window`, Task 5).
4. Перевод при выключенной «чистке» → всё равно переводится, а не вставляется русский (тест `translate_ignores_cleanup_off`, Task 4).
5. Пустой итог (ничего не сказано, но соединение было) → «Ничего не услышал», ничего не вставлено, окно не открывалось (тест `empty_live_is_nothing_heard`, Task 5).

---

### Task 1: Удалить способ ввода

**Files:**
- Delete: `src-tauri/src/output/ime/` (весь каталог), `macos/UtterIM/`, `scripts/build-im.sh`
- Modify: `src-tauri/src/output/mod.rs` (убрать `pub mod ime;`), `src-tauri/src/lib.rs` (убрать `draft_field` и `DraftFieldState`), `src-tauri/src/app/debug_eval.rs` (убрать `debug-im.txt` и `im_demo`), `src-tauri/src/output/macos.rs` (убрать `tap_key`), `scripts/dev-bundle.sh` (вернуть подпись без способа ввода: `codesign --force --deep --sign "Utter Dev" --identifier app.utter.desktop`; убрать вызов `build-im.sh` и строки про `UtterLive.app`), `src-tauri/tauri.conf.json` (убрать `bundle.macOS.files`), `.gitignore` (убрать `macos/UtterIM/build/`), `src-tauri/src/providers/defaults.rs` (комментарий-цифры не трогать)

**Interfaces:**
- Produces: ничего нового; после задачи в коде нет `output::ime`, `DraftField`, `ImChannel`.

- [ ] **Step 1:** удалить файлы и ссылки выше; `grep -rn 'ime::\|DraftField\|UtterIM\|UtterLive\|inputmethod' src-tauri/src scripts src-tauri/tauri.conf.json` → пусто.
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib` → PASS (минус тесты удалённых модулей); `cargo clippy --all-targets` и `--features debug-eval` → 0.
- [ ] **Step 3:** `scripts/dev-bundle.sh --debug` → в `Utter.app/Contents/Resources` нет `UtterLive.app`; `codesign --verify --deep --strict` → OK.
- [ ] **Step 4:** commit `chore(ime): remove the input method — the draft lives in the island` + push.

---

### Task 2: Черновик из ядра в окно острова

**Files:**
- Modify: `src-tauri/src/live/draft.rs` (`DraftParts`, `DraftComposer::parts`), `src-tauri/src/app/orchestrator.rs` (`OverlaySink::draft`), `src-tauri/src/app/overlay.rs` (`TauriOverlay::draft` → `overlay://draft`), `src/lib/types.ts`, `src/lib/ipc.ts`
- Test: `live/draft.rs` `mod tests`; `src/lib/ipc.test.ts` (если есть — иначе тип проверяет `pnpm test` через `tsc`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, Default, PartialEq, Serialize)] pub struct DraftParts { pub settled: String, pub interim: String }` (`live/draft.rs`); `DraftComposer::parts(&self) -> DraftParts` — `settled` = зафиксированные черновики кусков через пробел, `interim` = текущий черновой; `draft()` = `settled` + " " + `interim` без пустых частей (как сейчас).
  - `OverlaySink::draft(&self, parts: Option<&DraftParts>)` — с реализацией по умолчанию `{}` (фейки в тестах не меняются); `None` — убрать окно.
  - `TauriOverlay::draft` эмитит `overlay://draft` с `{ settled, interim }` или `null`.
  - TS: `export type DraftParts = { settled: string; interim: string }`; `ipc.overlay.draft(cb: (d: DraftParts | null) => void)`.

- [ ] **Step 1: Failing tests:** `parts_split_settled_and_interim` (interim «а б», segment «А б.», interim «в» → `settled == "а б"`, `interim == "в"`), `parts_empty_by_default`, `draft_parts_serialize` (`serde_json::to_value` → `{"settled":"x","interim":"y"}`).
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib live::draft` → FAIL (нет `parts`).
- [ ] **Step 3:** реализовать по Interfaces.
- [ ] **Step 4:** тесты PASS, `pnpm test` PASS, clippy 0.
- [ ] **Step 5:** commit `feat(live): draft parts reach the overlay window` + push.

---

### Task 3: Остров-окно с полем черновика

**Files:**
- Modify: `src/overlay/drop/islandMotion.ts` (форма «окно», база клипа), `src/overlay/drop/Island.tsx`, `src/overlay/Stage.tsx`, `src/overlay/main.tsx` (подписка на `overlay://draft`), `src/overlay/overlay.css`, `src-tauri/tauri.conf.json` (окно `overlay` 440 × 230), `src-tauri/src/app/overlay.rs` (если размер окна задаётся и там)
- Create: `src/overlay/drop/DraftPanel.tsx`, `src/overlay/drop/DraftPanel.test.tsx`
- Test: `islandMotion.test.ts`, `Island.test.tsx`, `DraftPanel.test.tsx`

**Interfaces:**
- Consumes: `DraftParts` (Task 2).
- Produces:
  - `View = "open" | "window" | "rest" | "folded"`; `WINDOW_BOX: Box = { w: 420, h: 206, r: 30 }`; база клипа — размер окна 420 × 206 (`.island` 420 × 206), все прежние формы (`ISLAND_BOX`, ушки, плашка) центрируются в ней с прежними размерами; `shapeTarget`/`dropTarget` знают `"window"`: Капля в окне — масштаб 26/48, центр (37, 43); `timerTarget(view)` — позиция/размер таймера (21 pt в `open`, 13 pt в `window`).
  - `DraftPanel({ draft: DraftParts | null, leaving: boolean })` — поле по Global Constraints; текст сверху вниз (`justify-content: flex-start`), после каждого обновления `scrollTop = scrollHeight`; `leaving` → анимация ухода (Global Constraints).
  - `Stage` получает `draft: DraftParts | null`; `view = "window"`, если состояние `recording`/`processing` и `draft` непуст; `done` после окна: поле и таймер уходят одной анимацией (`leaving`), форма → `open`, Капля → прежнее место с «Готово»; дальше всё как сейчас.

- [ ] **Step 1: Failing tests:**
  - `islandMotion.test.ts`: `window_box_clip` (`clip(WINDOW_BOX)` = `inset(0px 0px 0px 0px round 0px 0px 30px 30px)`), `island_box_centred_in_window_base` (`clip(ISLAND_BOX)` = `inset(0px 30px 102px 30px round 0px 0px 40px 40px)`), `drop_in_window_wing` (`dropTarget("window", …)` даёт центр (37, 43) и масштаб 26/48 относительно базы 420 × 206).
  - `DraftPanel.test.tsx`: `renders_settled_and_interim_colours` (два `span`, цвета из Global Constraints, `font-style: italic`), `text_starts_at_top` (контейнер с `justify-content: flex-start`), `long_draft_keeps_last_line_visible` (после 300 строк `scrollTop === scrollHeight - clientHeight` — в jsdom мокнуть `scrollHeight`/`clientHeight`), `no_fade_mask` (нет `mask-image`).
  - `Island.test.tsx`: `window_opens_only_with_draft` (recording без черновика → `open`; с черновиком → `window`), `done_after_window_marks_leaving` (переход `processing`+черновик → `done`: `DraftPanel` получает `leaving`, таймер в той же анимации), `new_recording_starts_without_old_draft` (`done` → `recording` с `draft = null` → `open`, поля нет).
- [ ] **Step 2:** `pnpm test` → FAIL по новым тестам.
- [ ] **Step 3:** реализовать по Interfaces; окно `overlay` в `tauri.conf.json` — 440 × 230 (островок по центру окна, как сейчас).
- [ ] **Step 4:** `pnpm test` PASS; `cargo test --lib` PASS (`island_origin_*` с новой шириной).
- [ ] **Step 5: Визуальная проверка в реальном окне** (jsdom не рисует): в `debug_eval.rs` под `#[cfg(feature = "debug-eval")]` — триггер-файл `<app data>/debug-draft.txt`: оверлей получает `recording`, затем черновик по словам 2 с, `processing`, `done`. Dev-сборка, `screencapture -l <окно острова>` в фазах «окно» и «уход»; сравнить с Global Constraints; триггер остаётся только в сборках с `debug-eval`.
- [ ] **Step 6:** commit `feat(overlay): the island opens into a draft window` + push.

---

### Task 4: Одна диктовка на лету — `live/run.rs`

**Files:**
- Create: `src-tauri/src/live/run.rs`
- Modify: `src-tauri/src/live/mod.rs` (`pub mod run;`)
- Test: `live/run.rs` `mod tests` (фейковые `LiveConnector` — каналы вместо сокета, `TextModel`, сборщик черновиков)

**Interfaces:**
- Consumes: `LiveConnector`, `LiveConfig`, `LiveEvent`, `TranscribeMode` (`providers/gemini_live.rs`); `Chunker` (`live/pcm.rs`); `DraftComposer`, `DraftParts`, `DraftThrottle` (`live/draft.rs`); `SegmentTranslator`, `TRANSLATE_TIMEOUT` (`live/translate.rs`); `pipeline::system_prompt`.
- Produces:
  - `pub enum Finisher { Plain, Text { model: Arc<dyn TextModel>, system: String } }` — `Text` для Диктовки с «чисткой» (`system_prompt(Mode::Dictation, …)`) и для Перевода всегда (`system_prompt(Mode::Translate, …)`).
  - `pub struct LiveRequest { pub cfg: LiveConfig, pub finisher: Finisher, pub rate: u32, pub channels: u16 }`
  - `pub enum LiveOutcome { Final { text: String, raw: String, requests: u32 }, Unfinished { raw: String, requests: u32 }, Fallback { reason: String, error: Option<ProviderError> }, Cancelled }` — `Unfinished`: дословный текст есть, дочистка/перевод не удались (оркестратор доделывает); `raw` — склеенные дословные куски.
  - `pub struct LiveHandle` — `pub fn audio(&self, interleaved: &[f32])`, `pub fn release(&self)`, `pub fn cancel(&self)`; `pub fn spawn(req: LiveRequest, connector: Arc<dyn LiveConnector>, draft: Arc<dyn Fn(Option<DraftParts>) + Send + Sync>, done: impl FnOnce(LiveOutcome) + Send + 'static) -> LiveHandle`.
  - Поведение: одна задача tokio; звук → `Chunker` → `session.audio`; `Interim`/`Segment` → `DraftComposer`; черновик → `draft(Some(parts))` по `DraftThrottle`; `Segment` → `SegmentTranslator::push` (при `Finisher::Text`); `release` → `finish()`, ждать `TurnComplete` или 1,5 с; хвостовой черновой кусок без чистовика добавляется как последний кусок; `Plain` → `Final { text: final_text() }`; `Text` → `SegmentTranslator::finish` → `Final` или при ошибке `Unfinished`; `connect` `Err(e)` или `LiveEvent::Error` → `Fallback`; ни одного куска → `Fallback { reason: "empty" }`; `cancel` → `Cancelled`, никаких запросов дальше; `draft(None)` при любом исходе.
  - Логи: `timing: live setup_ms`, `timing: live first interim_ms`, `timing: live release→final_ms`, `live fallback reason=…` (без адреса и ключа).

- [ ] **Step 1: Failing tests:** `plain_dictation_returns_final` (interim×2, segment, release → `Final` = текст сегмента, `draft` вызывался ≤ 1 раза на 100 мс и последний `None`), `clean_dictation_runs_text_model` (фейк чистки «x» → «X.»; `Final { text: "X.", raw: "x" }`, `requests ≥ 1`), `translate_returns_model_answer` (`Finisher::Text` с промптом перевода → `Final` = ответ фейка «Hello.»), `tail_without_segment_is_kept` (release, сервер прислал только interim → в тексте есть хвост), `connect_failure_falls_back`, `silent_server_falls_back` (соединение есть, событий нет → после `release` ≤ 1,5 с `Fallback { reason: "empty" }`), `server_error_mid_speech_falls_back`, `text_model_timeout_is_unfinished` (фейк чистки молчит 10 с → `Unfinished { raw }` не позже 3 с после release; `tokio::time::pause`), `cancel_sends_nothing` (`Cancelled`, у фейка модели 0 вызовов после cancel), `audio_is_chunked_and_streamed` (48 кГц стерео 0,35 с → в сессию ушли куски по 1 600 и хвост после release).
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib live::run` → FAIL.
- [ ] **Step 3:** реализовать (`select!` по событиям, звуку, release/cancel, тайм-аутам).
- [ ] **Step 4:** PASS, clippy 0.
- [ ] **Step 5:** commit `feat(live): one live dictation from first word to final text` + push.

---

### Task 5: Оркестратор выбирает путь на лету

**Files:**
- Modify: `src-tauri/src/app/orchestrator.rs`, `src-tauri/src/lib.rs` (`GeminiLive` в `Deps`), `src-tauri/src/pipeline/routes.rs` (если нужен `run_hedged` для текста — уже общий), `src/i18n/ru.json`, `src/i18n/en.json`
- Test: `orchestrator.rs` `mod tests` (существующий `rig` + фейковый `LiveConnector` + фейковый `OverlaySink::draft`)

**Interfaces:**
- Consumes: `spawn`, `LiveHandle`, `LiveOutcome`, `Finisher`, `LiveRequest` (Task 4); `OverlaySink::draft` (Task 2); `GeminiLive` (`providers/gemini_live.rs`); `routes::run_hedged` (Task 0 прошлого плана).
- Produces:
  - `Deps` + `pub live: Arc<dyn LiveConnector>`; `Input::LiveDone(u64, Box<LiveOutcome>)`.
  - Выбор в `start`: на лету, если провайдер Gemini, `in_flight == 0`, есть ключ; иначе — как сейчас. На лету `check_pieces` не работает.
  - `tick`: новые отсчёты `audio.peek(offset)` → `LiveHandle::audio`.
  - `submit`: запись останавливается как сейчас (звук сохраняется — для запасного пути и «Повторить»), `id = ids.next()`, `in_flight += 1`, `processing`, `release()`.
  - `LiveDone(id, outcome)`: `Final` → `JobResult { outcome: Ok(JobOutput { text, raw, degraded: false, llm_used: finisher != Plain, … }), requests }` в прежний `queue`/`finish`; `Unfinished { raw }` → один текстовый запрос через маршруты (`run_hedged` по `complete` маршрутов с тем же промптом); успех → как `Final`; неудача → Диктовка: `JobOutput { text: raw, degraded: true }`, Перевод: `spawn_job` по сохранённому звуку; `Fallback` → `spawn_job` по сохранённому звуку (ключ ошибки — от пакетного пути); `Cancelled` → ничего.
  - `overlay.draft(None)` на каждом конце (`finish`, ошибка, Esc).
  - Esc во время записи на лету → `cancel()`.
  - Ошибки: `ProviderError::Timeout` → ключ `slow_provider`; `BadResponse`, содержащий «location is not supported», → `region_blocked`.
  - Тексты: ru `"errors.slow_provider": "Gemini отвечает слишком долго"`, `"errors.region_blocked": "Gemini недоступен в твоём регионе — нужен VPN"`; en `"errors.slow_provider": "Gemini is taking too long"`, `"errors.region_blocked": "Gemini isn't available in your region — use a VPN"`.

- [ ] **Step 1: Failing tests:** `gemini_dictation_goes_live` (фейк соединения вызван, `deliver` вызван с итогом, история — итог и дословный `raw`), `live_waits_for_empty_queue`, `live_fallback_runs_batch_with_recorded_audio`, `mid_speech_drop_falls_back_keeping_window` (черновик был; после `Fallback` вставлен текст пакетного пути, `draft(None)` только в конце), `unfinished_cleanup_uses_routes` (маршрут отвечает → вставлен дочищенный), `unfinished_cleanup_fails_gives_raw_degraded`, `translate_ignores_cleanup_off` (настройка `cleanup = false`, Перевод → `Finisher::Text` с промптом перевода), `empty_live_is_nothing_heard` (`Fallback { reason: "empty" }` + пакетный путь «ничего не услышано» → `nothing_heard`, ничего не вставлено), `esc_cancels_live`, `groq_stays_batch`, `draft_cleared_on_every_end`, `timeout_is_slow_provider`, `location_error_is_region_blocked`; i18n-тест ключей RU/EN (существующий).
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** `cargo test --lib`, `pnpm test` PASS; clippy 0.
- [ ] **Step 5:** commit `feat(live): the orchestrator takes the live path; honest provider errors` + push.

---

### Task 6: Живая проверка, уборка на Mac и документы

**Files:**
- Modify (по итогам): модули задач 2–5; `docs/superpowers/specs/2026-11-06-live-draft-island-design.md` (итоговые цифры), `docs/superpowers/plans/2026-11-04-live-dictation.md` (пометка: задачи 4–8 заменены этим планом), `docs/superpowers/HANDOFF.md`, `CLAUDE.md`

- [ ] **Step 1: Уборка на Mac пользователя (с ним):** он удаляет «Utter» в «Системные настройки → Клавиатура → Источники ввода» (–); затем `killall UtterIM`, удалить `~/Library/Input Methods/UtterLive.app`; проверить, что «Utter» нет в списке раскладок.
- [ ] **Step 2:** `pkill -x utter; scripts/dev-bundle.sh`, предупредить про Связку ключей, `open Utter.app`.
- [ ] **Step 3: С пользователем:** по 3 диктовки в Заметках и Telegram — Диктовка с «чисткой», без «чистки», Перевод; длинная (≥ 30 с); Esc посреди; выключенный VPN (ожидание: окна нет, текст приходит пакетным путём или честная ошибка `region_blocked`).
- [ ] **Step 4: Бюджеты по логу** (медиана и максимум за ≥ 10 диктовок): `live first interim_ms` ≤ 1 000; `release→delivered` (`timing: delivered … total_ms`) ≤ 1 000 для Диктовки, ≤ 1 600 для Перевода. Видео `screencapture -v` раскрытия и ухода поля — 60 к/с без пропусков. Не уложились — `systematic-debugging`, цифры не подгонять.
- [ ] **Step 5:** простой 2 мин (`top`): utter и страница острова в прежних бюджетах (§8.4).
- [ ] **Step 6:** честный отчёт пользователю; обновить документы; commit `docs: live draft in the island — live check and handoff` + push.
