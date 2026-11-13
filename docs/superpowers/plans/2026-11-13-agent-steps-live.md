# Шаги Claude Code в острове вживую — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Остров при наведении показывает «Лесенку» — номер текущего шага, прошлый шаг, текущий с действием на лету и следующий — по плану Claude Code, который Utter теперь отслеживает сам.

**Architecture:** Rust хранит у сессии список шагов (`plan: Vec<PlanStep>`) и применяет к нему события плана (весь список / «следующий» / «не по порядку» / «всё готово») чистой функцией `agents::plan::apply`; список целиком уходит странице в `AgentRow`. Страница превращает его в лесенку чистой функцией `ladderOf` и рисует в `AgentList`.

**Tech Stack:** Rust (Tauri 2, serde), React + TypeScript, Vitest (jsdom).

**Spec:** `docs/superpowers/specs/2026-11-13-agent-steps-live-design.md`

## Global Constraints

- Ветка `feat/agent-steps-live`; коммит и `git push` после каждой задачи; в `main` — только с «да» пользователя; выпуск — только с «да».
- TDD: сначала падающий тест. `cargo clippy --all-targets` — 0 предупреждений; `pnpm test` и `cargo test --lib` зелёные.
- Команды: `export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH" COREPACK_ENABLE_DOWNLOAD_PROMPT=0`.
- Статусы шага для страницы: `"pending" | "active" | "done"` (serde `snake_case`).
- Список ≤ 50 шагов, название ≤ 200 символов (лишнее отбрасывается при разборе).
- `STEPS_MATCHER = "TodoWrite|mcp__ccd_turn__update_plan|mcp__ccd_turn__wrap_up"`.
- Заголовок первой строки: `«<проект> Claude Code · 3 из 9»` — `agents.step` = `"{n} из {m}"` / `"{n} of {m}"`; `n` — номер текущего шага.
- Высота острова: «Лесенка» добавляет не больше одной строки (16 pt) к нынешней первой строке; ничего другого в форме острова не менять.
- Ничего не рисовать в верхних 32 pt под чёлкой (панель уже начинается с `top(notch)`; не трогать).

## Review Focus

1. **Старый хук у пользователя не обновится:** `install::claude_installed` сверяет только команды, не `matcher` — с новым `STEPS_MATCHER` Utter решит, что всё уже установлено, и `wrap_up` так и не начнёт приходить. Ожидание: настройки со старым `matcher` считаются неустановленными и переписываются при запуске. Тест — в Задаче 2.
2. **Ответ пользователя посреди работы** («Да, верно» → `UserPromptSubmit`) не стирает незаконченный план; следующий `advance` продолжает счёт. Тест — в Задаче 3.
3. **Utter запущен посреди работы Claude:** отметка `advance` / `done` / `wrap_up` без списка ничего не ломает и не создаёт пустой план. Тест — в Задаче 1.
4. **Длинные названия шагов и действие:** текущая строка не переносится и не раздвигает остров; действие остаётся видно. Тест — в Задаче 5 (одна строка, `nowrap`, высота по `agentHeight`).
5. **Странный ввод:** `items` не массив, шаг без `content`/`status`, неизвестный статус, номер `0` или больше длины в `done`/`start` — не паника, шаг `pending`, номер пропускается. Тесты — в Задачах 1 и 2.

---

### Task 1: Чистая модель плана `agents/plan.rs`

