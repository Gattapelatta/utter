# Быстрая вставка после диктовки — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** к отпусканию клавиши почти весь текст уже очищен или переведён; Диктовка с чисткой вставляется не позже 1 с после отпускания (неуспевшие фразы — как распознаны), Перевод ждёт не дольше 4 с, «второго круга» больше нет.

**Architecture:** новый обработчик фраз `live/process.rs` (`PieceProcessor`, вместо `SegmentTranslator`) отправляет каждую чистовую фразу сразу, параллельно, через общий счётчик запросов `providers/budget.rs` (14/мин на модель, запас 3 на конец) и выбор запасной модели по `RouteHealth`; «догадка» отправляет хвост заранее на тишине и при отпускании. `live/run.rs` держит общий предел от отпускания; `finish` для Диктовки собирает обработанные фразы + распознанные (без звуков-паразитов, `pipeline/fillers.rs`), для Перевода — всё или ошибку. Оркестратор кладёт счётчик в `Shared` (его берут и агенты), пишет «частично без обработки» в Историю без сообщения Капли, а Перевод без ответа сразу отправляет на пакетный путь.

**Tech Stack:** Rust (tokio с остановленными часами в тестах, `test-util`), Tauri 2.

**Spec:** `docs/superpowers/specs/2026-11-10-fast-paste-design.md`

## Global Constraints

- Только Google; модели и лимиты — только в `providers/defaults.rs`.
- Предел Диктовки с чисткой — 1,0 с от отпускания до вставки; внутри `live/run.rs` берётся `DICTATION_LIMIT = 950 ms` (≈ 50 мс на доставку и очередь).
- Предел Перевода — `TRANSLATE_LIMIT = 4 s` от отпускания; дальше пакетный путь (`process_capture`).
- Счётчик: окно 60 с на модель, `GEMINI_LITE_PER_MINUTE = 14`, `END_RESERVE = 3`; 429 закрывает окно модели до `retry_after` (или 60 с).
- Повтор во время речи — через `RETRY_AFTER = 2.5 s` без ответа или сразу после ошибки; первый ответ побеждает, остальные отменяются.
- Догадка — при непустом хвосте и тишине `GUESS_QUIET = 400 ms`; одновременно не больше одной; совпадение — `pipeline::normalize` (сделать `pub`).
- Перевод после отпускания: сразу две попытки, через 1 с ещё одна свежая к основной, ошибка — следующая сразу.
- Запасная Lite берётся, только пока здорова: ошибка или нет ответа 2,5 с → `RouteHealth::punish(BACKUP_LITE_ROUTE)` (10 мин); иначе — свежий запрос к основной. `3.6-flash` в живой путь не брать.
- Фраза без обработки: только метка «без обработки» в Истории (`EntryStatus::Degraded`), Капля — обычное «Готово» (`degraded: false`).
- Звуки-паразиты: только отдельные слова «э», «ээ», «э-э», «э-э-э», «эм», «эмм», «мм», «ммм», «хм», «um», «uh», «erm»; «ну», «типа», «как бы», «а» не трогать.
- Каждый запрос (догадки, повторы) считается в `requests()` и в счётчиках использования.
- Диктовка без чистки, пакетный путь, Groq/OpenAI — без изменений.
- TDD; `cargo clippy --all-targets` — 0 предупреждений; `pnpm test` зелёный; коммит и `git push` после каждой задачи; ветка `feat/fast-paste`.
- В журнал — ни слова текста диктовки, только числа.

## Review Focus

1. Вторая диктовка сразу после длинной первой — окно счётчика полно: фразы не уходят, при отпускании `End` тоже не дают места → вставка как есть в пределе 1 с, без шквала 429 (тест в Task 5: `full_budget_pastes_raw_within_limit`).
2. Отпустил после паузы, всё уже обработано → вставка сразу, без ожидания предела (тест в Task 5: `all_answered_finishes_at_once`).
3. Google поправил слово в чистовике после догадки → в тексте слова чистовика, никогда слова догадки (тест в Task 4: `guess_miss_never_uses_guess_text`).
4. Esc во время обработки → обработчик удалён, все запросы отменены (тест в Task 3: `dropping_processor_aborts_requests`).
5. Модель вернула пустой текст для фразы из одних «э-э» → это ответ (фраза ничего не даёт), а не сбой с вставкой «э-э» (тест в Task 3: `empty_answer_counts_as_answer`).

