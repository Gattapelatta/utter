# Агенты: новый вид и Codex наравне с Claude Code — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Убрать «Итог одной строкой», подключать Codex хуками наравне с Claude Code, а агентов в знакомстве и настройках показывать двумя колонками со значками и переключателями цвета бренда.

**Architecture:** Codex ставится в `~/.codex/hooks.json` тем же кодом, что Claude Code в `~/.claude/settings.json` (общий установщик хуков); команда хука указывает на неизменную ссылку `~/.codex/utter/utter-agent`, которую Utter обновляет при запуске. События Codex идут через `utter-agent codex-hook` и общий разбор `parse_hook`. Страница рисует агентов одним компонентом `AgentPicker`.

**Tech Stack:** Rust (Tauri 2, serde_json), React + TypeScript, Tailwind, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-13-agents-look-and-codex-hooks-design.md`

## Global Constraints

- Ветка `feat/agents-look-codex`; коммит и `git push` после каждой задачи; в `main` и выпуск — только с «да» пользователя. TDD; clippy 0; `pnpm test`, `cargo test --lib` зелёные.
- Цвета включённого переключателя: Claude Code `#D97757`, Codex `#4C6EF5`, ручка белая; выключенный — как сейчас.
- Колонки: промежуток 72 pt, значок 34 pt без подложки, название 15 px / 500, без карточек и пояснений.
- Хуки Codex: `UserPromptSubmit`, `PreToolUse`, `PostToolUse` (`matcher` `^update_plan$`), `PermissionRequest` (`timeout` 120), `Stop`, `SessionEnd`. Команда: `<HOME>/.codex/utter/utter-agent codex-hook <Event>` (без оболочечных конструкций; путь в кавычках, только если в нём пробел).
- Для Codex нет «Всегда»; `Stop` печатает `{}`.
- Ничего не скачивать без «да» пользователя (значки — Задача 5).

## Review Focus

1. **Доверие Codex не должно слетать:** текст хука одинаков при любом пути Utter (dev-сборка ↔ `/Applications`). Тест — Задача 3 (`codex_hooks_same_text_for_any_helper`).
2. **Переход со старой схемы:** у пользователя, у которого стоял наш `notify`, после запуска `notify` возвращается прежний (Computer Use), хуки ставятся. Тест — Задача 3.
3. **Чужое в `hooks.json` и `config.toml` не трогается**, выключение Codex возвращает файл как был. Тест — Задача 3.
4. **Старый файл настроек с `agents.summary` и `agents_codex_prev_notify`** грузится без ошибок. Тест — Задача 1.
5. **Codex без событий** (не доверил хуки): подсказка видна; после первого события — исчезает и не возвращается после перезапуска. Тесты — Задачи 4 и 5.

---

### Task 1: Убрать «Итог одной строкой»

**Files:**
- Delete: `src-tauri/src/agents/summary.rs`, `src-tauri/src/pipeline/prompts/agent_summary.md`
- Modify: `src-tauri/src/agents/mod.rs`, `src-tauri/src/settings.rs` (`AgentSettings` без `summary`; serde пропускает старое поле), `src-tauri/src/agents/service.rs` (нет запроса итога на `Finished`, `SUMMARY_TIMEOUT`, `summary_may_use_model` и их тесты), `src-tauri/src/agents/registry.rs` (`AgentRow.summary`, `set_summary`), `src-tauri/src/providers/budget.rs` (вид запроса «итог агента», если он отдельный), `src-tauri/src/app/tray.rs:441`
- Modify: `src/lib/types.ts` (`AgentRow.summary`, `AgentSettings.summary`, `AGENTS_OFF`), `src/overlay/agents/AgentCard.tsx` (карточка `done` без строки итога; `items` для `done`), `src/settings/sections/AgentToggles.tsx` (строки «Итог» нет), `src/i18n/ru.json`, `src/i18n/en.json` (`agents.summary`, `agents.summary_hint`)
- Test: `settings.rs`, `registry.rs`, `AgentCard.test.tsx`, тесты страницы, где есть `summary`

**Interfaces:**
- Produces: `AgentSettings { claude_code: bool, codex: bool }` (Rust и TS); `AgentRow` без `summary`.