**Files:**
- Create: `src-tauri/src/agents/plan.rs`
- Modify: `src-tauri/src/agents/mod.rs` (добавить `pub mod plan;`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum PlanStatus { Pending, Active, Done }`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] pub struct PlanStep { pub title: String, pub status: PlanStatus }`
  - `#[derive(Clone, Debug, PartialEq)] pub enum PlanUpdate { Replace(Vec<PlanStep>), Advance, Mark { done: Vec<u32>, start: Option<u32> }, Finish }` (номера с 1)
  - `pub fn apply(plan: &mut Vec<PlanStep>, update: PlanUpdate)`
  - `pub fn finished(plan: &[PlanStep]) -> bool` — непустой и все `Done`.
  - `pub const MAX_STEPS: usize = 50; pub const MAX_TITLE: usize = 200;`

- [ ] **Step 1: Write the failing tests** (в `plan.rs`, `#[cfg(test)] mod tests`; помощник `fn p(s: &str) -> Vec<PlanStep>` строит план из строки вида `"dap"` — `d`=Done, `a`=Active, `p`=Pending, названия `"1".."n"`; `fn st(plan) -> String` обратно)

```rust
#[test] fn replace_takes_the_new_list() { let mut v = p("dd"); apply(&mut v, PlanUpdate::Replace(p("ap"))); assert_eq!(st(&v), "ap"); }
#[test] fn advance_completes_active_and_starts_next_pending() { let mut v = p("dap"); apply(&mut v, PlanUpdate::Advance); assert_eq!(st(&v), "dda"); }
#[test] fn advance_without_active_starts_first_pending() { let mut v = p("dpp"); apply(&mut v, PlanUpdate::Advance); assert_eq!(st(&v), "dap"); }
#[test] fn advance_on_last_step_completes_it() { let mut v = p("da"); apply(&mut v, PlanUpdate::Advance); assert_eq!(st(&v), "dd"); }
#[test] fn mark_out_of_order() { let mut v = p("papp"); apply(&mut v, PlanUpdate::Mark { done: vec![2], start: Some(4) }); assert_eq!(st(&v), "pdpa"); }
#[test] fn start_leaves_one_active() { let mut v = p("ap"); apply(&mut v, PlanUpdate::Mark { done: vec![], start: Some(2) }); assert_eq!(st(&v), "pa"); }
#[test] fn mark_ignores_numbers_outside_the_list() { let mut v = p("ap"); apply(&mut v, PlanUpdate::Mark { done: vec![0, 3], start: Some(9) }); assert_eq!(st(&v), "ap"); }
#[test] fn finish_completes_everything() { let mut v = p("dap"); apply(&mut v, PlanUpdate::Finish); assert_eq!(st(&v), "ddd"); assert!(finished(&v)); }
#[test] fn marks_without_a_plan_do_nothing() {
    for u in [PlanUpdate::Advance, PlanUpdate::Finish, PlanUpdate::Mark { done: vec![1], start: Some(1) }] {
        let mut v = vec![]; apply(&mut v, u); assert!(v.is_empty());
    }
    assert!(!finished(&[]));
}
#[test] fn status_serializes_for_the_page() { assert_eq!(serde_json::to_value(PlanStatus::Active).unwrap(), "active"); }
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib agents::plan` — Expected: FAIL (не компилируется: нет `apply`).

- [ ] **Step 3: Implement** `apply` и `finished` по таблице спецификации §3.3. `Advance`: найти первый `Active` (индекс `a`) → `Done`; затем первый `Pending` с индексом > `a` (без активного — первый `Pending` вообще) → `Active`. `Mark`: номера `1..=len` из `done` → `Done`; при `start = Some(k)` в пределах — все `Active`, кроме `k`, → `Pending`, шаг `k` → `Active`.

- [ ] **Step 4: Run** тот же тест — Expected: PASS; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` — 0 warnings.

- [ ] **Step 5: Commit** `feat(agents): plan model — steps follow advance/done/start/wrap_up` + `git push`.

### Task 2: Разбор событий плана и хук `wrap_up`

**Files:**
- Modify: `src-tauri/src/agents/event.rs` (`EventKind::Steps` → `EventKind::Plan(PlanUpdate)`, функция `steps` → `plan_update`)
- Modify: `src-tauri/src/agents/install.rs:15` (`STEPS_MATCHER`), `install::claude_installed` (сверять `matcher` группы `PostToolUse`)
- Create: `src-tauri/src/agents/fixtures/claude_post_tool_use_plan_advance.json` (реальная форма из замера: `"tool_name":"mcp__ccd_turn__update_plan","tool_input":{"advance":true}`, остальные поля — как в `claude_post_tool_use_todo.json`)
- Test: `event.rs`, `install.rs`

**Interfaces:**
- Consumes: `plan::{PlanStep, PlanStatus, PlanUpdate, MAX_STEPS, MAX_TITLE}` (Задача 1).
- Produces: `EventKind::Plan(PlanUpdate)` (вместо `EventKind::Steps`); `fn plan_update(tool: &str, input: &Value) -> Option<PlanUpdate>`.

- [ ] **Step 1: Write the failing tests** в `event.rs` (заменить `todo_write_gives_steps` и `plan_items_give_steps_too`):

```rust
#[test] fn todo_write_replaces_the_plan() {
    let EventKind::Plan(PlanUpdate::Replace(v)) = claude("PostToolUse", "post_tool_use_todo").kind else { panic!() };
    assert_eq!(v.len(), 5);
    assert_eq!(v[3], PlanStep { title: "d".into(), status: PlanStatus::Active });
    assert_eq!(v[0].status, PlanStatus::Done);
}
#[test] fn plan_tool_calls_as_measured() {
    let up = |tool: &str, input: Value| match parse_claude("PostToolUse", &json!({"session_id":"s","cwd":"/p","tool_name":tool,"tool_input":input}), Lang::En).map(|e| e.kind) { Some(EventKind::Plan(u)) => Some(u), _ => None };
    let plan = "mcp__ccd_turn__update_plan";
    assert!(matches!(up(plan, json!({"items":[{"content":"a","status":"in_progress"}],"scope":"continuation"})), Some(PlanUpdate::Replace(v)) if v[0].title == "a"));
    assert_eq!(claude("PostToolUse", "post_tool_use_plan_advance").kind, EventKind::Plan(PlanUpdate::Advance));
    assert_eq!(up(plan, json!({"done":[2],"start":4})), Some(PlanUpdate::Mark { done: vec![2], start: Some(4) }));
    assert_eq!(up(plan, json!({"done":[1]})), Some(PlanUpdate::Mark { done: vec![1], start: None }));
    assert_eq!(up("mcp__ccd_turn__wrap_up", json!({"recap":"r"})), Some(PlanUpdate::Finish));
    assert_eq!(up(plan, json!({})), None);
}
#[test] fn odd_plan_input_does_not_break() {
    let up = |input: Value| match parse_claude("PostToolUse", &json!({"session_id":"s","cwd":"/p","tool_name":"TodoWrite","tool_input":input}), Lang::En).map(|e| e.kind) { Some(EventKind::Plan(PlanUpdate::Replace(v))) => Some(v), _ => None };
    assert_eq!(up(json!({"todos":"x"})), None);
    let v = up(json!({"todos":[{}, {"content":"b","status":"weird"}]})).unwrap();
    assert_eq!((v[0].title.as_str(), v[0].status, v[1].status), ("", PlanStatus::Pending, PlanStatus::Pending));
    let many: Vec<Value> = (0..60).map(|i| json!({"content": "x".repeat(300 + i), "status":"pending"})).collect();
    let v = up(json!({"todos": many})).unwrap();
    assert_eq!(v.len(), MAX_STEPS);
    assert_eq!(v[0].title.chars().count(), MAX_TITLE);
}
```

В `install.rs` тест:

```rust
#[test] fn old_steps_matcher_counts_as_not_installed() {
    let new = claude_install(None, HELPER).unwrap();
    assert!(new.contains("mcp__ccd_turn__wrap_up"));
    let old = new.replace("|mcp__ccd_turn__wrap_up", "");
    assert!(!claude_installed(&old, HELPER));
    assert!(claude_installed(&claude_install(Some(&old), HELPER).unwrap(), HELPER));
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib agents::` — Expected: FAIL (нет `EventKind::Plan`).

- [ ] **Step 3: Implement.** `plan_update`: `wrap_up` (имя кончается на `__wrap_up`) → `Finish`; есть массив `todos` или `items` → `Replace` (до `MAX_STEPS`, `content` обрезан до `MAX_TITLE` символов; `completed` → `Done`, `in_progress` → `Active`, прочее → `Pending`); `todos`/`items` есть, но не массив → `None`; `advance: true` → `Advance`; есть `done` или `start` → `Mark` (нечисловые элементы пропускаются); иначе `None`. В `parse_claude`: `"PostToolUse" => EventKind::Plan(plan_update(tool, input)?)`. `claude_installed`: дополнительно требовать, чтобы группа с нашей командой в `PostToolUse` имела `matcher == STEPS_MATCHER`. Обновить `registry.rs`, где он пока использует `EventKind::Steps`, минимально, чтобы компилировалось (`EventKind::Plan(_) => working(row)`; полная логика — Задача 3).

- [ ] **Step 4: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib` — Expected: PASS; clippy — 0. Тесты реестра, которые строят `EventKind::Steps`, переписать здесь на `EventKind::Plan(PlanUpdate::Replace(…))` ровно настолько, чтобы компилировались (их проверки на `steps` — в Задаче 3).

