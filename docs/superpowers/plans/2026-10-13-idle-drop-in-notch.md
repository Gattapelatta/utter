# Капля в чёлке в простое — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** пока Utter не записывает, у выреза камеры видны «ушки» с живой Каплей; запись вырастает из них и сворачивается обратно в них.

**Architecture:** окно `overlay` (NSPanel) больше не убирается после записи, пока ушки показываются. Rust-«сторож» раз в 2 с собирает факты (экран под курсором, полноэкранное окно, блокировка, сон дисплея, настройка) и чистой функцией решает, показывать ли ушки; решение уходит странице событием `overlay://rest`. Страница рисует чёрную форму вырезанием (`clip-path`) в трёх видах (вырез / ушки-плашка / остров), одна Капля перелетает между ушком и островом, а её цикл кадров засыпает, когда пружины успокоились.

**Tech Stack:** Rust (Tauri 2, core-graphics 0.24, core-foundation 0.10, tokio), React 19 + Motion, Vitest/jsdom.

**Spec:** `docs/superpowers/specs/2026-10-06-utter-design.md` — §4.2a (главное), §4.2, §4.3, §5.4, §8.4, §9.2.

## Global Constraints

- Ушки: ширина `notch.width + 2 × 36`, высота `notch.height`, скругление только снизу, радиус `EAR_RADIUS = 8` (подбирается по снимку в Task 6).
- Капля в ушке 26 pt; центр по x — 18 pt левее левого края выреза, по y — `notch.height / 2`. Правое ушко пустое.
- Экран без выреза: плашка 36 × 32, Капля по её центру.
- Остров 360 × 104, скругление `0 0 40 40`; Капля 48 × 50, центр пары на 61 pt (не менять — утверждено 2026-10-08).
- Простой: взгляд раз в 2–5 с, моргание как сейчас, улыбка раз в 30–60 с на 1,6 с, ореол 0; только пружины `k = ω², c = 2ω` из `springs.ts`.
- Рост из ушек — пружина острова (`grow`); сворачивание в ушки — 0,6 с / Esc 0,35 с, `[0.4, 0, 0.2, 1]`.
- Сторож: период 2 с; ушек нет при: настройка выключена ∨ полноэкранное окно на экране курсора ∨ Mac заблокирован ∨ дисплей спит.
- Клики проходят насквозь всегда, кроме ошибки с действием (как сейчас).
- `prefers-reduced-motion`: Капля в ушке неподвижна, переходы — прозрачность 150 мс.
- Бюджет простоя (§8.4): WebContent острова ≤ 1 % ядра в среднем за 2 мин, utter ≤ 0,5 %.
- Один шаблон `clip-path` и один шаблон `transform` на элемент (иначе Motion схлопывает анимацию — см. CLAUDE.md).
- `cargo clippy --all-targets` — 0 предупреждений; коммит и `git push` после каждой задачи.

## Review Focus

1. Запись начинается, пока ушки сворачиваются в вырез (полноэкранное приложение закрылось/открылось за секунду до нажатия) — остров должен вырасти, окно не должно убраться отложенным `order_out` (тест в Task 3: `delayed_hide_skipped_when_rest_shown` + gate-поколения).
2. Настройку выключили во время записи — после записи остров сворачивается в вырез и окно убирается, а не застревает ушками (тест в Task 3: `hide_target_follows_current_presence`).
3. Курсор ушёл на другой экран во время записи — остров не прыгает посреди записи; переезд только в простое (тест в Task 3: `no_replace_while_busy`).
4. Капля «уснула», а пришла новая запись — она обязана проснуться в тот же кадр, иначе остров вырастет с неподвижной Каплей (тест в Task 4: `wakes_on_mood_change`).
5. Старый файл настроек без `notch_drop` — Капля включена; повреждённое значение (`"notch_drop": "yes"`) — файл уходит в `.bak`, как любое другое повреждение (тест в Task 1).

---

### Task 1: Настройка «Капля в чёлке»

**Files:**
- Modify: `src-tauri/src/settings.rs` (поле + тесты)
- Modify: `src/lib/types.ts` (поле в `Settings`)
- Modify: `src/settings/sections/General.tsx` (строка после «Автозапуска»)
- Modify: `src/i18n/ru.json`, `src/i18n/en.json`
- Modify: `src/settings/devMock.ts` (поле в мок-настройках, если там перечислены поля)
- Test: `src-tauri/src/settings.rs` (mod tests), `src/settings/sections/sections1.test.tsx`