- [ ] **Step 1: Write the failing tests.** `settings.rs`: `old_settings_with_summary_still_load` — файл `{"agents":{"claude_code":true,"summary":true},"agents_codex_prev_notify":["x"]}` грузится, `agents == AgentSettings { claude_code: true, codex: false }`. `AgentCard.test.tsx`: `done_card_has_title_and_buttons_only` — карточка `done` даёт заголовок «shop Claude Code finished», кнопки «Open»/«OK», нет `[data-agent-summary]`; `agentHeight({card: done})` = высота заголовка + кнопок (без строки итога).
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib settings` и `pnpm test AgentCard` — Expected: FAIL (поле `summary` ещё есть / строка итога рисуется).
- [ ] **Step 3: Implement:** удалить всё перечисленное; `grep -rn "summary" src src-tauri/src` не должен находить ничего про итог агента.
- [ ] **Step 4: Run** весь `cargo test --lib`, clippy, `pnpm test`, `pnpm exec tsc --noEmit` — Expected: зелёное, 0 предупреждений.
- [ ] **Step 5: Commit** `feat(agents): drop the one-line summary — the user's call` + push.

### Task 2: Разбор событий Codex

**Files:**
- Modify: `src-tauri/src/agents/event.rs`
- Create: `src-tauri/src/agents/fixtures/codex_pre_tool_use_patch.json`, `codex_post_tool_use_plan.json`, `codex_permission_request.json`, `codex_stop.json` (поля из справочника: `session_id`, `cwd`, `hook_event_name`, `turn_id`, `model`, `permission_mode`, `tool_name`, `tool_use_id`, `tool_input`, `last_assistant_message`)

**Interfaces:**
- Produces: `pub fn parse_hook(agent: Agent, hook_event: &str, payload: &Value, lang: Lang) -> Option<AgentEvent>`; `parse_claude(e, p, l)` = `parse_hook(Agent::ClaudeCode, e, p, l)` (оставить имя для вызовов и тестов); `parse_codex` (старый `notify`) — без изменений.