- [ ] **Step 5: Commit** `feat(agents): parse plan updates (advance/done/start/wrap_up); hook listens to wrap_up` + push.

### Task 3: Реестр хранит план

**Files:**
- Modify: `src-tauri/src/agents/registry.rs` (`AgentRow.steps` → `AgentRow.plan: Vec<PlanStep>`; ветки `Started` и `Plan`; тесты `steps_and_last_two_actions` и соседний)
- Modify: `src-tauri/src/agents/service.rs:1085` (конструктор `AgentRow` в тесте)

**Interfaces:**
- Consumes: `plan::{apply, finished, PlanStep}`, `EventKind::Plan(PlanUpdate)`.
- Produces: `AgentRow { …, plan: Vec<PlanStep>, … }` — сериализуется как `"plan":[{"title":"…","status":"active"}]` (поля `steps` больше нет).

- [ ] **Step 1: Write the failing tests** в `registry.rs`:

```rust
#[test] fn plan_follows_advance() {
    let t = Instant::now(); let mut r = Registry::default();
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Replace(vec![step("x", PlanStatus::Active), step("y", PlanStatus::Pending)]))), t);
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Advance)), t);
    let v = r.view(t);
    assert_eq!(row(&v, "a").plan.iter().map(|s| s.status).collect::<Vec<_>>(), vec![PlanStatus::Done, PlanStatus::Active]);
}
#[test] fn user_reply_keeps_an_unfinished_plan() {
    let t = Instant::now(); let mut r = Registry::default();
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Replace(vec![step("x", PlanStatus::Active), step("y", PlanStatus::Pending)]))), t);
    r.apply(ev("a", EventKind::Started), t);
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Advance)), t);
    assert_eq!(row(&r.view(t), "a").plan[1].status, PlanStatus::Active);
}
#[test] fn new_prompt_clears_a_finished_plan() {
    let t = Instant::now(); let mut r = Registry::default();
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Replace(vec![step("x", PlanStatus::Active)]))), t);
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Finish)), t);
    r.apply(ev("a", EventKind::Started), t);
    assert!(row(&r.view(t), "a").plan.is_empty());
}
#[test] fn row_serializes_plan_for_the_page() {
    let t = Instant::now(); let mut r = Registry::default();
    r.apply(ev("a", EventKind::Plan(PlanUpdate::Replace(vec![step("x", PlanStatus::Active)]))), t);
    let j = serde_json::to_value(row(&r.view(t), "a")).unwrap();
    assert_eq!(j["plan"], json!([{"title":"x","status":"active"}]));
    assert!(j.get("steps").is_none());
}
```