**Interfaces:**
- Produces: `Settings.notch_drop: bool` (Rust, по умолчанию `true`), `notch_drop: boolean` (TS); i18n-ключи `general.notch_drop`, `general.notch_drop_hint`.

- [ ] **Step 1: Failing Rust tests** в `settings.rs`:
  - `defaults_match_spec` — добавить `assert!(s.notch_drop);`
  - `missing_notch_drop_means_on`: записать в файл JSON старых настроек без поля (`{"schema_version":1,"autostart":true}`), `load` → `notch_drop == true` и `autostart == true`.
  - `bad_notch_drop_is_corrupt`: `{"notch_drop":"yes"}` → `load` возвращает умолчания, рядом появляется `settings.json.bak`.
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib settings` → FAIL (нет поля).
- [ ] **Step 3:** поле `pub notch_drop: bool` в `Settings`, `notch_drop: true` в `defaults_for`.
- [ ] **Step 4:** тесты Rust → PASS.
- [ ] **Step 5: Failing UI test** в `sections1.test.tsx`: в «Основном» есть переключатель с подписью «Капля в чёлке» (RU), включён при `notch_drop: true`; щелчок вызывает сохранение настроек с `notch_drop: false` (так же, как уже тестируется соседний переключатель — повторить его схему).
- [ ] **Step 6:** `pnpm test sections1` → FAIL.
- [ ] **Step 7:** строка в `General.tsx` сразу после «Автозапуска»: `<Row label={t("general.notch_drop")} description={t("general.notch_drop_hint")}><Switch …checked={s.notch_drop} onChange={(v) => update({ notch_drop: v })} /></Row>`. Тексты:
  - ru: `"general.notch_drop": "Капля в чёлке"`, `"general.notch_drop_hint": "Капля живёт у выреза камеры, пока Utter ждёт"`
  - en: `"general.notch_drop": "Drop in the notch"`, `"general.notch_drop_hint": "The Drop lives by the camera notch while Utter waits"`
- [ ] **Step 8:** `pnpm test` (весь набор, включая проверку одинаковых ключей RU/EN в `i18n.test.ts`) → PASS; `cargo clippy … --all-targets` → 0.
- [ ] **Step 9:** commit `feat(settings): "Drop in the notch" switch (§4.2a)` + push.

---

### Task 2: Факты для решения «показывать ушки» (Rust, чистая логика + пробы macOS)

**Files:**
- Create: `src-tauri/src/app/overlay_rest.rs`
- Modify: `src-tauri/src/app/mod.rs` (`pub mod overlay_rest;`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }`
  - `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct RestFacts { pub enabled: bool, pub fullscreen: bool, pub locked: bool, pub asleep: bool, pub display: u32 }`
  - `pub fn rest_visible(f: &RestFacts) -> bool` — `enabled && !fullscreen && !locked && !asleep`.
  - `pub fn covers_screen(window: Rect, screen: Rect) -> bool` — совпадение с допуском 1.0 pt по каждой стороне.
  - `#[cfg(target_os = "macos")] pub fn probe(enabled: bool) -> RestFacts` — всё в координатах CG (начало — левый верх главного экрана):
    - курсор: `CGEvent::new(CGEventSource::new(HIDSystemState)).location()`; дисплей: `CGGetDisplaysWithPoint` (extern), рамка — `CGDisplay::new(id).bounds()`;
    - `fullscreen`: `core_graphics::window::copy_window_info(kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID)`; ищется окно с `kCGWindowLayer == 0`, `kCGWindowOwnerPID != std::process::id()`, чьи `kCGWindowBounds` (`CGRectMakeWithDictionaryRepresentation`) `covers_screen` рамку дисплея курсора;
    - `locked`: `CGSessionCopyCurrentDictionary()` (extern из CoreGraphics) → ключ `CGSSessionScreenIsLocked` = true (нет ключа → false);
    - `asleep`: `CGDisplayIsAsleep(display) != 0` (extern).
  - Разбор словаря окна вынести в `fn window_is_fullscreen(layer: i64, pid: i64, own_pid: i64, bounds: Rect, screen: Rect) -> bool` — тестируется без системы.