- [ ] **Step 1: Write the failing tests:**
  - `codex_hook_events` — `UserPromptSubmit` → `Started`; `Stop` → `Finished { last_message }`; `SessionEnd` → `Ended`; ключ `SessionKey { agent: Codex, id: session_id }`.
  - `codex_apply_patch_names_the_file` — `tool_name: "apply_patch"`, `tool_input.command: "*** Begin Patch\n*** Update File: src/a/billing.ts\n@@…"` → `Action("Правит billing.ts")` (RU); без строки файла → `Action("Правит")`.
  - `codex_bash_runs` — `Bash`, `command: "npm test"` → `Action("Running npm test")` (EN).
  - `codex_update_plan_replaces` — `update_plan {"explanation":"x","plan":[{"step":"a","status":"completed"},{"step":"b","status":"in_progress"}]}` → `Plan(Replace([a Done, b Active]))`; `tool_input` строкой с тем же JSON — тот же результат.
  - `codex_permission_has_no_always` — `PermissionRequest`, `Bash`, `command: "git push"` → `Permission { tool: "Bash", detail: "git push", suggestion: None }` даже если в payload есть `permission_suggestions`; без `command` → `detail` из `tool_input.description`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib agents::event` — Expected: FAIL (нет `parse_hook`).
- [ ] **Step 3: Implement.** Общая функция с ветками по `agent` только там, где Codex отличается: `apply_patch` (первая строка `*** (Update|Add|Delete) File: <path>` → `basename`), `plan`/`step` в `plan_update`, строковый `tool_input` разбирается как JSON, у Codex `suggestion` всегда `None`. `Notification` у Codex не приходит — общая ветка не мешает.
- [ ] **Step 4: Run** `cargo test --lib agents::` и clippy — Expected: PASS, 0.
- [ ] **Step 5: Commit** `feat(agents): Codex hook events — actions, plan, permissions, finished` + push.

### Task 3: Установка хуков Codex и ссылка на помощника

**Files:**
- Modify: `src-tauri/src/agents/install.rs` (общий установщик хуков; Codex-обёртки), `src-tauri/src/agents/service.rs` (`codex_file` → `~/.codex/hooks.json`; `codex_link`, `apply_in`; ссылка; переход со старого `notify` в `~/.codex/config.toml`)

**Interfaces:**
- Produces (install.rs): `pub const CODEX_EVENTS: [&str; 6]`; `pub fn codex_hooks_install(current: Option<&str>, link: &str) -> Result<String, InstallError>`; `pub fn codex_hooks_installed(current: &str, link: &str) -> bool`; `pub fn codex_hooks_uninstall(current: &str) -> Result<String, InstallError>`; `fn codex_command(link: &str, event: &str) -> String` → `"<link> codex-hook <Event>"`. Старые `codex_install/codex_installed` (`notify`) удаляются, `codex_uninstall` (снять `notify`, вернуть прежний) остаётся для перехода.
- Produces (service.rs): `pub fn codex_helper_link(home: &Path) -> PathBuf` = `home/.codex/utter/utter-agent`; `fn point_link(link: &Path, helper: &str) -> Result<(), String>` (создать папку, заменить ссылку атомарно, если ведёт не туда).

- [ ] **Step 1: Write the failing tests** (`install.rs`): `codex_hooks_install_creates_file` (все 6 событий, `PostToolUse.matcher == "^update_plan$"`, `PermissionRequest.timeout == 120`), `codex_hooks_keep_foreign_and_do_not_duplicate`, `codex_hooks_uninstall_gives_back_the_same_text`, `codex_hooks_same_text_for_any_helper` (вход — путь ссылки, не путь Utter: `codex_hooks_install(None, link)` не зависит от того, где Utter). (`service.rs`, во временной папке): `codex_migrates_from_notify` — `config.toml` с нашим `notify` и `prev = Some(["SkyComputerUseClient","turn-ended"])` → после `apply_in` в `config.toml` `notify = ["SkyComputerUseClient","turn-ended"]`, в `hooks.json` наши хуки, ссылка ведёт на `helper`; `codex_off_removes_hooks_and_link`; `codex_link_follows_a_moved_helper` (второй `apply_in` с другим `helper` → ссылка на новый путь, `hooks.json` не переписан — тот же текст).
- [ ] **Step 2: Run** `cargo test --lib agents::install agents::service` — Expected: FAIL.
- [ ] **Step 3: Implement.** Вынести из Claude-установщика общий `hooks_install(current, events, command_of, group_extra)` / `hooks_installed` / `hooks_uninstall` (Claude: `STEPS_MATCHER`, `PERMISSION_TIMEOUT`; Codex: `^update_plan$`, 120). `apply_in`: при включённом и найденном Codex — `point_link`, затем `hooks.json`, затем снять наш `notify` из `config.toml`, если он там; при выключенном — снять наши хуки, удалить ссылку, снять наш `notify`. `agents_codex_prev_notify` после перехода — `None`.
- [ ] **Step 4: Run** весь `cargo test --lib`, clippy — Expected: PASS, 0.
- [ ] **Step 5: Commit** `feat(agents): connect Codex through its hooks (stable link, migration from notify)` + push.

### Task 4: `utter-agent codex-hook`, приём событий и «Codex отозвался»

**Files:**
- Modify: `src-tauri/src/bin/utter-agent.rs`, `src-tauri/src/agents/wire.rs` (функция вывода для Codex `Stop`), `src-tauri/src/agents/service.rs` (разбор `Source::Codex`: `event == "notify"` → `parse_codex`, иначе `parse_hook(Agent::Codex, …)`; первое событие хука Codex → `settings.agents_codex_heard = true` и сохранить), `src-tauri/src/settings.rs` (`agents_codex_heard: bool`, по умолчанию `false`), `AgentsStatus` (`codex_heard: bool`), `src/lib/types.ts`

**Interfaces:**
- Produces: `wire::stop_output() -> &'static str` = `"{}"`; `utter-agent codex-hook <Event>` (stdin JSON → `Request { source: Codex, event, wants_reply: event == "PermissionRequest" }`; ответ — `hook_output(reply, None)`; для `Stop` печатает `{}`); `AgentsStatus.codex_heard`.

- [ ] **Step 1: Write the failing tests:** `wire.rs` — `codex_permission_output_has_no_always` (`hook_output(Allow{always:true}, None)` = просто `allow`); `service.rs` — `codex_hook_request_is_parsed_as_hook` (запрос `Source::Codex`, `event: "Stop"` → `Finished`), `codex_notify_still_parsed`; `first_codex_hook_event_marks_heard` (чистая функция `heard_after(settings, &req) -> Option<Settings>`: `Some` с `agents_codex_heard = true` только для первого хука Codex, `None` для `notify`, Claude и при уже `true`); `settings.rs` — `codex_heard_defaults_off`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement** по Interfaces; `utter-agent` — режим `codex-hook` рядом с `claude` (общая функция чтения stdin и отправки), `Stop` → `println!("{}", wire::stop_output())`.
- [ ] **Step 4: Run** весь `cargo test --lib`, clippy — Expected: PASS, 0.
- [ ] **Step 5: Commit** `feat(agents): utter-agent codex-hook; Utter takes Codex hook events and remembers that Codex answered` + push.