(`fn step(t: &str, s: PlanStatus) -> PlanStep` — помощник в тестах. Метод представления называется так, как в файле сейчас; если не `view`, использовать существующий.)

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib agents::registry` — Expected: FAIL.

- [ ] **Step 3: Implement.** `Started`: `if plan::finished(&row.plan) { row.plan.clear(); }` (действия и итог сбрасываются как сейчас). `Plan(u)`: `working(row); plan::apply(&mut row.plan, u);`. Старые тесты на `steps` переписать на `plan`.

- [ ] **Step 4: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib` и clippy — Expected: PASS, 0 warnings.

- [ ] **Step 5: Commit** `feat(agents): sessions keep the plan; a reply mid-task keeps it` + push.

### Task 4: Лесенка на странице — модель и строки

**Files:**
- Modify: `src/lib/types.ts` (`AgentRow.steps` → `plan: PlanStep[]`; `export type PlanStep = { title: string; status: "pending" | "active" | "done" }`)
- Modify: `src/overlay/agents/agentIsland.ts` (добавить `ladderOf`)
- Modify: `src/i18n/ru.json`, `src/i18n/en.json`
- Test: `src/overlay/agents/agentIsland.test.ts`; поправить фикстуры `steps` в `AgentCard.test.tsx`, `AgentStage.test.tsx`, `Overlay.test.tsx` (и где ещё найдёт `rg "steps:" src`)

**Interfaces:**
- Produces: `export type Ladder = { n: number; m: number; prev: string | null; current: string | null; next: string | null; allDone: boolean }`; `export function ladderOf(plan: PlanStep[]): Ladder | null` (пустой план → `null`).
- Строки: `agents.step` = `"{n} из {m}"` / `"{n} of {m}"`; `agents.all_steps_done_one/_few/_many/_other` — RU: `"{n} шаг из {n} готов"`, `"Все {n} шага готовы"`, `"Все {n} шагов готовы"`, `"Все {n} шага готовы"`; EN: `"The step is done"`, `"All {n} steps done"` ×3.