- [ ] **Step 1: Failing tests** (`mod tests` в `overlay_rest.rs`):
  - `rest_shown_only_when_nothing_blocks` — `rest_visible` для всех 16 сочетаний `enabled/fullscreen/locked/asleep`: true только при `(true,false,false,false)`.
  - `fullscreen_window_covers_whole_screen` — `covers_screen(Rect{0,0,1512,982}, Rect{0,0,1512,982})` → true; с отклонением 0.5 → true.
  - `maximized_window_is_not_fullscreen` — окно `Rect{0,37,1512,945}` (ниже меню) на экране `Rect{0,0,1512,982}` → false.
  - `fullscreen_on_other_display_does_not_count` — окно `Rect{1512,0,1920,1080}` против экрана `Rect{0,0,1512,982}` → false.
  - `window_is_fullscreen_rules` — слой ≠ 0 → false; свой pid → false; чужой pid, слой 0, покрывает экран → true.
- [ ] **Step 2:** `cargo test … --lib overlay_rest` → FAIL (нет модуля).
- [ ] **Step 3:** реализовать чистые функции и `probe` по Interfaces.
- [ ] **Step 4:** тесты → PASS; clippy → 0.
- [ ] **Step 5: Проба на настоящей системе** — временный `#[ignore]` тест `probe_runs` печатает `probe(true)` и время вызова; запуск `cargo test … --lib probe_runs -- --ignored --nocapture` (не трогает Связку ключей, микрофон и сеть). Ожидание: `fullscreen: false, locked: false, asleep: false`, вызов < 5 мс. Тест оставить `#[ignore]`.
- [ ] **Step 6:** commit `feat(overlay): facts that decide whether the Drop rests in the notch` + push.

---

### Task 3: Сторож и окно, которое не прячется

**Files:**
- Modify: `src-tauri/src/app/overlay_rest.rs` (состояние присутствия, сторож)
- Modify: `src-tauri/src/app/overlay.rs` (`TauriOverlay` учитывает присутствие; `place` публичен для сторожа)
- Modify: `src-tauri/src/lib.rs` (создать `Presence`, запустить сторож, `app.manage(RestWake)`)
- Modify: `src-tauri/src/app/commands.rs` (`update_settings` будит сторож)

**Interfaces:**
- Consumes: `RestFacts`, `rest_visible`, `probe` (Task 2); `Settings.notch_drop` (Task 1); `HideGate`, `place`, `order_front`, `order_out`, `HIDE_AFTER` (`overlay.rs`).
- Produces:
  - `#[derive(Default)] pub struct Presence { pub busy: bool, pub shown: bool, pub display: Option<u32> }`
  - `pub enum RestAction { None, Show { replace: bool }, Hide }` и `pub fn decide(p: &Presence, facts: &RestFacts) -> RestAction` — `Show{replace}` при `rest_visible` и (`!p.shown` или `!p.busy && p.display != Some(facts.display)`), где `replace = !p.busy && p.display != Some(facts.display)`; `Hide` когда `p.shown && !rest_visible`; иначе `None`. При `p.busy` — никогда `replace: true`.
  - `pub fn hide_after_state(p: &Presence) -> bool` — отложенный `order_out` после `Hidden/Cancelled` выполняется только если `!p.shown`.
  - `pub struct RestWake(pub Arc<tokio::sync::Notify>)` — в state приложения.
  - Событие `overlay://rest` с payload `{ "shown": bool }`.
  - `pub fn spawn_watcher(app: AppHandle, shared: Arc<Shared>, presence: Arc<Mutex<Presence>>, gate: Arc<Mutex<HideGate>>, wake: Arc<Notify>)` — цикл `tokio::select!{ interval(2 s).tick(), wake.notified() }`; `probe(settings.notch_drop)` вне главного потока; по `decide`: `Show` → на главном потоке `place` (если `!busy`) + `emit overlay://rest {shown:true}` + `order_front`; `Hide` → `emit {shown:false}`, `gate.on_state()`, через `HIDE_AFTER` `order_out`, если поколение то же и `!busy`. `TauriOverlay` получает поле `presence: Arc<Mutex<Presence>>`: в `show` ставит `busy = !matches!(s, Hidden | Cancelled)`; отложенный `order_out` дополнительно требует `hide_after_state`.
  - Никогда не держать замок `presence` при вызове `run_on_main_thread`/`emit` (правило про мьютексы в CLAUDE.md).