---

### Task 1: Общий счётчик запросов и здоровье запасной модели

**Files:**
- Create: `src-tauri/src/providers/budget.rs`
- Modify: `src-tauri/src/providers/mod.rs` (`pub mod budget;`), `src-tauri/src/providers/defaults.rs`, `src-tauri/src/pipeline/routes.rs` (`RouteHealth`)

**Interfaces:**
- Produces:
  - `pub enum Priority { Speech, End, Agent }`
  - `pub struct RequestBudget` — `new(per_minute: u32, reserve: u32) -> Self`; `try_take(&mut self, model: &str, p: Priority, now: tokio::time::Instant) -> bool`; `rate_limited(&mut self, model: &str, retry_after: Option<Duration>, now: tokio::time::Instant)`
  - `pub type SharedBudget = Arc<std::sync::Mutex<RequestBudget>>`
  - `defaults::GEMINI_LITE_PER_MINUTE: u32 = 14`, `defaults::END_RESERVE: u32 = 3`, `defaults::BACKUP_LITE_ROUTE: usize = 1`
  - `RouteHealth::healthy(&self, route: usize, now: Instant) -> bool` (публичная обёртка над `punished`)

- [ ] **Step 1: Failing tests** в `budget.rs`:
  - `speech_keeps_the_end_reserve`: `new(14, 3)`; 11 раз `try_take("m", Speech)` → true, 12-й → false; затем 3 раза `End` → true, 4-й → false.
  - `agent_is_like_speech`: после 11 `Speech` — `Agent` → false.
  - `window_slides_after_a_minute`: 14 `End`, через 60 с + 1 мс — снова true.
  - `models_have_their_own_windows`: полное окно `"a"` не мешает `"b"`.
  - `rate_limited_closes_the_window`: `rate_limited("m", Some(20 s))` → `End` false до 20 с, true после; `None` → 60 с.
  - в `defaults.rs`: `backup_lite_route_is_the_backup_model` — `GEMINI_ROUTES[BACKUP_LITE_ROUTE] == Route::Direct { model: GEMINI_BACKUP_LITE_MODEL }`.
  - в `routes.rs`: `healthy_is_false_while_punished` — после `punish(1, t)` `healthy(1, t)` false, `healthy(1, t + PUNISH_FOR)` true.
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib budget` — FAIL (нет модуля).
- [ ] **Step 3:** Реализовать: окно — `HashMap<String, VecDeque<Instant>>`, закрытие — `HashMap<String, Instant>`; `Speech`/`Agent` берут, только если после взятия занято ≤ `per_minute - reserve`.
- [ ] **Step 4:** Тесты из Step 1 — PASS; clippy 0.
- [ ] **Step 5:** Commit + push: `feat(budget): shared request budget per model with an end reserve`.

### Task 2: Звуки-паразиты без модели

**Files:**
- Create: `src-tauri/src/pipeline/fillers.rs`
- Modify: `src-tauri/src/pipeline/mod.rs` (`pub mod fillers;`, `normalize` → `pub fn normalize`)

**Interfaces:**
- Produces: `pub fn strip_fillers(text: &str) -> String`; `pipeline::normalize(text: &str) -> String` (pub).

- [ ] **Step 1: Failing tests** (`strip_fillers`):
  - `"Э-э, давай завтра."` → `"Давай завтра."`
  - `"Я думаю, эм, что надо мм позвонить."` → `"Я думаю, что надо позвонить."`
  - `"Ну, давай."` → `"Ну, давай."`; `"А потом типа ушёл."` → без изменений
  - `"Эмоции и мама."` → без изменений (части слов не трогать)
  - `"Um, let's call, uh, tomorrow."` → `"Let's call, tomorrow."`
  - `"э-э-э"` → `""`; `"Ммм. Хм."` → `""`
- [ ] **Step 2:** `cargo test … --lib fillers` — FAIL.
- [ ] **Step 3:** Разбор по словам (`split_whitespace`), сравнение слова без окружающих знаков, в нижнем регистре, со списком из Global Constraints; запятая сразу после убранного слова уходит с ним; знак конца фразы после убранного слова переходит к предыдущему слову; первая буква результата и после «. ! ?» — заглавная.
- [ ] **Step 4:** PASS; clippy 0.
- [ ] **Step 5:** Commit + push: `feat(pipeline): strip standalone filler sounds from unprocessed text`.

### Task 3: Обработчик фраз — во время речи

**Files:**
- Create: `src-tauri/src/live/process.rs` (тесты и `FakeTranslator` переносятся из `translate.rs`)
- Delete: `src-tauri/src/live/translate.rs` (в конце Task 6, когда `run.rs` переведён; до этого оба модуля живут рядом)
- Modify: `src-tauri/src/live/mod.rs`

**Interfaces:**
- Consumes: `RequestBudget`, `Priority`, `SharedBudget`, `RouteHealth::healthy/punish`, `BACKUP_LITE_ROUTE` (Task 1).
- Produces:
  - `pub struct Models { pub main: Arc<dyn TextModel>, pub main_id: String, pub backup: Option<Arc<dyn TextModel>>, pub backup_id: String }`
  - `pub enum Kind { Cleanup, Translate }`
  - `pub struct PieceProcessor` — `new(models: Models, system: String, kind: Kind, budget: SharedBudget, health: Arc<Mutex<RouteHealth>>) -> Self`; `push(&mut self, segment: String)`; `poll(&mut self)`; `requests(&self) -> u32`
  - константа `pub const RETRY_AFTER: Duration = Duration::from_millis(2_500)`
  - `Drop` отменяет все запросы.

Модель: у каждой фразы `index`, распознанный текст и состояние `Waiting | InFlight { attempts, first_sent } | Done(String)`; попытка — `tokio::spawn` + `oneshot`, как сейчас. Несколько ждущих фраз уходят одним запросом (текст через пробел, ответ кладётся на первую фразу диапазона, остальные — `Done("")`, как `done: BTreeMap` сейчас). Часы — `tokio::time::Instant`; для `RouteHealth` — `.into_std()`.

- [ ] **Step 1: Failing tests** (`#[tokio::test(start_paused = true)]`):
  - `each_piece_goes_at_once`: два `push` с разницей 1 с → две попытки, вторая через 1 с (а не через 4 с).
  - `a_slow_request_does_not_hold_the_next`: первая фраза отвечает за 5 с, вторая за 0,1 с → вторая `Done` раньше первой.
  - `full_budget_joins_pieces`: счётчик с 11 занятыми → три `push` без запросов; освободилось место → один запрос с текстом трёх фраз.
  - `unanswered_piece_is_sent_again_after_2_5_s`: ответ 10 с → вторая попытка ровно через `RETRY_AFTER` на запасную модель (`tag`), запасная отвечает → `Done`, первая отменена.
  - `sick_backup_is_skipped`: `health.punish(BACKUP_LITE_ROUTE)` → повтор идёт на основную.
  - `slow_backup_gets_punished`: запасная не ответила за 2,5 с → `healthy(BACKUP_LITE_ROUTE)` false.
  - `failure_retries_at_once`: ошибка через 0,2 с → новая попытка в ту же миллисекунду.
  - `rate_limit_closes_the_window`: ответ 429 (`ProviderError::RateLimited`) → `budget.try_take(main_id, End)` false.
  - `empty_answer_counts_as_answer` (Review Focus 5): ответ `""` → `Done("")`, без повтора.
  - `dropping_processor_aborts_requests` (Review Focus 4): `drop` во время запроса → фейк не дошёл до ответа (счётчик завершений 0).
  - перенесённые: `translations_join_in_order` (склейка по порядку при перепутанных ответах), `requests` считает каждую попытку.