- [ ] **Step 1: Write the failing tests** (`agentIsland.test.ts`):

```ts
const P = (s: string) => s.split("").map((c, i) => ({ title: `s${i + 1}`, status: ({ d: "done", a: "active", p: "pending" } as const)[c as "d"] }));
it("ladder_middle_step", () => expect(ladderOf(P("ddapp"))).toEqual({ n: 3, m: 5, prev: "s2", current: "s3", next: "s4", allDone: false }));
it("ladder_first_step_has_no_prev", () => expect(ladderOf(P("app"))).toMatchObject({ n: 1, prev: null, next: "s2" }));
it("ladder_last_step_has_no_next", () => expect(ladderOf(P("dda"))).toMatchObject({ n: 3, prev: "s2", next: null }));
it("ladder_without_active_takes_first_unfinished", () => expect(ladderOf(P("dpp"))).toMatchObject({ n: 2, current: "s2", prev: "s1", next: "s3" }));
it("ladder_out_of_order_prev_is_nearest_done", () => expect(ladderOf(P("dpap"))).toMatchObject({ n: 3, prev: "s1", next: "s4" }));
it("ladder_all_done", () => expect(ladderOf(P("ddd"))).toEqual({ n: 3, m: 3, prev: null, current: null, next: null, allDone: true }));
it("ladder_no_plan", () => expect(ladderOf([])).toBeNull());
```

- [ ] **Step 2: Run** `pnpm test agentIsland` — Expected: FAIL.

- [ ] **Step 3: Implement** `ladderOf`: текущий — первый `active`, иначе первый `pending`; нет — `allDone` (`n = m`). `prev` — ближайший `done` перед текущим; `next` — первый не-`done` после текущего. Типы и строки — как в Interfaces; фикстуры тестов: `steps: [2, 4]` → `plan: []` (или осмысленный план, где тест проверял шаг).

- [ ] **Step 4: Run** `pnpm test` — Expected: PASS целиком (рисовка ещё старая, но на `plan`: временно `AgentList` берёт шаг из `ladderOf(r.plan)` — `n`/`m`, чтобы компилировалось).

- [ ] **Step 5: Commit** `feat(island): ladder model from the agent's plan` + push.

### Task 5: `AgentList` рисует лесенку

**Files:**
- Modify: `src/overlay/agents/AgentCard.tsx` (`items` для `rows`, `AgentList`)
- Test: `src/overlay/agents/AgentCard.test.tsx`

**Interfaces:**
- Consumes: `ladderOf`, `Ladder` (Задача 4); `pastOf` (есть).
- Produces: разметка с атрибутами `data-agent-step="prev" | "current" | "next" | "all-done"`, `data-agent-action` (действие в текущей строке), `data-agent-row`.

Правила (спецификация §3.1–3.2):
- Первая строка, есть план: заголовок `<b>проект</b> Claude Code · {n} из {m}` (при `allDone` — без «· …»); затем строки `prev` (`✓ `, цвет `DIM`), `current` (`› `, `TEXT`, после названия — `<span data-agent-action>` серым `· {действие со строчной буквы}`, если `actions` не пуст), `next` (цвет `DIM`, `opacity: 0.6`, `paddingLeft: 12px`); при `allDone` — одна строка `✓ {t("agents.all_steps_done", {n: m})}`. Текущая строка — `display:flex; white-space:nowrap`: название `flex: 0 1 auto; min-width: 40%; overflow:hidden; text-overflow:ellipsis`, действие `flex: 0 1 auto; overflow:hidden; text-overflow:ellipsis`.
- Первая строка без плана — как сейчас (`✓ pastOf(older)`, `› newest`).
- Остальные строки: ровно одна строка высотой `TITLE_H`: точка, `<b>проект</b>`, серым `· {n} из {m} · {current}` / при `allDone` `· {все готовы}` / без плана — `· {последнее действие}` / ничего — `{имя агента}`.
- `items({rows})`: первая — `TITLE_H + k * LINE_H` (k — число строк лесенки: `allDone` → 1, иначе непустые из prev/current/next; без плана — `min(2, actions.length)`), остальные — `TITLE_H`.

- [ ] **Step 1: Write the failing tests** (`AgentCard.test.tsx`, язык тестов — EN, как у соседних):

