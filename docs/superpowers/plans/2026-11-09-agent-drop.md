# Капля и ИИ-агенты — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Капля у чёлки показывает, что Claude Code и Codex работают, ждут или закончили (с итогом одной строкой), даёт отвечать на разрешения Claude Code из острова; плюс клавиша «Задача» для агентов.

**Architecture:** Агенты вызывают маленькую программу `utter-agent` (второй bin того же крейта, внутри `Utter.app/Contents/MacOS`), она шлёт событие в Utter по Unix-сокету и, для разрешений, ждёт ответ. В Utter модуль `agents` разбирает события, держит реестр сессий и шлёт снимок `AgentView` в окно острова; что рисовать, решает чистая модель на странице. Хуки ставятся правкой `~/.claude/settings.json` и строки `notify` в `~/.codex/config.toml` чистыми функциями над текстом.

**Tech Stack:** Rust (tokio, serde_json, Tauri 2), React + Motion, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-09-agent-drop-design.md`

## Global Constraints

- Ничего не рисуется под вырезом камеры: в покое только ушки (`EAR_W = 36`), в раскрытом острове содержимое ниже `notch.height` (≈32 pt).
- Горячих клавиш Y/N для разрешений нет — только кнопки мышью.
- Utter никогда не решает разрешение сам: решение уходит только после нажатия пользователя; иначе хук возвращается **без решения**.
- `utter-agent` не задерживает агента: нет Utter / сокет молчит 300 мс → выход с кодом 0 без вывода.
- Чужие хуки и прежняя команда `notify` сохраняются; при выключении убирается только своё (метка — `utter-agent` в команде).
- В провайдера уходит только последнее сообщение агента (для итога); всё остальное — на Mac.
- Строки RU/EN через i18n; копия из спецификации — дословно.
- Итог: до 70 знаков, таймаут 3 с, иначе первая фраза.
- Разрешение ждёт до 30 с с момента показа карточки; хук Claude Code для `PermissionRequest` ставится с `"timeout": 120`.
- TDD; `cargo clippy --all-targets` — 0 предупреждений; коммит и push после каждой задачи; ветка `feat/agent-drop`.
- Цвета состояний — рабочие (синий `#3B82F6`, янтарный `#F59E0B`, зелёный `#22C55E`), финальные подбираются с пользователем в Task 12.

## Review Focus

1. `~/.claude/settings.json` нет, пустой или с чужими хуками на те же события → создаём/дописываем, чужое остаётся, повторная установка не дублирует (тест в Task 2).
2. `notify` в `config.toml` многострочный массив, отсутствует или уже наш → корректная установка и возврат прежнего (тест в Task 2).
3. Utter не запущен или сокет-файл остался от упавшего процесса → `utter-agent` выходит < 300 мс без вывода (тест в Task 3).
4. Два запроса разрешения сразу, запрос во время диктовки или на паузе → по очереди / ждёт конца диктовки / сразу без решения (тест в Task 6).
5. Последнее сообщение агента пустое, огромное или в markdown → строка итога не ломается: пусто → «Готово» без строки, длинное → обрезка по слову, markdown снят (тест в Task 4).

---

### Task 1: События агентов

**Files:**
- Create: `src-tauri/src/agents/mod.rs`, `src-tauri/src/agents/event.rs`, `src-tauri/src/agents/fixtures/*.json`
- Modify: `src-tauri/src/lib.rs` (`pub mod agents;`)
- Test: в `event.rs`