- [ ] **Step 2:** `cargo test … --lib live::process` — FAIL.
- [ ] **Step 3:** Реализовать `PieceProcessor` с интерфейсами выше; `poll` собирает ответы (`try_recv`), запускает повторы, отправляет ждущие фразы, если `try_take(main_id, Speech)`; 429 → `budget.rate_limited`.
- [ ] **Step 4:** PASS; старые тесты `translate.rs` тоже зелёные (модуль пока не тронут); clippy 0.
- [ ] **Step 5:** Commit + push: `feat(live): piece processor — every piece at once, in parallel, within the budget`.

### Task 4: Догадка

**Files:** Modify: `src-tauri/src/live/process.rs`

**Interfaces:**
- Produces: `PieceProcessor::guess(&mut self, tail: &str)` — отправить хвост заранее (`Priority::End`); повтор того же нормализованного текста — ничего; другой текст при живой догадке — старая отменяется, новая уходит.
  `push` сверяет фразу с живой или готовой догадкой через `normalize`: совпала — фраза берёт её попытки/ответ без нового запроса; нет — догадка отменяется, фраза идёт обычным порядком.
  Журнал: `timing: live guess` с полем `hit = true|false`.

- [ ] **Step 1: Failing tests:**
  - `guess_hit_needs_no_new_request`: `guess("давай завтра")`, затем `push("Давай завтра.")` → `requests() == 1`, фраза получает ответ догадки.
  - `guess_miss_never_uses_guess_text` (Review Focus 3): `guess("по продам")`, `push("по продажам")` → догадка отменена, запрос с текстом «по продажам», в итоге нет `EN[по продам]`.
  - `same_tail_is_guessed_once`: два `guess` с одинаковым текстом (разные знаки/регистр) → один запрос.
  - `changed_tail_replaces_the_guess`: `guess("давай")`, `guess("давай завтра")` → первая отменена, живёт одна.
  - `guess_counts_in_end_priority`: при 11 занятых (`Speech` уже нельзя) догадка всё равно уходит.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0.