```ts
const planRow = (s: string, over: Partial<AgentRow> = {}): AgentRow => ({ ...row, state: "working", plan: P(s), actions: ["Reading event.rs"], ...over });
it("ladder_shows_prev_current_next", () => {
  render(<AgentList rows={[planRow("ddapp")]} notch={notch} />);
  expect(title()).toBe("shop Claude Code · 3 of 5");
  const step = (k: string) => document.querySelector(`[data-agent-step="${k}"]`)?.textContent;
  expect(step("prev")).toBe("✓ s2");
  expect(step("current")).toBe("› s3 · reading event.rs");
  expect(step("next")).toBe("s4");
});
it("ladder_all_done_is_one_line", () => {
  render(<AgentList rows={[planRow("ddd")]} notch={notch} />);
  expect(title()).toBe("shop Claude Code");
  expect(document.querySelectorAll("[data-agent-step]").length).toBe(1);
  expect(document.querySelector('[data-agent-step="all-done"]')?.textContent).toBe("✓ All 3 steps done");
});
it("other_agents_get_one_line", () => {
  render(<AgentList rows={[planRow("ap"), planRow("dap", { key: { agent: "claude_code", id: "s2" }, project: "web" })]} notch={notch} />);
  const second = document.querySelectorAll("[data-agent-row]")[1] as HTMLElement;
  expect(second.textContent?.replace(/\s+/g, " ").trim()).toBe("web · 2 of 3 · s2");
  expect(second.querySelectorAll("[data-agent-step]").length).toBe(0);
});
it("ladder_height_matches_lines", () => {
  const n = { width: 179, height: 32 };
  const base = agentHeight({ rows: [planRow("ap")] }, n);           // current + next
  expect(agentHeight({ rows: [planRow("dap")] }, n)).toBe(base + 16); // + prev
  expect(agentHeight({ rows: [planRow("dap"), planRow("ap", { key: { agent: "claude_code", id: "s2" } })] }, n)).toBe(base + 16 + 18 + 6);
});
it("current_step_stays_one_line", () => {
  render(<AgentList rows={[planRow("ap", { plan: [{ title: "x".repeat(300), status: "active" }] })]} notch={notch} />);
  expect((document.querySelector('[data-agent-step="current"]') as HTMLElement).style.whiteSpace).toBe("nowrap");
});
```

(`P` — тот же помощник, что в Задаче 4; импортировать `agentHeight`.)

- [ ] **Step 2: Run** `pnpm test AgentCard` — Expected: FAIL.

- [ ] **Step 3: Implement** по правилам выше.

- [ ] **Step 4: Run** `pnpm test` — Expected: всё PASS.

- [ ] **Step 5: Commit** `feat(island): the ladder — previous, current with the live action, next step` + push.

### Task 6: Живая проверка, итоги, передача

**Files:**
- Modify: `docs/superpowers/specs/2026-11-13-agent-steps-live-design.md` (§8 «Итоги реализации»), `docs/superpowers/HANDOFF.md`, `CLAUDE.md` («Current focus»)

- [ ] **Step 1:** Полные проверки: `cargo test --lib`, `cargo clippy --all-targets` (0), `pnpm test`.
- [ ] **Step 2:** Сборка `pkill -x utter; scripts/dev-bundle.sh`; предупредить пользователя о Связке ключей, дождаться исчезновения `SecurityAgent`, `open src-tauri/target/release/bundle/macos/Utter.app`.
- [ ] **Step 3:** Проверить, что Utter переписал хук: `python3 -c` по `~/.claude/settings.json` → у `PostToolUse` `matcher` = `STEPS_MATCHER` (Review Focus 1).
- [ ] **Step 4:** Живая проверка: эта сессия ведёт план (`update_plan` с `advance`); фоново снимать окно острова (`screencapture -x -o -l <номер окна острова>` раз в секунду ~30 с, номера окон — через `CGWindowListCopyWindowInfo`); пользователь один раз наводит мышку на ушки. Ожидание: на снимках «Translate Claude Code · n из m», прошлый/текущий/следующий шаг с названиями из плана, действие меняется. Посмотреть снимки, показать лучший пользователю.
- [ ] **Step 5:** Записать итоги (что проверено и что нет) в §8 спецификации, HANDOFF («С чего начать»), `CLAUDE.md`; commit `docs: agent steps live — results, handoff` + push.
- [ ] **Step 6:** Финальное свежее ревью всей ветки (`requesting-code-review`), затем `finishing-a-development-branch` — слияние только с «да» пользователя.