**Interfaces:**
- Produces:
```rust
pub enum Agent { ClaudeCode, Codex }                       // serde: "claude_code" | "codex"
pub struct SessionKey { pub agent: Agent, pub id: String }  // Claude: session_id; Codex: thread-id
pub enum EventKind {
    Started,                                   // UserPromptSubmit
    Action(String),                            // PreToolUse → «Правит billing.ts», «Запускает npm test» (RU/EN по lang)
    Steps { done: u32, total: u32 },           // PostToolUse с tool_name == "TodoWrite"
    Waiting(String),                           // Notification, notification_type idle_prompt | agent_needs_input
    Permission { tool: String, detail: String, suggestion: Option<serde_json::Value> }, // PermissionRequest
    Finished { last_message: String },         // Stop (last_assistant_message) | Codex agent-turn-complete (last-assistant-message)
    Ended,                                     // SessionEnd
}
pub struct AgentEvent { pub key: SessionKey, pub project: String /* basename(cwd) */, pub kind: EventKind }
pub fn parse_claude(hook_event: &str, payload: &serde_json::Value, lang: UiLanguage) -> Option<AgentEvent>;
pub fn parse_codex(payload: &serde_json::Value) -> Option<AgentEvent>;
```

- [ ] **Step 1: Сверить форматы.** Прочитать https://code.claude.com/docs/en/hooks.md (разделы UserPromptSubmit, PreToolUse, PostToolUse, Notification, PermissionRequest, Stop, SessionEnd) и записать по одному образцу входа каждого события в `agents/fixtures/claude_<event>.json`; для Codex — `codex_turn_complete.json` с ключами `type, thread-id, turn-id, cwd, client, input-messages, last-assistant-message` (видели в пробе 2026-11-09). Расхождения с этим планом — `Ruling:` в журнал.
- [ ] **Step 2: Падающие тесты** `parses_each_claude_event`, `todo_write_gives_steps` (3 из 5 completed → `Steps{done:3,total:5}`), `pre_tool_use_makes_short_action` (`Edit` с `file_path: /a/b/billing.ts` → «Правит billing.ts»; `Bash` с `command: "npm test"` → «Запускает npm test»; команда длиннее 40 знаков обрезается с «…»), `permission_keeps_first_suggestion`, `stop_uses_last_assistant_message`, `codex_turn_complete`, `unknown_event_is_none`, `project_is_cwd_basename`.
- [ ] **Step 3: Запустить** `cargo test --manifest-path src-tauri/Cargo.toml --lib agents::event` → FAIL (нет модуля).
- [ ] **Step 4: Реализовать** разбор; глаголы действий: Read «Читает»/«Reading», Edit|Write|MultiEdit «Правит»/«Editing», Bash «Запускает»/«Running», Grep|Glob «Ищет»/«Searching», прочее — имя инструмента.
- [ ] **Step 5: Тесты зелёные**, коммит `feat(agents): parse Claude Code hook and Codex notify events`.

### Task 2: Установка и снятие подключения

**Files:**
- Create: `src-tauri/src/agents/install.rs`
- Test: в нём

**Interfaces:**
- Produces:
```rust
pub const CLAUDE_EVENTS: [&str; 7] = ["UserPromptSubmit","PreToolUse","PostToolUse","Notification","PermissionRequest","Stop","SessionEnd"];
pub enum InstallError { Corrupt(String) }
pub fn claude_install(current: Option<&str>, helper: &str) -> Result<String, InstallError>;   // None = файла нет
pub fn claude_uninstall(current: &str) -> Result<String, InstallError>;
pub fn claude_installed(current: &str, helper: &str) -> bool;
pub fn codex_install(current: Option<&str>, helper: &str) -> Result<(String, Option<Vec<String>>), InstallError>; // (новый текст, прежний notify)
pub fn codex_uninstall(current: &str, prev: Option<&[String]>) -> String;
pub fn codex_installed(current: &str, helper: &str) -> bool;
pub struct Detected { pub claude_code: bool, pub codex: bool }
pub fn detect(home: &Path) -> Detected;   // ~/.claude или /Applications/Claude.app; ~/.codex
```
- Команда хука Claude Code: `"<helper>" claude <Event>`; у `PermissionRequest` ещё `"timeout": 120`. Codex: `notify = ["<helper>", "codex", <прежние элементы…>]`.