- [ ] **Step 5:** Commit + push: `feat(live): guess — the tail goes ahead on silence and is matched with its final piece`.

### Task 5: Отпускание и сборка текста

**Files:** Modify: `src-tauri/src/live/process.rs`

**Interfaces:**
- Produces:
  - `PieceProcessor::release(&mut self, tail: &str)` — хвост (если непуст и не совпадает с догадкой) → догадка в две попытки; ждущие фразы → один запрос в две попытки (основная + запасная, если здорова, иначе две к основной); фразы в работе → по одной доп. попытке от самой старой, пока `End` даёт место.
  - `pub struct Ends { pub text: String, pub pieces: usize, pub raw_pieces: usize, pub requests: u32 }`
  - `pub async fn finish(self, tail: &str, deadline: tokio::time::Instant) -> Result<Ends, ProviderError>`:
    - `Kind::Cleanup`: ждёт, пока все фразы (и хвост, если непуст: ответ догадки с тем же текстом) готовы, но не дольше `deadline`; неготовые → `strip_fillers(распознанный текст)`, `raw_pieces` += 1; всегда `Ok`.
    - `Kind::Translate`: лестница — через 1 с без ответа ещё одна свежая попытка к основной, ошибка → сразу; всё готово → `Ok`; `deadline` → `Err(ProviderError::Timeout)`.
    - хвост, отличный от догадки (чистовой не пришёл) → отправить сейчас (`End`).
  - Журнал: `timing: live end` с полями `ready_ms`, `pieces`, `raw_pieces`.