### Task 5: Вид — значки, переключатели цвета бренда, две колонки

**Files:**
- Create: `src/settings/sections/AgentPicker.tsx`, `src/settings/sections/agentIcons.tsx` (`ClaudeIcon`, `CodexIcon` — векторные знаки), `src/settings/sections/AgentPicker.test.tsx`
- Modify: `src/settings/ui/Switch.tsx` (необязательные `color?: string; knob?: string` для включённого состояния), `src/settings/sections/AgentToggles.tsx` (рисует `AgentPicker`; в знакомстве — только найденные), `src/settings/onboarding/Onboarding.tsx` (шаг `agents` — `AgentPicker` без обёртки `w-[440px] text-left`), `src/i18n/*.json` (удалить `agents.claude_code_hint`, `agents.codex_hint`; добавить `agents.codex_trust`: «В Codex: Settings → Hooks → доверить Utter» / «In Codex: Settings → Hooks → trust Utter»)

**Interfaces:**
- Consumes: `AgentsStatus.codex_heard` (Задача 4), `AgentSettings` без `summary` (Задача 1).
- Produces: `export function AgentPicker({ onboarding }: { onboarding?: boolean })`; `BRAND = { claude_code: "#D97757", codex: "#4C6EF5" }`.

- [ ] **Step 1: Спросить пользователя** про скачивание значков: источник (официальные знаки или набор знаков брендов), имена файлов, размер. Без «да» — нарисовать знаки самим по образцу, без скачивания.
- [ ] **Step 2: Write the failing tests** (`AgentPicker.test.tsx`): `two_columns_with_icons_and_names` (оба найдены → 2 колонки `[data-agent-col]`, в каждой `svg` и название, нет подписей-пояснений); `onboarding_hides_missing_agent` (Codex не найден → в знакомстве одна колонка, в настройках две, у Codex переключатель недоступен и строка «Не нашёл Codex…»); `switch_takes_brand_colour_when_on` (`style.background` включённого Claude — `#D97757` / `rgb(217, 119, 87)`, Codex — `#4C6EF5`; выключенный — без инлайн-цвета); `codex_trust_hint_only_while_unheard` (Codex включён, `codex_heard: false` → строка `agents.codex_trust`; `true` или Codex выключен → нет); `no_summary_row`.
- [ ] **Step 3: Run** `pnpm test AgentPicker` — Expected: FAIL.
- [ ] **Step 4: Implement** по Global Constraints (колонки `flex justify-center gap-[72px]`, колонка `flex-col items-center gap-2.5`).
- [ ] **Step 5: Run** `pnpm test`, `pnpm exec tsc --noEmit` — Expected: PASS.
- [ ] **Step 6: Commit** `feat(settings): agents as two columns with their marks and brand-coloured switches` + push.

### Task 6: Сборка, проверка вживую, итоги

- [ ] **Step 1:** Полные проверки (`cargo test --lib`, clippy 0, `pnpm test`).
- [ ] **Step 2:** `pkill -x utter; scripts/dev-bundle.sh`; предупредить о Связке ключей; дождаться ухода `SecurityAgent`; `open Utter.app` (дважды — окно настроек).
- [ ] **Step 3:** Проверить файлы: `~/.codex/hooks.json` (если пользователь включит Codex), ссылка `~/.codex/utter/utter-agent`, `config.toml` — `notify` Computer Use на месте, `~/.claude/settings.json` не изменился.
- [ ] **Step 4:** Снимки окна настроек (`screencapture -l`): раздел «Агенты»; шаг знакомства — через `debug-eval` (открыть шаг `agents`, если есть способ; иначе — только Vitest и снимок раздела, отметить в итогах).
- [ ] **Step 5:** Итоги в §7 спецификации, HANDOFF, `CLAUDE.md`; commit + push.
- [ ] **Step 6:** Финальное свежее ревью ветки; правки Critical/Important с тестами; вечером с пользователем — Codex вживую (доверие хукам, «работает», действия, «Лесенка», разрешение из острова, «закончил»); слияние — только с «да».