- [ ] **Step 1: Падающие тесты** `claude_install_creates_file_when_missing`, `claude_install_keeps_foreign_hooks` (чужой хук на `Stop` остаётся рядом с нашим), `claude_install_is_idempotent`, `claude_reinstall_updates_moved_helper_path`, `claude_uninstall_removes_only_ours`, `claude_corrupt_json_is_error`, `codex_install_chains_previous_notify` (вход — строка из пробы с SkyComputerUseClient; выход — `["<helper>","codex","…SkyComputerUseClient","turn-ended"]`, prev = `Some([…,"turn-ended"])`), `codex_install_without_notify_adds_top_level_line` (строка встаёт до первой `[таблицы]`), `codex_multiline_notify`, `codex_install_is_idempotent` (уже наш → prev не теряется), `codex_uninstall_restores_previous`, `codex_uninstall_without_previous_removes_line`, `detect_finds_dirs` (временный home).
- [ ] **Step 2: Запустить** → FAIL.
- [ ] **Step 3: Реализовать.** JSON — через `serde_json::Value` с `preserve_order` (порядок ключей пользователя сохраняется; если фичи нет в Cargo.toml — включить). TOML — построчно: найти верхнеуровневый `notify = [` (до первой строки `[`), собрать массив до закрывающей `]`, разобрать строки в кавычках; остальное не трогать.
- [ ] **Step 4: Тесты зелёные**, коммит `feat(agents): connect and disconnect Claude Code hooks and Codex notify`.

### Task 3: Программа `utter-agent` и протокол сокета

**Files:**
- Create: `src-tauri/src/agents/wire.rs`, `src-tauri/src/bin/utter-agent.rs`, `src-tauri/src/agents/host.rs`
- Modify: `src-tauri/Cargo.toml` (`[[bin]] name = "utter-agent"`), `src-tauri/tauri.conf.json` (`bundle.externalBin: ["binaries/utter-agent"]`), `scripts/dev-bundle.sh`, `scripts/release.sh` (собрать `utter-agent` под нужную цель и положить в `src-tauri/binaries/utter-agent-<target-triple>` до `tauri build`; для universal — `lipo` двух сборок в `utter-agent-universal-apple-darwin`), `scripts/release/main.ts` (`verify`: `Contents/MacOS/utter-agent` есть, подписан «Utter Dev», x86_64 + arm64), `.gitignore` (`src-tauri/binaries/`)
- Test: в `wire.rs`, `host.rs`; `src-tauri/tests/utter_agent.rs`

**Interfaces:**
- Produces:
```rust
// wire.rs — одна строка JSON туда, одна (только для разрешений) обратно
pub struct Request { pub v: u8 /*1*/, pub source: Source /*claude|codex*/, pub event: String /*имя события; для codex "notify"*/, pub payload: serde_json::Value, pub host: Option<Host>, pub wants_reply: bool }
pub struct Host { pub pid: i32, pub bundle_id: Option<String>, pub name: Option<String> }
pub enum Reply { NoDecision, Allow { always: bool }, Deny }
pub fn socket_path() -> PathBuf;   // env UTTER_AGENT_SOCKET или <app data>/agents.sock
pub fn hook_output(reply: &Reply, suggestion: Option<&serde_json::Value>) -> Option<String>;
// Allow{always:true} + suggestion → {"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedPermissions":[suggestion]}}}
// Allow{always:false} → behavior allow без updatedPermissions; Deny → {"behavior":"deny","message":"Denied in Utter"}; NoDecision → None
// host.rs
pub fn pick_host(chain: &[(i32, Option<String>, Option<String>)]) -> Option<Host>; // первый предок с bundle_id
pub fn parent_chain(pid: i32) -> Vec<(i32, Option<String>, Option<String>)>;  // sysctl KERN_PROC + NSRunningApplication
```
- Поведение `utter-agent`: `claude <Event>` — stdin → Request; ждёт ответ только для `PermissionRequest` (до 110 с), печатает `hook_output`; `codex <prev…> <json>` — шлёт, затем `exec` прежнюю команду с прежними аргументами и тем же JSON последним. Подключение к сокету и запись — таймаут 300 мс; любая ошибка → выход 0 без вывода (для codex — всё равно запустить прежнюю команду).