- [ ] **Step 1: Failing tests:**
  - `release_sends_waiting_pieces_twice`: две ждущие фразы → один текст ушёл в основную и в запасную одновременно.
  - `release_sends_the_tail_as_a_guess`: `release("и в три")` → запрос сразу, не дожидаясь `push`.
  - `cleanup_mixes_done_and_raw_at_the_deadline`: фраза 1 отвечает за 0,2 с, фраза 2 молчит → в `deadline` текст `"EN[раз] Два, э-э, три."` → `"EN[раз] Два, три."`, `raw_pieces == 1`, `finish` вернулся ровно в `deadline`.
  - `all_answered_finishes_at_once` (Review Focus 2): всё готово до отпускания → `finish` возвращается без ожидания (`elapsed < 10 ms`).
  - `full_budget_pastes_raw_within_limit` (Review Focus 1): окно занято целиком (14) → ни одного нового запроса, `finish` в `deadline`, весь текст — распознанный без паразитов.
  - `translate_waits_for_every_piece`: фраза отвечает через 2 с → `Ok` через 2 с.
  - `translate_ladder_adds_a_fresh_main_after_1_s`: обе стартовые попытки молчат → третья через 1 с к основной, её ответ побеждает.
  - `translate_gives_up_at_its_deadline`: всё молчит → `Err(Timeout)` ровно в `deadline`.
  - перенесённый `nothing_to_translate_is_empty`.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0.
- [ ] **Step 5:** Commit + push: `feat(live): release and finish — two attempts at once, a hard deadline, raw pieces without fillers`.

### Task 6: `live/run.rs` на новом обработчике

**Files:**
- Modify: `src-tauri/src/live/run.rs`; Delete: `src-tauri/src/live/translate.rs`; Modify: `src-tauri/src/live/mod.rs`
- Modify (только чтобы собиралось): `src-tauri/src/app/orchestrator.rs` — поле `Shared.budget: SharedBudget` (в `Shared::new`: `RequestBudget::new(GEMINI_LITE_PER_MINUTE, END_RESERVE)`); `start_live` строит новый `Finisher::Text` (`Kind` по режиму, `shared.budget`, `route_health`); `live_done` разбирает `Final { partial, .. }` (пока без последствий — их делает Task 7).

**Interfaces:**
- Consumes: `PieceProcessor`, `Models`, `Kind`, `Ends` (Tasks 3–5).
- Produces:
  - `Finisher::Text { models: Models, system: String, kind: Kind, budget: SharedBudget, health: Arc<Mutex<RouteHealth>> }`
  - `LiveOutcome::Final { text, raw, requests, partial: bool }` (`partial` = `raw_pieces > 0`); `Unfinished` — только при `Kind::Translate` и `Err`.
  - `pub const DICTATION_LIMIT: Duration = Duration::from_millis(950)`, `pub const TRANSLATE_LIMIT: Duration = Duration::from_secs(4)`, `pub const GUESS_QUIET: Duration = Duration::from_millis(400)`.

Поведение: в `tick` во время речи — непустой хвост и голоса за `GUESS_QUIET` меньше `TAIL_VOICE_MS` → `guess(хвост)`; на отпускании — `release(хвост)`; цикл для `Kind::Cleanup` выходит не позже `released_at + DICTATION_LIMIT` (вместо `FINAL_WAIT`); `finish(хвост, released_at + LIMIT)` с тем же абсолютным пределом.

- [ ] **Step 1: Failing tests** (поддельный сервер Live, как сейчас):
  - `quiet_tail_is_guessed_before_release`: черновой «давай завтра», 0,4 с тишины → модель получила запрос до отпускания.
  - `cleanup_ends_within_the_dictation_limit`: чистовой кусок не приходит, модель молчит → исход через ≤ `DICTATION_LIMIT + 20 ms`, `Final { partial: true }` с распознанным хвостом.
  - `cleanup_answered_is_not_partial`: всё ответило → `Final { partial: false }`.
  - `translate_without_answer_is_unfinished`: модель молчит → `Unfinished` через ≤ `TRANSLATE_LIMIT + 20 ms`.
  - обновить тесты, что строили `Finisher::Text { model, system, backup }` (`run.rs` ~663–761), под новый вид; тест `FINAL_WAIT` (~702) остаётся для `Finisher::Plain`.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать, удалить `translate.rs`. **Step 4:** `cargo test … --lib live` PASS, clippy 0.
- [ ] **Step 5:** Commit + push: `feat(live): run on the piece processor — guesses on silence, one deadline from release`.

### Task 7: Оркестратор