- [ ] **Step 1: Failing tests** (`overlay_rest.rs`):
  - `first_tick_shows_rest` — `Presence::default()` (`display: None`), факты видимы → `Show{replace:true}` (первый показ всегда ставит окно на экран курсора).
  - `rest_hides_on_fullscreen` — `shown:true`, `fullscreen:true` → `Hide`.
  - `no_replace_while_busy` — `shown:true, busy:true, display:Some(1)`, факты на дисплее 2 → `None`.
  - `moves_to_cursor_display_when_idle` — то же при `busy:false` → `Show{replace:true}`.
  - `delayed_hide_skipped_when_rest_shown` — `hide_after_state(&Presence{shown:true,..}) == false`, `{shown:false}` → true.
  - `hide_target_follows_current_presence` — после `Hide` (применили `shown=false`) завершение записи (`busy=false`) → `hide_after_state == true`.
- [ ] **Step 2:** `cargo test … --lib overlay_rest` → FAIL.
- [ ] **Step 3:** реализовать `Presence`, `RestAction`, `decide`, `hide_after_state`, `spawn_watcher`; подключить в `overlay.rs`, `lib.rs` (после `configure`), `update_settings` (`app.state::<RestWake>().0.notify_one()`).
- [ ] **Step 4:** тесты Rust → PASS, весь `cargo test --lib` → PASS, clippy → 0.
- [ ] **Step 5:** commit `feat(overlay): the window stays up for the Drop in the notch` + push.

---

### Task 4: Капля в простое — своя жизнь и сон между действиями

**Files:**
- Modify: `src/overlay/drop/dropModel.ts`
- Modify: `src/overlay/drop/Drop.tsx`
- Test: `src/overlay/drop/dropModel.test.ts`, `src/overlay/drop/drop.test.ts` (или новый `dropSleep.test.tsx`, jsdom)

**Interfaces:**
- Produces:
  - `ModelInput.rest?: boolean` — режим «ушка»: интервал взгляда `2000 + random × 3000` мс, улыбка: `nextSmile = now + 30000 + random × 30000`, держится 1600 мс (`P.smile.t = 1` → `0`), цель ореола `P.glow.t = 0`. При `reduced` в `rest` — ни взгляда, ни улыбки (только моргание формой глаз, как сейчас). Без `rest` поведение не меняется.
  - `Frame.settled: boolean` — все пружины `|x − t| < 1e-3` и `|v| < 1e-3`, настроение `idle` или `thinking`, нет активной эмоции.
  - `Frame.wakeAt: number` — ближайшее из `nextLook`, `nextBlink`, `blinkUntil` (если ≠ 0), `smileUntil`, `nextSmile` (в `rest`).
  - `Drop` prop `rest?: boolean` — передаётся в модель.
  - Цикл в `Drop.tsx`: если `f.settled` и нет указателя (`follow`) — вместо `requestAnimationFrame` ставится `setTimeout(wake, max(16, f.wakeAt − now))`; `wake` запускает rAF. Эффект на `[mood, translate, rest, look, frozen, reduced]` и `pointermove` будят спящий цикл немедленно. `level` не будит (меняется только в записи, а там цикл не спит).

- [ ] **Step 1: Failing tests** (`dropModel.test.ts`, детерминированный `random`):
  - `rest_looks_every_2_to_5_s` — в `rest` разница между сменами цели взгляда ∈ [2000, 5000].
  - `rest_smiles_every_30_to_60_s_for_1_6_s` — за 5 мин модельного времени улыбок 5–10, каждая `smile > 0.5` не дольше ~1,6 с + время пружины.
  - `rest_has_no_glow` — через 2 с в `rest` `glow < 0.01`.
  - `settles_and_reports_next_wake` — после успокоения `settled == true`, `wakeAt` равно ближайшему запланированному событию.
  - `listening_never_settles` — в `listening` `settled == false`.
  - `reduced_rest_never_smiles` — `rest` + `reduced`: за 5 мин `smile` всегда < 0.01, цель взгляда не меняется.