- [ ] **Step 1: Падающие тесты** `hook_output_allow_always_echoes_suggestion`, `hook_output_allow_once`, `hook_output_deny`, `no_decision_prints_nothing`, `pick_host_skips_shells` (zsh/node без bundle → Claude.app), в `tests/utter_agent.rs`: `no_utter_exits_fast` (UTTER_AGENT_SOCKET на несуществующий путь; < 300 мс, stdout пуст, код 0), `stale_socket_file_exits_fast`, `permission_waits_for_reply` (тестовый слушатель отвечает Allow{always:true} → stdout = ожидаемый JSON), `codex_runs_previous_notify` (прежняя команда — скрипт, пишущий аргументы в файл; файл содержит `turn-ended` и JSON).
- [ ] **Step 2: Запустить** `cargo test --manifest-path src-tauri/Cargo.toml` → FAIL.
- [ ] **Step 3: Реализовать** (синхронный `std::os::unix::net::UnixStream` в бинаре — без tokio, чтобы стартовал быстро).
- [ ] **Step 4: Тесты зелёные; `scripts/release.sh 0.0.0 --dry-run --allow-branch`** → `verify` печатает ✓ и видит `utter-agent`. Коммит `feat(agents): utter-agent helper and its socket protocol`.

### Task 4: Строка итога

**Files:**
- Create: `src-tauri/src/agents/summary.rs`, `src-tauri/src/pipeline/prompts/agent_summary.md`
- Test: в `summary.rs`