**Files:**
- Modify: `src-tauri/src/app/orchestrator.rs` (`Shared`, `start_live` ~737, `live_done` ~809, `finish_text`/`finish_text_work` ~829/1133, `finish` ~1000), `src-tauri/src/pipeline/mod.rs` (`JobOutput`)

**Interfaces:**
- Consumes: `Shared.budget`, `Finisher::Text`, `LiveOutcome::Final { partial }` (Task 6).
- Produces: `JobOutput.partial: bool` — «часть фраз без обработки»: `EntryStatus::Degraded`, но `OverlayState::Done { degraded: false }`; все остальные места создания `JobOutput` — `partial: false`.

- [ ] **Step 1: Failing tests:**
  - `live_partial_is_degraded_in_history_only`: `LiveDone(Final { partial: true })` → История `EntryStatus::Degraded`, состояние Капли `Done { degraded: false }`, текст вставлен.
  - `unfinished_translate_takes_the_batch_path_at_once`: `Unfinished` в Переводе → ни одного текстового запроса по маршрутам, сразу пакетный перевод звука.
  - `start_live_passes_budget_and_health`: фабрика видит `Route::Direct` основной и запасной Lite, обработчик получает `shared.budget` (проверка через занятое окно: при полном окне живая диктовка не шлёт текстовых запросов во время речи).
  - заменить `unfinished_cleanup_uses_routes` и `unfinished_cleanup_fails_gives_raw_degraded` (~2406–2430): Диктовка больше не даёт `Unfinished`; оставить `crash_in_finish_text_does_not_stall_queue` для Перевода.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать; убрать текстовый круг из `finish_text_work` (только `process_capture`). **Step 4:** `cargo test … --lib` целиком PASS, clippy 0.
- [ ] **Step 5:** Commit + push: `feat(orchestrator): shared budget, partial cleanup marked in History only, no second cleanup round`.

### Task 8: Агенты в общем счётчике

**Files:** Modify: `src-tauri/src/agents/service.rs` (`summarize` ~624)

**Interfaces:**
- Consumes: `Shared.budget`, `Priority::Agent` (Task 1, 7).
- Produces: резюме агента с моделью только если `provider == Gemini` и `budget.try_take(GEMINI_TEXT_MODEL, Agent)`; иначе `model = None` (резюме без модели, как при выключенном «умном» резюме); для других провайдеров — как сейчас.

- [ ] **Step 1: Failing test** (вынести решение в чистую функцию `fn summary_may_use_model(provider: ProviderKind, budget: &SharedBudget, now) -> bool`): Gemini + место → true; Gemini + окно занято до запаса → false; Groq → true без счётчика.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0, `pnpm test` зелёный.
- [ ] **Step 5:** Commit + push: `feat(agents): summaries share the request budget and never take the dictation reserve`.

### Task 9: Живая проверка и документы

**Files:** Modify: `docs/superpowers/specs/2026-11-10-fast-paste-design.md` (§9 «Результаты»), `docs/superpowers/HANDOFF.md`, `CLAUDE.md` (заметка про живой путь), `docs/superpowers/specs/2026-11-04-live-dictation-design.md` §5.6 (ссылка на новую спецификацию)

- [ ] **Step 1:** Полный прогон: `cargo test --lib`, `cargo clippy --all-targets` (0), `pnpm test` — вывод в отчёт.
- [ ] **Step 2:** `pkill -x utter; scripts/dev-bundle.sh`, предупредить пользователя про окно «Связки ключей», дождаться исчезновения `SecurityAgent`, `open Utter.app`.
- [ ] **Step 3:** С пользователем — 20+ диктовок: короткие, длинные 30–40 с, с паузой перед отпусканием и без; Диктовка и Перевод.
- [ ] **Step 4:** По журналу: медиана/макс `delivered total_ms` (Диктовка, Перевод отдельно), доля `raw_pieces`, `guess hit/miss`, 429, `live fallback`. Цифры — в §9 спецификации; честно отметить, что не проверено.
- [ ] **Step 5:** HANDOFF и CLAUDE.md обновлены; commit + push: `docs: fast paste — live results`. Финальное ревью ветки и слияние в `main` — только с «да» пользователя.