- [ ] **Step 2: Failing test** (jsdom, фейковые таймеры, `requestAnimationFrame` через `vi.stubGlobal`): `sleeps_when_settled` — после успокоения число вызовов rAF перестаёт расти до `wakeAt`; `wakes_on_mood_change` — смена `mood` на `listening` у спящей Капли даёт rAF сразу (до истечения таймера).
- [ ] **Step 3:** `pnpm test drop` → FAIL.
- [ ] **Step 4:** реализовать по Interfaces.
- [ ] **Step 5:** `pnpm test` → PASS (включая тесты окна настроек: `FloatingDrop`, логотип — их Капля тоже засыпает).
- [ ] **Step 6:** commit `perf(drop): the character sleeps between moves; calm life in the notch` + push.

---

### Task 5: Ушки, плашка и перелёт Капли на странице острова

**Files:**
- Modify: `src/overlay/drop/islandMotion.ts` (геометрия и варианты)
- Modify: `src/overlay/drop/Island.tsx` (фон-форма отдельно от содержимого, Капля в летающей обёртке)
- Modify: `src/overlay/main.tsx` (три вида, событие `overlay://rest`, DevStates)
- Modify: `src/overlay/overlay.css` (`.island` без фона; `.island-shape` — чёрный слой `inset: 0`)
- Modify: `src/lib/ipc.ts` (`events.overlayRest`)
- Test: `src/overlay/drop/islandMotion.test.ts`, `src/overlay/drop/islandExit.test.tsx`, `src/overlay/drop/Island.test.tsx`

**Interfaces:**
- Consumes: `overlay://rest {shown}` (Task 3); `Drop` prop `rest` (Task 4).
- Produces (`islandMotion.ts`):
  - `export type Box = { w: number; h: number; r: number }`; константы `EAR_W = 36`, `EAR_RADIUS = 8`, `REST_DROP = 26`, `PLATE = { w: 36, h: 32 }`.
  - `export function restBox(notch: Notch): Box` — с вырезом `{ w: notch.width + 2*EAR_W, h: notch.height, r: EAR_RADIUS }`, без — `{ w: 36, h: 32, r: EAR_RADIUS }`.
  - `export function foldedBox(notch: Notch): Box` — с вырезом `{ w: notch.width, h: notch.height, r: EAR_RADIUS }`, без — `{ w: 36, h: 0, r: 0 }`.
  - `export const ISLAND_BOX: Box = { w: 360, h: 104, r: 40 }`.
  - `export function clip(b: Box): string` — единственный шаблон: `inset(0px ${(360-b.w)/2}px ${104-b.h}px ${(360-b.w)/2}px round 0px 0px ${b.r}px ${b.r}px)`.
  - `export function restDropOffset(notch: Notch): { x: number; y: number; scale: number }` — сдвиг Капли от её места в острове (центр 134, 61 в координатах острова) к ушку: с вырезом центр `(180 − notch.width/2 − 18, notch.height/2)`, без — `(180, 16)`; `scale = 26/48`.
  - `export function shapeVariants(notch: Notch)` — `folded` / `rest` / `open` / `leave(cancelled)` по `clip(...)`; `open` — пружина `grow`; `rest` и `leave` — tween 0,6 / 0,35 с `[0.4, 0, 0.2, 1]`.
  - `export function dropVariants(notch: Notch)` — те же имена, `transform` одного шаблона `translate(x px, y px) scale(s)`; в `open` — `(0,0,1)`, в `rest` — `restDropOffset`, в `folded`/`leave` — как `rest` + `opacity: 0`.
  - Содержимое справа (`contentVariants`) — как сейчас, но только для `open`.
- `main.tsx`: `const view = visible ? "open" : rest ? "rest" : "folded"`; смонтировано, пока `visible || rest`; `AnimatePresence` с `exit="leave"` только когда уходят и остров, и ушки. `reduced` → `fadeOnly`. В `rest` Капля получает `mood="idle"` и `rest`. Таймер (`useClock`) тикает только при `visible` (как сейчас).
- `events.overlayRest(cb: (r: { shown: boolean }) => void)` в `ipc.ts`.
- DevStates: кнопка `rest` (переключает ушки).