**Interfaces:**
- Consumes: `providers::TextModel::complete(system, user)`.
- Produces:
```rust
pub const SUMMARY_MAX: usize = 70;
pub fn first_sentence(text: &str, max: usize) -> Option<String>;  // снимает markdown (#, *, `, ссылки), первая фраза, обрезка по слову + «…»; пусто → None
pub async fn summarize(text: &str, lang: UiLanguage, model: Option<&dyn TextModel>, timeout: Duration) -> Option<String>;
```
- Промпт `agent_summary.md`: «Ты пишешь одну строку для уведомления: что сделал ИИ-агент. Язык: {lang}. Не больше 70 знаков, без кавычек и точки в конце, без выдумок; только то, что есть в тексте.»

- [ ] **Step 1: Падающие тесты** `empty_is_none`, `markdown_is_stripped`, `long_text_cut_on_word` (≤ 70 знаков, кончается «…»), `model_answer_used_and_trimmed_to_70`, `model_timeout_falls_back_to_first_sentence` (фейк, отвечающий через 5 с, таймаут 3 с, paused clock), `model_error_falls_back`, `no_model_falls_back`.
- [ ] **Step 2: Запустить** → FAIL. **Step 3: Реализовать.** **Step 4:** зелёные, коммит `feat(agents): one-line summary of what the agent did`.

### Task 5: Реестр сессий, пауза

**Files:**
- Create: `src-tauri/src/agents/registry.rs`
- Test: в нём

**Interfaces:**
- Consumes: `AgentEvent`, `SessionKey`, `Agent` (Task 1).
- Produces:
```rust
pub enum SessionState { Working, Waiting, Permission, Done }
pub struct AgentRow { pub key: SessionKey, pub agent: Agent, pub project: String, pub state: SessionState, pub steps: Option<(u32, u32)>, pub actions: Vec<String> /*≤2, новое последним*/, pub summary: Option<String>, pub seen: bool }
pub enum Card { Permission { id: u64, key: SessionKey, tool: String, detail: String, can_always: bool }, Done { key: SessionKey }, Waiting { key: SessionKey }, AfterPause { done: u32, waiting: u32 } }
pub struct AgentView { pub rows: Vec<AgentRow> /*сначала Permission, Waiting, потом Working, Done*/, pub card: Option<Card>, pub paused: bool }
pub struct Registry { /* sessions, очередь карточек, paused, missed */ }
impl Registry {
    pub fn apply(&mut self, ev: AgentEvent, now: Instant) -> Option<u64>;   // Some(id) для нового запроса разрешения
    pub fn set_summary(&mut self, key: &SessionKey, s: Option<String>);
    pub fn resolve_permission(&mut self, id: u64);                           // после ответа/таймаута
    pub fn dismiss_card(&mut self, now: Instant);                            // OK / истёк показ
    pub fn mark_seen(&mut self, key: &SessionKey);
    pub fn set_paused(&mut self, on: bool, now: Instant);                    // снятие → карточка AfterPause, если что-то пропущено
    pub fn tick(&mut self, now: Instant);                                    // Done-карточка гаснет через 6 с; Working без событий 30 мин → удалить
    pub fn view(&self) -> AgentView;
}
pub const DONE_CARD: Duration = Duration::from_secs(6);
pub const AFTER_PAUSE_CARD: Duration = Duration::from_secs(4);
pub const STALE: Duration = Duration::from_secs(30 * 60);
```

- [ ] **Step 1: Падающие тесты** `started_then_finished_shows_done_card`, `done_card_hides_after_6s_but_row_stays_unseen`, `mark_seen_clears_green`, `steps_and_last_two_actions`, `permission_card_has_priority_and_queue` (два запроса → по очереди), `can_always_only_with_suggestion`, `waiting_notification`, `ended_removes_session`, `stale_working_removed_after_30min`, `paused_hides_cards_and_counts_missed`, `unpause_shows_after_pause_card_for_4s`, `codex_finished_without_started` (сразу строка Done), `rows_sorted_by_urgency`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные; коммит `feat(agents): session registry, cards and pause`.

### Task 6: Служба агентов в Utter

**Files:**
- Create: `src-tauri/src/agents/service.rs`, `src-tauri/src/agents/broker.rs`
- Modify: `src-tauri/src/settings.rs` (`agents: AgentSettings { claude_code: bool, codex: bool, summary: bool /*true*/ }`, `agents_codex_prev_notify: Option<Vec<String>>`, миграция со старых файлов — значения по умолчанию), `src-tauri/src/app/services.rs`, `src-tauri/src/app/commands.rs`, `src-tauri/src/app/overlay.rs`, `src-tauri/src/lib.rs`
- Test: `broker.rs`, `service.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
```rust
// broker.rs — ожидание ответа на разрешение
pub struct Broker;  // id → oneshot::Sender<Reply>
impl Broker {
    pub fn open(&self, id: u64) -> oneshot::Receiver<Reply>;
    pub fn answer(&self, id: u64, r: Reply);
    pub fn shown(&self, id: u64, now: Instant);  // отсчёт 30 с начинается здесь
    pub fn expire(&self, now: Instant) -> Vec<u64>; // истёкшие → NoDecision
}
pub const PERMISSION_WAIT: Duration = Duration::from_secs(30);
// service.rs
pub fn spawn(app: AppHandle);                       // слушает socket_path(), применяет события, шлёт "overlay://agents" (AgentView)
pub fn apply_settings(app: &AppHandle, s: &AgentSettings) -> AgentsStatus; // ставит/снимает хуки (Task 2), пишет файлы
pub struct AgentsStatus { pub detected: Detected, pub claude_code: Link, pub codex: Link }
pub enum Link { Off, On, Missing /*агента нет*/, Broken(String) /*файл повреждён*/ }
// команды Tauri: agents_status() -> AgentsStatus; agent_permission(id: u64, choice: "allow"|"always"|"deny"|"open");
//               agent_open(key: SessionKey); agent_dismiss(); agent_seen(key: SessionKey)
```
- На `Finished`: если `agents.summary` — `summary::summarize(last_message, ui_lang, Some(text model текущего провайдера), 3 s)`, иначе `first_sentence`; результат → `Registry::set_summary`, затем снова `overlay://agents`. Пока идёт запись/обработка диктовки, `tick()` не гасит карточки (6 с начинаются после показа).
- Путь к помощнику: `std::env::current_exe()`.parent / `utter-agent`. Самопроверка `apply_settings` при запуске и после каждого изменения настроек. Запрос во время записи/обработки диктовки: карточка не показывается, `shown()` не вызывается, пока `AppStatus != Idle`. На паузе — сразу `NoDecision`.
- Окно острова принимает клики, пока в `AgentView.card` есть кнопки или курсор над ушками (опрос `overlay_rest::cursor_screen()`-точки каждые 100 мс, только пока `rows` не пусты); событие `overlay://agent-hover {over: bool}`.
- «Открыть» — `NSRunningApplication(processIdentifier: host.pid).activate(options: .activateAllWindows)` (переключает рабочий стол); если процесса нет — по `bundle_id` через `NSWorkspace`.

- [ ] **Step 1: Падающие тесты** `broker_answer_reaches_waiter`, `broker_expires_30s_after_shown_not_before` (paused clock; без `shown` не истекает), `two_requests_independent`, `permission_during_dictation_waits_until_idle` (фейк статуса), `paused_request_gets_no_decision_at_once`, `apply_settings_on_off_round_trip` (временный home: файлы появились/вернулись), `broken_file_reports_broken_and_untouched`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные; `cargo clippy` 0; коммит `feat(agents): agents service, permission broker and settings`.

### Task 7: Остров агентов

**Files:**
- Create: `src/overlay/agents/agentIsland.ts`, `src/overlay/agents/AgentEars.tsx`, `src/overlay/agents/AgentCard.tsx`, `src/overlay/agents/agentIsland.test.ts`, `src/overlay/agents/AgentCard.test.tsx`
- Modify: `src/lib/types.ts` (`AgentView`, `AgentRow`, `Card` как в Task 5), `src/overlay/Overlay.tsx`, `src/overlay/drop/Island.tsx`, `src/i18n/*` (строки ниже)

**Interfaces:**
- Consumes: событие `overlay://agents` (`AgentView`), `overlay://agent-hover`, команды Task 6.
- Produces:
```ts
export type AgentShow = { kind: "none" } | { kind: "ears"; dot: Dot; dots: Dot[] /*≤4, иначе 3 + "+"*/ } | { kind: "list"; rows: AgentRow[] } | { kind: "card"; card: Card };
export type Dot = "working" | "waiting" | "done";
export function agentShow(view: AgentView, dictation: OverlayState["kind"], hover: boolean): AgentShow;
// приоритет: dictation !== "hidden" → none; card → card; hover && rows → list; rows → ears; иначе none
```
- Строки (RU/EN): «просит разрешения»/«needs permission», «Запретить»/«Deny», «Разрешить»/«Allow», «Всегда»/«Always», «закончил»/«finished», «ждёт твоего ответа»/«is waiting for you», «Открыть»/«Open», «OK», «шаг {n} из {m}»/«step {n} of {m}», «Пока была пауза: {done} закончили, {waiting} ждут тебя»/«While paused: {done} finished, {waiting} waiting for you». Имя агента: «Claude Code», «Codex».
- Раскрытый остров — ширина `BASE_W` (420); содержимое с отступом сверху `notch.height + 12`; Капля 44 pt слева, текст справа (макет 2026-11-09). Ушки — существующая форма покоя (`islandMotion`), точки 6 pt в правом ушке, точка состояния 9 pt на Капле в левом.