- [ ] **Step 1: Failing tests** (`islandMotion.test.ts`):
  - `rest_box_with_notch` — для `{width:185,height:32}` → `{w:257,h:32,r:8}`; для `null` → `{w:36,h:32,r:8}`.
  - `rest_drop_sits_in_left_ear` — для `{width:185,height:32}` центр Капли `x = 180 − 92.5 − 18 = 69.5`, т.е. смещение `x = −64.5`, `y = 16 − 61 = −45`, `scale ≈ 0.5417`; Капля целиком в ушке: `69.5 ± 13` ∈ `[180 − 92.5 − 36, 180 − 92.5]`.
  - `one_clip_template` — у `clip` для всех трёх видов совпадает строка после замены чисел на `#` (защита от схлопывания Motion).
  - `one_drop_transform_template` — то же для `transform` в `dropVariants`.
- [ ] **Step 2: Failing tests** (`islandExit.test.tsx`, jsdom, переписать на `clip-path`):
  - `folds_into_ears_not_to_nothing` — из `open` в `rest`: элемент формы остаётся смонтирован, его `clip-path` за 0,6 с приходит к `clip(restBox)`; ширина по пути только уменьшается, ни разу не 0.
  - `folds_into_notch_when_rest_off` — `rest=false`: уход в `leave` к `clip(foldedBox)`, затем размонтирование.
  - `grows_from_ears` — из `rest` в `open`: первая выборка ширины ≥ ширины ушек.
- [ ] **Step 3: Failing test** (`Island.test.tsx`): в `rest` Капля рисуется (виден `svg`), справа пусто; в `recording` — как раньше 48 × 50 и таймер (существующие тесты не ломать).
- [ ] **Step 4:** `pnpm test overlay` → FAIL.
- [ ] **Step 5:** реализовать по Interfaces.
- [ ] **Step 6:** `pnpm test` → PASS; `pnpm build` (tsc) без ошибок.
- [ ] **Step 7:** commit `feat(overlay): ears in the notch; the Drop flies between ear and island` + push.

---

### Task 6: Живая проверка, подгонка и бюджет

**Files:**
- Modify (если подгонка нужна): `src/overlay/drop/islandMotion.ts` (`EAR_RADIUS`, позиции)
- Modify: `docs/superpowers/HANDOFF.md`, `CLAUDE.md` (если появилось неочевидное правило), спецификация §4.2a (финальный радиус)

- [ ] **Step 1:** сборка `pkill -x utter; scripts/dev-bundle.sh`, предупредить пользователя про окно Связки ключей, дождаться исчезновения `SecurityAgent`, `open …/Utter.app`.
- [ ] **Step 2: Снимок ушек:** `screencapture -x -o -l <номер окна overlay>` (номер через `CGWindowListCopyWindowInfo`) и снимок верхней полосы экрана `screencapture -x -R 0,0,<ширина>,60`; сверить: ушки 36 pt, Капля 26 pt, 5 pt до выреза, скругление совпадает со скруглением выреза (подогнать `EAR_RADIUS`). Показать снимок пользователю.
- [ ] **Step 3: Бюджет:** `top -l 120 -s 1 -stats pid,command,cpu` за 2 мин простоя; среднее по WebContent острова ≤ 1 %, utter ≤ 0,5 %. Не уложились — найти причину (`systematic-debugging`), не подгонять цифры.
- [ ] **Step 4: Плавность:** видео `screencapture -v -V 3 -R <прямоугольник острова>` с записью (попросить пользователя нажать клавишу) — переходы ушки → остров → ушки без пропущенных кадров (трекер кадров: шаг `+0` перед двойным шагом = пропуск). Если пропуски из-за `clip-path` — собрать форму из частей на `transform` (запасной вариант из спецификации).
- [ ] **Step 5: Полноэкранный режим:** открыть любое окно на весь экран (попросить пользователя, если нельзя самому) — по логу/снимку ушки прячутся за ≤ 2 с и возвращаются после выхода; запись на полном экране показывает остров.
- [ ] **Step 6:** честный отчёт пользователю: что проверено, что нет (внешний монитор без выреза — только если подключён).
- [ ] **Step 7:** обновить HANDOFF.md (статус фичи, что дальше: Task 12 → Task 13 → ревью → слияние), commit `docs: handoff after the Drop in the notch` + push.