- [ ] **Step 1: Падающие тесты** `dictation_hides_agents`, `card_beats_list`, `hover_opens_list`, `five_agents_show_three_dots_and_plus`, `nothing_drawn_above_notch_height` (рендер `AgentCard` с `notch={width:179,height:32}`: у всех текстовых узлов `offsetTop`/стиль `top` ≥ 32 — проверка по inline-стилю отступа контейнера), `permission_buttons_send_choice` (клик «Всегда» → `invoke("agent_permission",{id,choice:"always"})`), `always_hidden_without_suggestion`, `done_card_shows_summary_and_open`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные (`pnpm test`); коммит `feat(overlay): agents in the ears and island cards`.

### Task 8: Пасхалка

**Files:**
- Modify: `src/overlay/drop/dropModel.ts` (настроение `"dizzy"`: глаза-спирали, румянец, покачивание ±6° с частотой 2 Гц, 3 с), `src/overlay/drop/Drop.tsx`, `src/overlay/agents/agentIsland.ts` (`pokes(times: number[], now: number): boolean` — 5 нажатий за 2000 мс), `src/i18n/*` («Слишком много тычков. Дай секунду — вернусь к работе через три секунды.» / «Too many pokes. Give me a sec — back in three seconds.»)
- Modify: `src-tauri/src/app/overlay.rs` (ушки принимают клики, когда в них Капля)
- Test: `src/overlay/drop/dropModel.test.ts`, `src/overlay/agents/agentIsland.test.ts`

- [ ] **Step 1: Падающие тесты** `five_pokes_in_2s_make_dizzy`, `four_pokes_do_not`, `dizzy_ends_after_3s`, `no_dizzy_while_recording`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные; коммит `feat(drop): dizzy easter egg`.

### Task 9: Пауза в меню

**Files:**
- Modify: `src-tauri/src/app/tray.rs` (CheckMenuItem `agents_pause`: «Пауза агентов» / «Pause agents», показывается, только если подключён хоть один агент), `src-tauri/src/agents/service.rs` (`set_paused`)
- Test: `tray.rs` (тексты меню RU/EN), `service.rs`

- [ ] **Step 1: Падающие тесты** `menu_has_pause_when_agent_connected`, `pause_item_toggles_registry`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные; коммит `feat(tray): pause agents`.

### Task 10: Раздел «Агенты» и шаг онбординга

**Files:**
- Create: `src/settings/sections/Agents.tsx`, `src/settings/sections/Agents.test.tsx`, `src/settings/onboarding/steps/AgentsStep.tsx`
- Modify: `src/settings/App.tsx` (раздел после «Провайдер»), `src/settings/onboarding/steps.ts` (`"agents"` между `"key"` и `"trial"`, `dropAt("agents") = "center"`, пропуск шага, если `agents_status().detected` пуст), `src/settings/onboarding/Onboarding.tsx`, `src/lib/types.ts` (`AgentSettings`, `AgentsStatus`, `Link`), `src/lib/ipc.ts`, `src/i18n/*`
- Test: `Agents.test.tsx`, `src/settings/onboarding/onboarding.test.tsx`

- Копия: заголовок «Агенты»/«Agents»; Claude Code — «Капля скажет, когда Claude Code закончил или ждёт тебя, и даст разрешить из острова»; Codex — «Капля скажет, когда Codex закончил»; «Итог одной строкой (через Gemini)» — «Последнее сообщение агента уходит в {provider} по твоему ключу, чтобы сделать короткую строку»; ошибки: «Не нашёл Claude Code на этом Mac», «Не смог подключиться: файл настроек {agent} повреждён». Шаг онбординга: «Подключи агентов» / «Connect your agents», строка «Капля будет следить за ними и скажет, когда они закончат».

- [ ] **Step 1: Падающие тесты** `toggles_call_update_settings`, `missing_agent_disables_toggle_with_text`, `broken_link_shows_error`, `summary_toggle_names_provider`, `onboarding_skips_agents_step_when_nothing_detected`, `onboarding_shows_agents_step_between_key_and_trial` (в тесте использовать `until()` из `onboarding.test.tsx`, не фиксированные паузы).
- [ ] **Step 2–4:** FAIL → реализация → зелёные; коммит `feat(settings): Agents section and onboarding step`.

### Task 11: Клавиша «Задача»

**Files:**
- Modify: `src-tauri/src/types.rs` (`Mode::Task`), `src-tauri/src/settings.rs` (`Hotkeys.task: KeyId`, по умолчанию `RightCtrl` на macOS; старые файлы без поля → по умолчанию), `src-tauri/src/hotkey/router.rs` (`mode_of`), `src-tauri/src/pipeline/mod.rs` (`system_prompt(Mode::Task, …)` → `AGENT_TASK` всегда, даже при выключенной «чистке»; `Mode::Task` везде, где `Mode::Dictation` — тот же путь: живой черновик, проверка галлюцинаций), `src-tauri/src/pipeline/prompts/agent_task.md`, остальные места `match mode` (компилятор покажет), `src/lib/types.ts`, `src/settings/sections/Hotkeys.tsx`, `src/settings/sections/History.tsx` (метка «Задача»/«Task»), `src/overlay/drop/*` (цвет режима — рабочий `#A78BFA`, финальный в Task 12), `src/i18n/*`
- Test: `pipeline/mod.rs`, `hotkey/router.rs`, `settings.rs`, `src/settings/sections/sections1.test.tsx`

- Промпт `agent_task.md`: «Перепиши сказанное как задачу для ИИ-агента на том же языке, на котором говорили. Первая строка — суть задачи. Дальше пункты: что сделать, детали, ограничения, если они прозвучали. Ничего не добавляй от себя, не пиши вступлений и пояснений. Слова-паразиты и повторы убери.»

- [ ] **Step 1: Падающие тесты** `task_key_routes_to_task_mode`, `old_settings_get_default_task_key`, `task_prompt_used_even_without_cleanup`, `task_uses_dictation_path` (живой путь выбирается так же, как для диктовки), `hotkeys_section_has_three_keys`, `history_labels_task`.
- [ ] **Step 2–4:** FAIL → реализация → зелёные; коммит `feat: Task key — speech turned into a task for an agent`.

### Task 12: Цвета вместе с пользователем, документы, живая проверка

**Files:**
- Modify: цвета в `src/overlay/agents/*`, `src/overlay/drop/*` (только то, что утвердит пользователь), `README.md`, `README.en.md` (что умеет + приватность: итог агента уходит провайдеру), `docs/superpowers/specs/2026-11-09-agent-drop-design.md` (итоги, решения), `docs/superpowers/HANDOFF.md`, `CLAUDE.md`

- [ ] **Step 1: Цвета.** Показать 2–3 варианта палитры состояний и цвета режима «Задача» виджетом `visualize`, не похожие на присланное видео; поставить выбранный. Тест: `pnpm test` зелёный.
- [ ] **Step 2: Сборка для пользователя** — предупредить о Связке ключей, `pkill -x utter; scripts/dev-bundle.sh`, открыть.
- [ ] **Step 3: Живая проверка с пользователем** (каждый пункт — запись в журнал «проверено / не проверено»): включить Claude Code и Codex в «Агентах» → строки в файлах на месте; новая сессия Claude Code: «работает», лента, шаги, «готово» с итогом на другом рабочем столе; запрос разрешения (режим с вопросами) → «Разрешить», «Всегда» (правило появилось в `.claude/settings.local.json` проекта), «Запретить»; таймаут 30 с → вопрос в Claude; Codex → «готово»; пауза и строка после неё; пасхалка; «Открыть» переключает рабочий стол; шаг онбординга; клавиша «Задача» в окне агента; выключение → файлы вернулись как были.
- [ ] **Step 4: Документы** (README, спецификация, HANDOFF, CLAUDE.md), коммит и push `docs: the Drop and AI agents`.
