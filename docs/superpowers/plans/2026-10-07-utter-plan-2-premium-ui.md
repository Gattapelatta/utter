# Utter — План 2: премиальный интерфейс

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Заменить служебный интерфейс Плана 1 продуктом, который выглядит и ощущается дорого: анимированная пилюля в NSPanel, окно настроек с шестью разделами, онбординг с пробной записью, история и счётчики, светлая/тёмная тема, RU/EN, звуки, иконка трея с состояниями, автозапуск и автообновление — и закрыть UI-долги Плана 1 (M2, M6, M7, M8, M11, M15).

**Architecture:** Ядро по-прежнему владеет состоянием. Оркестратор получает две новые зависимости-трейта — `UiSink` (события для окна настроек и трея) и `Sounds`; всё, что меняет настройки (окно, трей), идёт через одну функцию `update_settings`, которая сохраняет, применяет и рассылает `ui://settings` всем окнам. Окно настроек создаётся лениво и уничтожается при закрытии (экономия памяти WebKit). React-часть: Tailwind 4 на CSS-токенах §5.1, Motion для пружин, Zustand-store, синхронизированный с ядром событиями, собственный словарь i18n.

**Tech Stack:** существующие Rust/Tauri 2/React 19/Vite/Vitest + новые: `tailwindcss` 4 и `@tailwindcss/vite`, `motion`, `zustand`, `@fontsource-variable/geist`, `@fontsource-variable/geist-mono`; Rust — `tauri-plugin-autostart` 2, `tauri-plugin-updater` 2, `tauri-nspanel` (git, ветка `v2.1`), фича `NSSound`/`NSAppearance` у `objc2-app-kit`.

**Spec:** `docs/superpowers/specs/2026-10-06-utter-design.md` (особенно §3.6–3.7, §4, §5). Состояние проекта и долги — `docs/superpowers/HANDOFF.md`.

## Global Constraints

- Ветка `feat/premium-ui`; после каждой задачи — коммит и `git push`; сообщения коммитов заканчиваются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. TDD: сначала тест, потом код.
- Перед каждой задачей с интерфейсом (4–12) загрузить скилы `emil-design-eng`, `apple-design`, `high-end-visual-design` и следовать им в мелочах, не противоречащих спецификации.
- Референс стиля — **x.ai** (§5: тёмный минимализм, иерархия цветом текста, сдержанные pop-in анимации; вдохновение, не клон — их шрифт и айдентику не используем). Перед задачами 4 и 6 открыть `https://x.ai` во встроенном браузере и сверить с ним плотность, отступы, вес шрифтов и характер анимаций.
- Цвет в компонентах — только через токены (`var(--accent-*)` / классы Tailwind на них), никаких hex в `.tsx`. Пользователь ещё не решил, оставлять ли цветные надписи и значки: все места, где акцент красит текст или значок, помечены атрибутом `data-accent`, чтобы перевести интерфейс в монохром одной правкой `tokens.css` (`[data-accent] { color: var(--text) }` и т. п.).
- Тема по умолчанию — **тёмная**; варианты Тёмная / Светлая / Как в системе. Пилюля **всегда тёмная**.
- Токены — ровно значения таблицы §5.1 спецификации. Правило контраста (замерено при написании плана): `text` и `text-secondary` — на `bg` и `surface` (≥ 4.5:1 в обеих темах); на `surface-raised` — только `text` (у `text-secondary` там 4.07:1 в тёмной теме); в светлой теме акцентные цвета — только заливки/иконки рядом с текстом, **не цвет текста** (record 3.03, success 4.12, danger 4.34, warning 2.82 на `bg`).
- Шрифты **Geist** и **Geist Mono** встроены в сборку (пакеты `@fontsource-variable/*`, лицензия OFL), без загрузки из сети. Цифры таймера и счётчиков — `font-variant-numeric: tabular-nums`. Основной текст 14 px / 1.5; заголовки weight 500, letter-spacing −1%…0, line-height 1.1–1.2. Карточки: `surface`, рамка `border` 1 px, радиус 12 px.
- Анимируем только `transform`, `opacity`, `filter`; никаких `transition: all`; прерываемое — на пружинах Motion или CSS-transitions. Единственное исключение — запасной путь кроссфейда темы (Task 1). `prefers-reduced-motion`: перемещения/масштабы → opacity 150 мс, shimmer → статичная линия с пульсом прозрачности.
- Каждая видимая пользователю строка — из словарей `src/i18n/ru.json` / `en.json` (в Rust — только меню трея, `app/tray.rs`). Язык «Авто» = язык системы: начинается с `ru` → RU, иначе EN.
- Все изменения настроек (окно, трей, онбординг) — только через `app::commands::update_settings`; она же рассылает `ui://settings`.
- Адреса, модели **и бесплатные лимиты** провайдеров — только в `src-tauri/src/providers/defaults.rs`. Внешние ссылки открываются только командой `open_link` по фиксированному списку идентификаторов (никаких произвольных URL из интерфейса).
- Никаких `unwrap()`/`expect()` в рабочих путях Rust; `cargo clippy --all-targets` — 0 предупреждений; `pnpm test` и `cargo test --lib` зелёные после каждой задачи.
- Бюджеты §8.4: нажатие → первый кадр пилюли < 50 мс; нажатие → первый сэмпл < 100 мс; в простое ~0% CPU и < 80 МБ суммарно (с учётом процессов WebKit).
- Сборка для проверки пользователем — только `pkill -x utter; scripts/dev-bundle.sh`, запуск `open src-tauri/target/release/bundle/macos/Utter.app`.

## Review Focus

1. **Трей и открытое окно настроек меняют настройки одновременно** (переключил «Нажатие» в трее, потом поменял тему в окне) → изменение из трея не должно затереться старой копией настроек окна. → тест `store applies patches on top of the latest broadcast settings` (Task 3).
2. **Пилюля начала прятаться (анимация ухода), и тут же началась новая запись** → отложенное скрытие окна не должно спрятать новую пилюлю. → тест `new_state_cancels_pending_hide` (Task 5).
3. **Окно настроек закрыли во время назначения клавиши или пробной записи онбординга** → захват клавиши и «пробный режим» не должны остаться включёнными (иначе следующая горячая клавиша «съедается» или диктовки перестают вставляться). → тесты `cancel_capture_restores_routing` (Task 2), `closing_settings_turns_trial_off` (Task 6).
4. **Длинные строки** (английская ошибка с именем провайдера, «ещё 3 в обработке» + таймер, запись истории на 10 минут речи) → пилюля не шире окна и обрезает подпись многоточием; запись истории свёрнута до 3 строк. → тесты `label is truncated, not wrapped` (Task 4), `long entries are clamped until expanded` (Task 10).
5. **Онбординг прерван** (закрыл окно на шаге 3, перезапустил приложение) → продолжение с первого невыполненного обязательного шага, а не с начала и не «готово». → тест `firstIncompleteStep` (Task 11).

---

## Файловая структура

```
src/
  i18n/ru.json, en.json, index.ts        словари, t(), useT(), resolveLang
  lib/types.ts, ipc.ts                   + новые типы/команды/события (обновить)
  lib/store.ts                           Zustand: settings, lang, theme, синхронизация
  lib/theme.ts                           resolveTheme, applyTheme (кроссфейд)
  lib/contrast.ts                        WCAG-контраст для теста токенов
  styles/tokens.css                      токены тёмной и светлой тем (обновить)
  styles/app.css                         Tailwind 4 + @theme на токенах + шрифты
  overlay/Pill.tsx, Wave.tsx, Check.tsx, pillMotion.ts, main.tsx   новая пилюля
  settings/main.tsx, App.tsx, Sidebar.tsx, routes.ts
  settings/ui/{Button,Switch,Segmented,Select,Card,Row,Keycap,Bar,TextField}.tsx
  settings/sections/{General,Hotkeys,Translate,Provider,History,About,Permissions}.tsx
  settings/onboarding/{Onboarding.tsx,steps.ts,DemoPhrase.tsx,steps/*.tsx}
  удалить: settings/DevSettings.tsx, settings/dev.css, lib/messages.ts, overlay/pill.css
src-tauri/src/
  sounds.rs                              синтез звуков старта/стопа (чистая функция)
  types.rs                               + KeyId::is_assignable
  hotkey/router.rs, hotkey/mod.rs        + cancel_capture
  usage.rs, pipeline/mod.rs              + учёт одного запроса Gemini (M7)
  providers/defaults.rs                  + FreeLimits
  providers/gemini.rs                    + коды языков без таблицы
  app/orchestrator.rs                    UiSink, Sounds, AppStatus, Cancelled, trial, M6
  app/services.rs                        TauriUi, TauriSounds
  app/commands.rs                        update_settings + новые команды
  app/settings_window.rs                 ленивое окно настроек, маршруты
  app/overlay.rs                         NSPanel (M8), отложенное скрытие, замер кадра
  app/tray.rs                            иконка с состояниями, меню RU/EN
  app/updates.rs                         проверка/установка обновлений
  lib.rs, tauri.conf.json, capabilities/default.json, Cargo.toml
```

---

### Task 1: Фундамент интерфейса — Tailwind, шрифты, токены двух тем, CSP

**Files:**
- Modify: `package.json`, `vite.config.ts`, `src/styles/tokens.css`, `src-tauri/tauri.conf.json`
- Create: `src/styles/app.css`, `src/lib/theme.ts`, `src/lib/contrast.ts`, `src/lib/theme.test.ts`, `src/styles/tokens.test.ts`

**Interfaces:**
- Produces:
  - `tokens.css`: тёмные значения на `:root` и `[data-theme="dark"]`, светлые на `[data-theme="light"]`; имена переменных как сейчас (`--bg`, `--surface`, `--surface-raised`, `--border`, `--border-strong`, `--text`, `--text-secondary`, `--accent-record`, `--accent-translate`, `--accent-success`, `--accent-danger`, `--accent-warning`), `--font: "Geist Variable", …`, `--mono: "Geist Mono Variable", …`.
  - `app.css`: `@import "tailwindcss"`, `@theme inline` с цветами `bg, surface, surface-raised, border, border-strong, text, text-secondary, record, translate, success, danger, warning` и шрифтами `sans`, `mono` на этих переменных; импорт шрифтов.
  - `src/lib/theme.ts`: `export type Resolved = "dark" | "light"; export function resolveTheme(pref: Theme, systemDark: boolean): Resolved; export function applyTheme(t: Resolved, animate: boolean): void` — ставит `data-theme` на `<html>`; при `animate` — через `document.startViewTransition`, если он есть, иначе класс `theme-fade` на 300 мс (`transition: background-color, color, border-color 250ms` — единственное исключение из правила анимаций); `export function watchSystemDark(cb: (dark: boolean) => void): () => void`.
  - `src/lib/contrast.ts`: `export function contrast(fg: string, bg: string): number` (формула WCAG 2.x, hex `#RRGGBB`).

- [ ] **Step 1: Установить пакеты:** `pnpm add motion zustand @fontsource-variable/geist @fontsource-variable/geist-mono` и `pnpm add -D tailwindcss @tailwindcss/vite`; подключить `tailwindcss()` в `vite.config.ts` рядом с `react()`.
- [ ] **Step 2: Тесты (Vitest):**

```ts
// src/styles/tokens.test.ts — читает tokens.css как текст, разбирает обе темы
it("text and text-secondary reach 4.5:1 on bg and surface in both themes")
it("text reaches 4.5:1 on surface-raised in both themes")
it("pill accents reach 4.5:1 on #0A0A0A")          // record, translate, success, danger, warning (тёмные)
it("light tokens match spec §5.1")                 // bg #FAFAF9, surface #FFFFFF, … accent-warning #C98A12
// src/lib/theme.test.ts
it("System follows the OS, explicit choices win")  // resolveTheme("System", true) === "dark"; ("Light", true) === "light"
it("applyTheme sets data-theme without animation") // document.documentElement.dataset.theme === "light"
it("watchSystemDark reports changes")              // мок matchMedia, событие change(false) → cb(false)
```

- [ ] **Step 3:** `pnpm test` — FAIL (нет модулей/светлой темы).
- [ ] **Step 4: Реализовать** токены, `app.css`, `theme.ts`, `contrast.ts`.
- [ ] **Step 5: CSP (долг M15)** в `tauri.conf.json`: `"csp": "default-src 'self' ipc: http://ipc.localhost; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:"`, `"devCsp": null`. Проверить `pnpm tauri dev`: окна открываются, в консоли WebView нет нарушений CSP.
- [ ] **Step 6:** `pnpm test` PASS, `pnpm build` без ошибок.
- [ ] **Step 7: Commit + push** — `feat(ui): design tokens for both themes, Geist fonts, Tailwind, CSP`.

---

### Task 2: Ядро для нового интерфейса — события UI, звуки, долги M2/M6/M7/M11

**Files:**
- Create: `src-tauri/src/sounds.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod sounds`), `types.rs`, `hotkey/router.rs`, `hotkey/mod.rs`, `usage.rs`, `pipeline/mod.rs`, `providers/defaults.rs`, `providers/gemini.rs`, `app/orchestrator.rs`, `app/services.rs`, `app/commands.rs`, `Cargo.toml` (фичи `objc2-app-kit`: `NSSound`, `NSAppearance`, `NSApplication`; `objc2-foundation`: `NSData`)

**Interfaces:**
- Produces (Rust):
  - `orchestrator.rs`:
    - `OverlayState` + вариант `Cancelled` (serde `"cancelled"`: пилюля быстро гаснет); `Done.outcome` может быть `"shown"` (пробная запись); `Error` + поле `provider: Option<String>` (`"Gemini" | "Groq" | "OpenAI"` — для «Лимит Gemini исчерпан…», «Сервис Gemini недоступен»).
    - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum AppStatus { Idle, Recording, Processing }`
    - `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum TrialOutcome { Text { mode: Mode, text: String }, Error { message_key: String } }`
    - `#[derive(Clone, Debug, PartialEq)] pub enum UiEvent { Status(AppStatus), Hotkey { mode: Mode, down: bool }, Trial(TrialOutcome), HistoryChanged }`
    - `pub trait UiSink: Send + Sync { fn emit(&self, e: UiEvent); }`, `pub trait Sounds: Send + Sync { fn play(&self, cue: crate::sounds::Cue); }`; `Deps` + поля `ui: Arc<dyn UiSink>`, `sounds: Arc<dyn Sounds>`.
    - `Shared` + `pub trial: AtomicBool` (пробный режим онбординга: результат не вставляется, а уходит в окно).
  - `sounds.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Cue { Start, Stop }`; `pub fn cue_wav(cue: Cue) -> Vec<u8>` — WAV 44.1 кГц mono 16-bit: две синусоиды по 60 мс (Start 660 → 880 Гц, Stop 880 → 660 Гц), пик 0.1 (−20 dBFS), огибающие 5 мс на каждой ноте.
  - `types.rs`: `impl KeyId { pub fn is_assignable(&self) -> bool }` — `true` для `RightAlt, RightMeta, RightCtrl, RightShift, CapsLock, Fn, F(_)`, `false` для `Escape` и `Raw(_)` (буквы, цифры, левые модификаторы — долг M11).
  - `hotkey`: `Router::cancel_capture(&mut self)`, `HotkeyHandle::cancel_capture(&self)`, `HotkeyService::cancel_capture(&self)`.
  - `usage.rs`: `pub fn record_tokens(&mut self, today: NaiveDate, tokens: u32)` (токены без счёта запроса); `pipeline::JobOutput` + `pub direct: bool`.
  - `defaults.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Serialize)] pub struct FreeLimits { pub stt_requests: Option<u32>, pub audio_secs: Option<u32>, pub llm_requests: Option<u32>, pub llm_tokens: Option<u32> }`; `pub fn free_limits(kind: ProviderKind) -> Option<FreeLimits>` (`OpenAi` → `None`).
  - `gemini_language_code(code: &str) -> Option<String>`: таблица как сейчас; любой другой двухбуквенный код ISO 639-1 передаётся как есть (BCP-47).
  - Команды: `cancel_capture()`, `set_trial(on: bool)`, `free_limits(provider: ProviderKind) -> Option<FreeLimits>`; `capture_key` при таймауте 15 с вызывает `cancel_capture` и возвращает `"timeout"`, при неподходящей клавише — `"not_assignable"`.
  - `services.rs`: `pub struct TauriUi { pub app: AppHandle }` (пока: `Status` → лог; остальные события — `emit` в окно `settings` под именами `ui://status`, `ui://hotkey`, `ui://trial`, `ui://history`; трей подключит Task 9); `pub struct TauriSounds { pub app: AppHandle }` — на главном потоке (`run_on_main_thread`) создаёт `NSSound initWithData:` из `cue_wav`, громкость 0.5, `play`; оба `NSSound` кэшируются в `thread_local!` главного потока.

Поведение оркестратора:
- `Input::Hotkey(Down(m) | Up(m))` → `UiEvent::Hotkey { mode: m, down }`; `Tap(m)` → два события (down, затем up).
- После каждого `handle()` статус (`recording.is_some()` → `Recording`, иначе `in_flight > 0` → `Processing`, иначе `Idle`) отправляется, только если изменился.
- `Start` → `Cue::Start` (если `settings.sounds`) — **после** `audio.start()`, чтобы звук не задерживал запись; `Submit` → `Cue::Stop`; отмена — без звука.
- `Command::Cancel` без ожидающих результатов → `OverlayState::Cancelled` вместо `Hidden`.
- `Input::HideOverlay`, когда `in_flight > 0`, → `processing_state()` вместо `Hidden` (долг M6).
- `finish()` при `shared.trial == true`: успех → не вызывать `output.deliver`, отправить `Trial(Text)`, пилюля `Done { outcome: "shown" }`; ошибка → `Trial(Error)` + обычная пилюля ошибки. После обновления истории — `UiEvent::HistoryChanged`.
- Учёт (M7): при `out.direct` — `record_stt(audio_secs)` + `record_tokens(tokens)`, без `record_llm`.

- [ ] **Step 1: Сверить бесплатные лимиты** на страницах `https://ai.google.dev/gemini-api/docs/rate-limits` (модель `GEMINI_TEXT_MODEL`, суточные запросы) и `https://console.groq.com/docs/rate-limits` (`whisper-large-v3-turbo`: запросы/сутки и секунды аудио/сутки; `openai/gpt-oss-120b`: запросы и токены/сутки). Записать числа в `free_limits`, источник и дату — комментарием над функцией. Если страница недоступна — спросить пользователя, не подставлять числа по памяти.
- [ ] **Step 2: Тесты:**

```rust
// sounds.rs
#[test] fn cue_is_short_quiet_wav()            // hound читает; 44100 Гц, 1 канал; длительность 115–125 мс; |пик| ≤ 0.1·i16::MAX
#[test] fn cue_starts_and_ends_silent()        // первый и последний сэмплы == 0
#[test] fn start_and_stop_differ()             // cue_wav(Start) != cue_wav(Stop)
// types.rs
#[test] fn letters_and_escape_are_not_assignable() // Raw(0) (A), Escape, Raw(55) (левый ⌘) → false; RightAlt, CapsLock, Fn, F(13) → true
// hotkey/router.rs
#[test] fn cancel_capture_restores_routing()   // begin_capture; cancel_capture; Down(RightAlt) → Some(HotkeyEvent::Down(Dictation))
// usage.rs
#[test] fn record_tokens_does_not_count_a_request()
// defaults.rs
#[test] fn free_limits_known_for_free_tiers()  // Gemini: stt_requests.is_some(); Groq: все четыре is_some(); OpenAi: None
// gemini.rs
#[test] fn unmapped_language_passes_bare_code() // "nl" → Some("nl"), "ru" → Some("ru-RU"), "" → None
// orchestrator.rs (фейки FakeUi(Mutex<Vec<UiEvent>>), FakeSounds(Mutex<Vec<Cue>>))
#[tokio::test] async fn status_events_follow_recording_and_processing() // Idle→Recording→Processing→Idle, без повторов подряд
#[tokio::test] async fn hotkey_presses_are_forwarded_to_ui()            // Down/Up → Hotkey{down:true}, Hotkey{down:false}
#[tokio::test] async fn sounds_play_on_start_and_submit_when_enabled()  // [Start, Stop]
#[tokio::test] async fn no_sounds_when_disabled_or_cancelled()          // sounds=false → []; Esc → только [Start]
#[tokio::test] async fn escape_hides_with_cancelled_state()             // последний OverlayState == Cancelled
#[tokio::test] async fn error_hide_returns_to_processing_while_jobs_remain() // M6
#[tokio::test] async fn trial_mode_shows_result_in_window_without_pasting()  // FakeOutput пуст; Trial(Text{..}); Done{outcome:"shown"}; HistoryChanged
#[tokio::test] async fn direct_request_counts_once()                    // stt_requests 1, llm_requests 0, llm_tokens > 0
#[tokio::test] async fn rate_limit_error_names_provider()               // Error{provider: Some("Gemini"), ..}
```

- [ ] **Step 3:** `cargo test --lib` — FAIL.
- [ ] **Step 4: Реализовать.** Обновить `Rig` в тестах оркестратора и сборку `Deps` в `lib.rs`. TS-зеркала новых типов (`AppStatus`, `TrialOutcome`, `FreeLimits`, `Cancelled`, `provider`) — в `src/lib/types.ts`; команды и события — в `src/lib/ipc.ts`.
- [ ] **Step 5:** `cargo test --lib`, `cargo clippy --all-targets` (0), `pnpm test` — PASS.
- [ ] **Step 6: Commit + push** — `feat(core): UI events, sounds, trial mode; fix capture timeout, letter hotkeys, hide-while-processing, Gemini usage`.

---

### Task 3: i18n и общий store

**Files:**
- Create: `src/i18n/ru.json`, `src/i18n/en.json`, `src/i18n/index.ts`, `src/i18n/i18n.test.ts`, `src/lib/store.ts`, `src/lib/store.test.ts`
- Modify: `src/lib/ipc.ts` (событие `ui://settings`)

**Interfaces:**
- Consumes: `Settings`, `UiLanguage` (types.ts); `ipc.getSettings`, `ipc.saveSettings`.
- Produces:
  - `export type Lang = "ru" | "en"; export function resolveLang(pref: UiLanguage, systemLang: string): Lang; export function translate(lang: Lang, key: string, vars?: Record<string, string | number>): string; export function useT(): (key: string, vars?: Record<string, string | number>) => string` — подстановка `{name}`; множественное число: если передан `vars.n` и есть ключи `key_one/_few/_many/_other`, выбор по `Intl.PluralRules(lang)`; отсутствующий ключ → сам ключ.
  - Ключи словаря, разбитые по префиксам: `pill.*`, `errors.*`, `nav.*`, `general.*`, `hotkeys.*`, `translate.*`, `provider.*`, `history.*`, `about.*`, `onboarding.*`, `common.*`. Тексты ошибок — по таблице §8.3 спецификации (`errors.rate_limited` = «Лимит {provider} исчерпан, сбросится через {n} мин», `errors.service_down` = «Сервис {provider} недоступен» и т. д.).
  - `src/lib/store.ts`: `export const useApp = create<AppStore>()` с полями `settings: Settings | null`, `lang: Lang`, `theme: Resolved`, `systemDark: boolean` и методами `init(): Promise<() => void>` (загрузить настройки, подписаться на `ui://settings` и `watchSystemDark`, применить тему), `update(patch: Partial<Settings>): Promise<void>` (накладывает `patch` на **текущие** `settings` из store и сохраняет), `applyRemote(s: Settings): void`.

- [ ] **Step 1: Тесты:**

```ts
it("ru and en have exactly the same keys")
it("interpolates variables")                 // translate("ru","errors.rate_limited",{provider:"Gemini",n:3}) содержит "Gemini" и "3"
it("picks Russian plural forms")             // pill.pending: n=1 «ещё 1 в обработке», n=2 «ещё 2 в обработке», n=5 «ещё 5 в обработке»
it("Auto follows the system language")       // resolveLang("Auto","ru-RU")==="ru"; ("Auto","de-DE")==="en"; ("En","ru")==="en"
it("store applies patches on top of the latest broadcast settings") // init; applyRemote({...s, record_mode:"Toggle"}); update({theme:"Light"}) → saveSettings вызван с record_mode "Toggle" и theme "Light"
it("store resolves theme and language from settings")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать (словари — полностью, для всех экранов Задач 4–11; тексты пилюли — из `src/lib/messages.ts`, который удаляется в Task 4). **Step 4:** PASS.
- [ ] **Step 5: Commit + push** — `feat(ui): RU/EN dictionaries and settings store`.

---

### Task 4: Пилюля — дизайн и анимации

**Files:**
- Create: `src/overlay/pillMotion.ts`, `src/overlay/Wave.tsx`, `src/overlay/Check.tsx`, `src/overlay/pillMotion.test.ts`
- Modify: `src/overlay/Pill.tsx`, `src/overlay/main.tsx`, `src/overlay/Pill.test.tsx`
- Delete: `src/overlay/pill.css`, `src/lib/messages.ts`

**Interfaces:**
- Consumes: `OverlayState` (с `cancelled`, `provider`, outcome `shown`), `useT`, `useApp` (только язык).
- Produces:
  - `pillMotion.ts`: `export function pillVariants(reduced: boolean): Variants` с ключами `hidden`, `visible`, `exit`, `cancel`; `export const widthSpring`, `barSpring`, `shake`.
  - `Pill({ state, levels, now })` — сигнатура как сейчас; корневой элемент `motion.div` с `layout`.
  - `main.tsx`: `useApp.init()` (нужен язык), `AnimatePresence` вокруг `Pill`. Корень пилюли несёт `data-theme="dark"`, поэтому токены всегда тёмные, какая бы тема ни была выбрана.

Значения (§4.2, §5.3, §5.4 спецификации):
- Пилюля: фон `rgba(10,10,10,0.85)` + `backdrop-filter: blur(20px)`, рамка `border-strong`, тень `0 8px 32px rgba(0,0,0,0.45)`, высота 44 px, радиус 22 px, всегда тёмные токены; `max-width: 448px`, подпись — одна строка с многоточием. `@media (prefers-reduced-transparency: reduce)` → сплошной `#0A0A0A`, без blur.
- Появление: y 8→0, scale 0.96→1, opacity 0→1, blur 4px→0; 220 мс, `cubic-bezier(0.2, 0.9, 0.3, 1.15)`, `transform-origin: bottom center`.
- Смена ширины: spring bounce 0, duration 0.35. Уход: y 0→8, opacity→0, 160 мс, `cubic-bezier(0.23, 1, 0.32, 1)`. `cancelled`: opacity→0, 120 мс.
- Запись: 24 полоски, высота — spring к уровню (bounce 0.15, duration 0.18); таймер `m:ss` Geist Mono tabular; за 30 с до лимита (`warn`) — `accent-warning`; метка режима `pill.dictation` / `pill.translate_business` / `pill.translate_casual` с акцентом `accent-record` / `accent-translate`; справа `pill.pending` при `pending > 0`.
- Обработка: волна сворачивается в тонкую линию, по ней линейный shimmer 1.2 с, бесконечно; через 4 с — `pill.slow`.
- Готово: галочка `accent-success`, stroke-dashoffset 200 мс ease-out; подписи `pill.pasted` / `pill.typed` / `pill.copied` / `pill.failed` / `pill.shown` / `pill.degraded`.
- Ошибка: красноватый оттенок (`accent-danger` с прозрачностью 0.12 поверх фона), одно покачивание по X ±4 px (spring bounce 0.4), текст — `errors.<message_key>` с `{provider}` и `{n}`; при `action` — курсор-указатель.
- Reduced motion (`useReducedMotion()` из Motion): все варианты — только opacity 150 мс; shimmer — статичная линия с пульсом прозрачности.

- [ ] **Step 1: Тесты:**

```ts
// pillMotion.test.ts
it("reduced motion uses opacity only")      // pillVariants(true).visible/hidden/exit без y, scale, filter; duration 0.15
it("appear matches the spec")               // pillVariants(false).hidden: {y:8, scale:0.96, opacity:0, filter:"blur(4px)"}; visible.transition.duration 0.22, ease [0.2,0.9,0.3,1.15]
it("exit and cancel timings")               // exit 0.16 [0.23,1,0.32,1]; cancel opacity 0, 0.12
// Pill.test.tsx (обновить существующие ожидания под словари; язык ru)
it("shows translated labels in English")    // lang en → "→ EN · Casual"
it("names the provider in rate-limit errors")
it("label is truncated, not wrapped")       // у подписи стиль white-space nowrap + text-overflow ellipsis
it("renders the success check for shown trial results")
it("renders nothing for hidden and cancelled")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Визуальная проверка** в `pnpm dev` (страница `overlay.html`): временная панель с кнопками состояний под `import.meta.env.DEV`; проверить каждое состояние и смену ширины без рывков, в обеих настройках reduced motion (Системные настройки › Универсальный доступ › Дисплей › Уменьшить движение). Панель не попадает в production-сборку.
- [ ] **Step 6: Commit + push** — `feat(ui): animated recording pill`.

---

### Task 5: Окно пилюли — NSPanel (M8), отложенное скрытие, замеры бюджета

**Files:**
- Modify: `src-tauri/Cargo.toml` (`tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }` для macOS), `src-tauri/src/app/overlay.rs`, `src-tauri/src/lib.rs` (плагин nspanel), `src-tauri/tauri.conf.json` (размер окна), `src-tauri/src/recorder.rs`, `src-tauri/src/app/commands.rs`, `src/lib/ipc.ts` (`overlayPainted`), `src/overlay/main.tsx` (после первого кадра состояния `recording` — `requestAnimationFrame` → `ipc.overlayPainted()`)

**Interfaces:**
- Consumes: `OverlayState::Cancelled` (Task 2).
- Produces:
  - `pub struct HideGate { gen: u64 }` в `overlay.rs`: `pub fn on_state(&mut self) -> u64` (каждое новое состояние увеличивает поколение и возвращает его), `pub fn still_current(&self, gen: u64) -> bool`.
  - Команда `overlay_painted()` — пишет в лог `timing: key→pill painted` (мс от последнего показа состояния `Recording`).
  - Лог `timing: start→first sample` из `recorder.rs` (мс от `start()` до первого колбэка с данными).

Решения:
- Окно `overlay` 480×100; пилюля прижата к низу окна с отступом 24 px (место под тень и анимацию), `BOTTOM_MARGIN` окна = 24 pt — низ пилюли остаётся в 48 pt от низа рабочей области (§4.2).
- NSPanel через `tauri-nspanel` (Rev фиксируется в `Cargo.lock`): `can_become_key_window = false`, `is_floating_panel = true`, style mask `nonactivating_panel`, уровень и `collectionBehavior` как сейчас (`CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY`, уровень 101), `ignoresMouseEvents` кроме `Error` с `action`. API — по README крейта на зафиксированной ревизии. **Если** крейт не собирается с нашей версией Tauri — оставить текущее NSWindow-решение, записать причину в HANDOFF (долг M8 остаётся открытым) и не изобретать свою подмену класса.
- `Hidden`: событие уходит в окно сразу, `orderOut` — через 200 мс, если `HideGate` не сменил поколение; `Cancelled` — через 140 мс. Любое не-скрытое состояние показывает окно сразу.

- [ ] **Step 1: Тест:**

```rust
#[test] fn new_state_cancels_pending_hide()   // g1 = gate.on_state() (Hidden); g2 = gate.on_state() (Recording); !gate.still_current(g1) && gate.still_current(g2)
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать `HideGate`, панель, отложенное скрытие, замеры. **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Ручная проверка** (собрать `scripts/dev-bundle.sh`): пилюля видна поверх полноэкранного Safari и на другом рабочем столе; фокус остаётся в TextEdit (курсор мигает, после вставки печать продолжается); клик по пилюле ошибки «Добавь API-ключ» открывает настройки; анимация ухода видна целиком; в логе есть `key→pill painted` и `start→first sample` — записать значения.
- [ ] **Step 6: Commit + push** — `feat(overlay): non-activating panel, exit animation, latency logs`.

---

### Task 6: Окно настроек — каркас, ленивое создание, компоненты

**Files:**
- Create: `src-tauri/src/app/settings_window.rs`, `src/settings/App.tsx`, `src/settings/Sidebar.tsx`, `src/settings/routes.ts`, `src/settings/ui/{Button,Switch,Segmented,Select,Card,Row,Keycap,Bar,TextField}.tsx`, `src/settings/App.test.tsx`, `src/settings/ui/ui.test.tsx`
- Modify: `src-tauri/src/app/{mod.rs,commands.rs}`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json` (убрать окно `settings` из `app.windows`), `src-tauri/capabilities/default.json` (`core:window:allow-start-dragging`), `src/settings/main.tsx`, `src/lib/ipc.ts` (обёртки новых команд и `ui://navigate`)
- Delete: `src/settings/DevSettings.tsx`, `src/settings/dev.css`

**Interfaces:**
- Consumes: `useApp`, `useT`, `applyTheme`.
- Produces (Rust):
  - `settings_window::open(app: &AppHandle, route: Option<&str>)` — если окна нет: `WebviewWindowBuilder` (`label "settings"`, `settings.html#/<route>`, 900×620, минимум 760×520, `title_bar_style(TitleBarStyle::Overlay)`, `hidden_title(true)`, `visible(false)`, `background_color` = `bg` темы: Dark `#0A0A0A`, Light `#FAFAF9`, System — по `theme()` окна `overlay`); показать по `settings_ready` или через 1,5 с, что раньше. Если окно есть: `emit_to("settings", "ui://navigate", route)`, `show`, `set_focus`.
  - `settings_window::on_destroyed(shared: &Shared, hotkeys: &HotkeyService)` — `trial = false`, `cancel_capture()`; вызывается из `WindowEvent::Destroyed`.
  - `pub fn update_settings(app: &AppHandle, state: &AppState, next: Settings) -> Result<(), String>` — то, что делал `save_settings` (сохранить, очистить историю при выключении, `Input::SettingsChanged`), плюс `app.emit("ui://settings", &next)`; `save_settings` вызывает её. Автозапуск и трей подключаются к ней в Задачах 7 и 9.
  - Команды: `settings_ready()`, `open_settings(route: Option<String>)`, `open_link(link: String)` (`gemini_key` → `https://aistudio.google.com/apikey`, `groq_key` → `https://console.groq.com/keys`, `openai_key` → `https://platform.openai.com/api-keys`, `github` → `https://github.com/Gattapelatta/utter`, `releases` → `https://github.com/Gattapelatta/utter/releases`; другое — ошибка), `copy_text(text: String)` (через `arboard`), `open_microphone_settings()` (`x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`).
  - `overlay_action`: `open_provider` → `open(app, Some("provider"))`, `open_permissions` → `open(app, Some("permissions"))`. Трей «Настройки…», single-instance и стартовая проверка настройки — через `open`; при старте: `onboarding_done == false` → маршрут `onboarding`; иначе нет ключа → `provider`; нет Accessibility → `permissions`.
  - `RunEvent::ExitRequested { code: None, .. }` → `prevent_exit()` (приложение живёт в трее без окон).
- Produces (TS):
  - `routes.ts`: `export type Route = "onboarding" | "permissions" | "general" | "hotkeys" | "translate" | "provider" | "history" | "about"; export function parseRoute(hash: string): Route` (неизвестное → `"general"`).
  - `App.tsx`: при монтировании `useApp.init()`, затем `ipc.settingsReady()`; слушает `ui://navigate`; `onboarding` — во всё окно без бокового меню; остальные — боковое меню (6 разделов §4.3, `permissions` — без пункта меню) + контент; смена раздела — кроссфейд 150 мс (`AnimatePresence mode="wait"`).
  - Компоненты: `Button` (`variant: "primary" | "secondary" | "ghost"`, `:active` scale 0.97 за 120 мс, цвет 150 мс), `Switch`, `Segmented<T>`, `Select<T>`, `Card`, `Row` (подпись + описание + контрол), `Keycap` (`label`, `pressed`: translateY 2px + меньшая тень, 80 мс), `Bar` (`value`, `max`), `TextField`.
  - Верх бокового меню 52 px — зона перетаскивания окна (`data-tauri-drag-region`) под «светофором».

- [ ] **Step 1: Тесты:**

```rust
#[test] fn closing_settings_turns_trial_off()  // shared.trial = true; on_destroyed(...) → false
#[test] fn unknown_link_is_rejected()          // link_url("evil") == None; link_url("gemini_key") == Some("https://aistudio.google.com/apikey")
```
```ts
it("parses routes and falls back to general")
it("onboarding hides the sidebar")
it("navigation event switches section")       // мок listen("ui://navigate") → "history" → заголовок раздела История
it("Segmented calls onChange with the value")
it("Keycap reflects pressed state")           // data-pressed
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать (разделы пока — заглушки с заголовком). **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Ручная проверка:** окно открывается без белой вспышки в обеих темах; закрытие уничтожает окно (Мониторинг системы: процесс WebKit пропадает), повторное открытие из трея работает; повторный запуск `Utter.app` открывает окно.
- [ ] **Step 6: Commit + push** — `feat(settings): lazy settings window shell and UI kit`.

---

### Task 7: Разделы «Основное», «Клавиши», «Перевод» + автозапуск

**Files:**
- Create: `src/settings/sections/{General,Hotkeys,Translate}.tsx`, `src/settings/languages.ts`, `src/settings/sections/sections1.test.tsx`
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-autostart = "2"`), `src-tauri/src/lib.rs`, `src-tauri/src/app/commands.rs` (`update_settings`)

**Interfaces:**
- Consumes: `useApp().update`, `ipc.captureKey`, `ipc.cancelCapture`, событие `ui://hotkey`, компоненты Task 6.
- Produces:
  - `languages.ts`: `export const SPEECH_LANGUAGES: { code: string | null; name: string }[]` — сначала Авто, Русский, English, Українська, Deutsch, Español, Français, затем разделитель и остальные языки Whisper самоназваниями по алфавиту (it, pt, pl, tr, ja, zh, ko, nl, sv, fi, da, no, cs, sk, ro, hu, el, he, ar, hi, id, vi, th, bg, hr, sr, lt, lv, et, kk, be, ka, hy, az, uz, fa, ms).
  - `update_settings`: при изменении `autostart` — `app.autolaunch().enable()/disable()`; при старте приложения — сверить состояние плагина с настройкой (`MacosLauncher::LaunchAgent`).

Содержимое:
- **Основное:** Режим записи (Segmented Удержание/Нажатие), Способ вставки (Вставка/Печать), Основной язык речи (Select), Чистовая обработка (Switch), Абзацы в длинных текстах (Switch, подпись «для записей длиннее 30 с»), Автозапуск, Тема (Тёмная/Светлая/Как в системе — смена применяется сразу с кроссфейдом 250 мс), Язык интерфейса (Авто/Русский/English).
- **Клавиши:** строки «Диктовка» и «Перевод»: `Keycap` с названием клавиши (`keyLabel`), кнопка «Изменить» → состояние «Нажми клавишу… (Esc — отмена)» + кнопка «Отмена» (`cancelCapture`); ошибки под полем: `taken` («Эта клавиша уже назначена на {mode}»), `not_assignable` («Подойдёт правый модификатор, Fn, Caps Lock или F-клавиша»), `timeout`; при размонтировании во время захвата — `cancelCapture`. `Keycap` нажимается при реальном нажатии своей горячей клавиши (`ui://hotkey`).
- **Перевод:** стиль по умолчанию (Segmented Деловой/Разговорный) и две карточки с одной и той же фразой в двух стилях; выбранная подсвечена `accent-translate`. Фразы — в словарях (`translate.example_source`, `translate.example_business`, `translate.example_casual`); для RU-интерфейса исходник по-русски, для EN — по-испански.

- [ ] **Step 1: Тесты:**

```ts
it("changing record mode calls update with Toggle")
it("theme switch applies immediately")                  // data-theme меняется до ответа ipc
it("rejects a key taken by the other mode")             // captureKey → "RightMeta" при translate = RightMeta → текст ошибки, update не вызван
it("shows the assignable-keys hint on not_assignable")
it("cancels capture on unmount")                        // unmount во время захвата → cancelCapture вызван
it("keycap presses on ui://hotkey for its mode only")
it("translation example highlights the selected style")
it("language list starts with Auto and the six main languages")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Ручная проверка автозапуска:** включить → в `~/Library/LaunchAgents/` появился plist Utter; выключить → исчез.
- [ ] **Step 6: Commit + push** — `feat(settings): general, hotkeys and translation sections; autostart`.

---

### Task 8: Раздел «Провайдер» и счётчики использования

**Files:**
- Create: `src/settings/sections/Provider.tsx`, `src/settings/sections/Provider.test.tsx`

**Interfaces:**
- Consumes: `ipc.setApiKey`, `ipc.apiKeyMask`, `ipc.verifyKey`, `ipc.usageToday`, `ipc.freeLimits`, `ipc.openLink`, `useApp().update`.
- Produces: `export function Provider()`; `export function usageBars(day: UsageDay, limits: FreeLimits | null): { key: "stt_requests" | "audio_secs" | "llm_requests" | "llm_tokens"; value: number; max: number }[]` — только поля с известным лимитом.

Содержимое (§4.3 п. 4): три карточки-варианта Gemini / Groq / OpenAI с подписями `provider.gemini_caption` («Бесплатно, вход через Google-аккаунт»), `provider.groq_caption` («Бесплатный тариф»), `provider.openai_caption` («Оплата по факту — не подписка ChatGPT»); поле ключа `type=password` с маской сохранённого ключа в placeholder, «Сохранить», «Проверить» → статус ✓ «Ключ работает» / ✗ `errors.*`; ссылка «Получить бесплатный ключ» (`open_link` `gemini_key`/`groq_key`, для OpenAI — «Получить ключ» `openai_key`); карточка «Сегодня»: запросы, минуты аудио, LLM-запросы, токены (Geist Mono, tabular) и полоски `Bar` относительно `free_limits` с подписью «бесплатный тариф»; обновление каждые 5 с, пока раздел открыт, и по `ui://history`.

- [ ] **Step 1: Тесты:**

```ts
it("usageBars keeps only limited counters")    // Gemini-лимиты {stt_requests: N} → один элемент
it("saving a key refreshes the mask and clears the field")
it("verify shows success and mapped errors")   // reject("bad_key") → текст errors.bad_key
it("switching provider updates settings and mask")
it("free-key link opens the provider page")    // openLink("gemini_key")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** — `feat(settings): provider section with usage bars`.

---

### Task 9: Иконка в трее с состояниями и локализованное меню

**Files:**
- Create: `src-tauri/src/app/tray.rs`
- Modify: `src-tauri/src/lib.rs` (построение трея переезжает в `tray.rs`), `src-tauri/src/app/services.rs` (`TauriUi`: `Status` → трей), `src-tauri/src/app/commands.rs` (`update_settings` → `tray::refresh`)

**Interfaces:**
- Consumes: `AppStatus`, `update_settings`, `settings_window::open`, `UiLanguage`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum TrayVisual { Idle, Recording, Processing { dim: bool } }`
  - `pub fn render_icon(v: TrayVisual, dark_menu_bar: bool) -> Vec<u8>` — RGBA 36×36 (18 pt @2x). Временный знак до бренд-кита (§5.5): пять скруглённых вертикальных полосок шириной 4 px с высотами 12/22/30/22/12 px по центру. `Idle` — чёрный знак, используется как template-изображение. `Recording` — знак цвета меню (белый при `dark_menu_bar`, иначе чёрный) + красная точка `#F2555A` диаметром 10 px в правом нижнем углу (центр 30,30). `Processing { dim }` — то же, альфа точки 255 или 90.
  - `pub enum Lang { Ru, En }`, `pub fn resolve_lang(ui: UiLanguage, system: Option<&str>) -> Lang`, `pub struct TrayLabels { status_idle, status_recording, status_processing, record_mode, hold, toggle, translate_style, business, casual, history, settings, check_updates, update_to /* "Обновить до {v}…" */, quit: &'static str }`, `pub fn labels(lang: Lang) -> TrayLabels`.
  - `pub fn build(app: &AppHandle, s: &Settings) -> tauri::Result<()>` — меню §4.1: статус (неактивный пункт) · Режим записи ▸ (Удержание ✓ / Нажатие) · Стиль перевода ▸ (Деловой / Разговорный) · История · Настройки… · Проверить обновления · Выход; ручки пунктов хранятся в `app.manage(TrayHandles)` и обновляются через `set_text` / `set_checked`.
  - `pub fn set_status(app: &AppHandle, s: AppStatus)` — иконка + текст статуса; при `Processing` — мигание точки раз в 500 мс, пока статус не сменится (счётчик поколений, как `HideGate`); цвет меню — по `NSApp.effectiveAppearance` (Aqua/DarkAqua).
  - `pub fn refresh(app: &AppHandle, s: &Settings)` — галочки и язык пунктов.
  - Пункты меню режима и стиля вызывают `update_settings` с изменённой копией; «История» → `open(app, Some("history"))`.

- [ ] **Step 1: Тесты:**

```rust
#[test] fn idle_icon_has_no_red()               // нет пикселя с r>200 && g<120
#[test] fn recording_icon_has_red_dot()         // пиксель (30,30) == [0xF2,0x55,0x5A,255]
#[test] fn processing_dim_frame_is_translucent()// альфа (30,30) == 90
#[test] fn dark_menu_bar_draws_white_glyph()    // центр средней полоски (18,18) — белый при dark и чёрный иначе
#[test] fn labels_follow_language()             // resolve_lang(Auto, Some("ru-RU")) == Ru; labels(En).quit == "Quit Utter"
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Ручная проверка** в светлой и тёмной теме macOS: точка во время записи, мигание при обработке, возврат к знаку; переключение режима в трее меняет галочку и значение в открытом окне настроек; язык меню меняется вместе с языком интерфейса.
- [ ] **Step 6: Commit + push** — `feat(tray): status icon and localized menu`.

---

### Task 10: Раздел «История»

**Files:**
- Create: `src/settings/sections/History.tsx`, `src/settings/sections/History.test.tsx`

**Interfaces:**
- Consumes: `ipc.historyList`, `ipc.historyRetry`, `ipc.copyText`, событие `ui://history`, `useApp().update` (`history_enabled`).
- Produces: `export function History()`.

Содержимое (§3.6): переключатель «Сохранять историю» вверху; выключение — подтверждение в строке («История будет удалена» · «Удалить» / «Отмена») и затем `update({ history_enabled: false })`; список до 50 записей, новые сверху: дата и время (`Intl.DateTimeFormat` языка интерфейса; сегодня — только время), метка режима (Диктовка / → EN), итоговый текст (свёрнут до 3 строк, клик — развернуть и показать сырую расшифровку), статус: «без обработки» для `degraded`, «Ничего не услышал», «Ошибка: …» для `failed`; кнопки «Копировать» (галочка «Скопировано» на 1,5 с) и «Повторить» (только `failed` с `audio_file`); пустое состояние. Обновление по `ui://history`.

- [ ] **Step 1: Тесты:**

```ts
it("lists entries newest first with mode labels")
it("long entries are clamped until expanded")       // data-clamped; клик → raw виден
it("retry only for failed entries with audio")
it("copy uses copyText and shows feedback")
it("turning history off asks for confirmation")      // update не вызван до «Удалить»
it("refreshes on ui://history")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** — `feat(settings): history section`.

---

### Task 11: Онбординг и экран разрешений

**Files:**
- Create: `src/settings/onboarding/{Onboarding.tsx,steps.ts,DemoPhrase.tsx}`, `src/settings/onboarding/steps/{Welcome,Microphone,Accessibility,ProviderKey,DictationKey,Trial}.tsx`, `src/settings/sections/Permissions.tsx`, `src/settings/onboarding/onboarding.test.tsx`

**Interfaces:**
- Consumes: `ipc.permissionsStatus`, `ipc.requestMicrophone`, `ipc.openMicrophoneSettings`, `ipc.openAccessibilitySettings`, `ipc.setApiKey`, `ipc.verifyKey`, `ipc.captureKey`, `ipc.setTrial`, событие `ui://trial`, `useApp().update`; ключевые части разделов Provider и Hotkeys переиспользуются, а не копируются.
- Produces:
  - `steps.ts`: `export type StepId = "welcome" | "microphone" | "accessibility" | "provider" | "key" | "trial"; export const STEPS: StepId[]; export function firstIncompleteStep(p: PermissionsStatus, hasKey: boolean): StepId` — микрофон не выдан → `welcome`; выдан микрофон, нет Accessibility → `accessibility`; нет ключа → `provider`; всё есть → `key`. `export function canAdvance(step: StepId, p: PermissionsStatus, keyVerified: boolean): boolean` — `microphone` требует `Granted`, `accessibility` — `true`, `provider` — успешную проверку ключа (или явное «Продолжить без проверки» после ошибки `network`).
  - `Onboarding()`; `Permissions()` — экран для `open_permissions`: шаги Microphone и Accessibility подряд + кнопка «Готово» → `general`.

Поведение (§4.4): прогресс-точки, «Назад» на всех шагах кроме первого, «Пропустить» только на 5–6; статусы разрешений опрашиваются раз в 1 с; шаг 1 — `DemoPhrase`: русская фраза печатается по буквам (35 мс/символ), пауза 600 мс, кроссфейд в английский перевод (при reduced motion — сразу финальный текст); шаг 4 — Gemini выбран, ссылка на ключ, «Проверить»; шаг 5 — `Keycap` с текущей клавишей и «Изменить»; шаг 6 — при входе `setTrial(true)`, при уходе/размонтировании `setTrial(false)`, «Зажми {key} и скажи что-нибудь», результат из `ui://trial` показывается крупно в карточке (ошибка — текстом `errors.*`). Завершение (шаг 6 или «Пропустить» на 5–6): `update({ onboarding_done: true, autostart: true })` → маршрут `general`. Анимации: первое появление окна — fade 1 с `cubic-bezier(0.22, 1, 0.36, 1)`; элементы шага — каскад с шагом 60 мс, pop-in 420 мс `cubic-bezier(0.2, 0.9, 0.3, 1.15)`.

- [ ] **Step 1: Тесты:**

```ts
it("firstIncompleteStep resumes at the first missing requirement") // 4 случая из определения выше
it("Next is disabled until the microphone is granted")
it("accessibility status updates by polling")                       // фейковые таймеры, 1 с
it("provider step requires a verified key")
it("skip is offered only on the last two steps")
it("trial mode is on only while the trial step is shown")         // setTrial(true) при входе, setTrial(false) при «Назад» и при unmount
it("finishing enables autostart and marks onboarding done")
it("demo phrase shows the final text at once with reduced motion")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Ручная проверка первого запуска:** удалить `~/Library/Application Support/app.utter.desktop/settings.json`, сбросить разрешения (`tccutil reset Microphone app.utter.desktop`, `tccutil reset Accessibility app.utter.desktop`) **только с согласия пользователя**, пройти онбординг с секундомером — цель < 2 минут вместе с получением ключа Gemini (§1). Пробная запись показывает текст в окне и ничего не вставляет.
- [ ] **Step 6: Commit + push** — `feat(onboarding): six-step first-run flow and permissions screen`.

---

### Task 12: «О приложении» и автообновление

**Files:**
- Create: `src-tauri/src/app/updates.rs`, `src/settings/sections/About.tsx`, `src/settings/sections/About.test.tsx`
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-updater = "2"`), `src-tauri/tauri.conf.json` (`plugins.updater`), `src-tauri/src/lib.rs`, `src-tauri/src/app/{mod.rs,commands.rs,tray.rs}`, `src/lib/{types.ts,ipc.ts}` (`UpdateState`, команды, `ui://update`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum UpdateState { Idle, Checking, UpToDate, Available { version: String, notes: Option<String> }, Downloading { percent: u8 }, Ready, Failed }`
  - `pub fn due(last: Option<SystemTime>, now: SystemTime) -> bool` — `true`, если проверки не было или прошло ≥ 24 ч.
  - Фоновая задача: проверка через 10 с после старта и затем раз в 24 ч (проверка `due` на каждом часовом тике — переживает сон); состояние в `Mutex<UpdateState>` + событие `ui://update`; при `Available` пункт трея меняется на «Обновить до {version}…» (открывает раздел «О приложении»).
  - Команды: `update_state() -> UpdateState`, `check_updates()`, `install_update()` (скачать с прогрессом → `Ready` → `app.restart()`); установка только по нажатию пользователя (§3.7). `app_info() -> { version: String, os: String }`.
  - Конфигурация: `endpoints: ["https://github.com/Gattapelatta/utter/releases/latest/download/latest.json"]`, `pubkey` — из ключа, созданного `pnpm tauri signer generate --ci -w ~/.tauri/utter.key`. `bundle.createUpdaterArtifacts` **не** включать до Плана 3 (иначе локальная сборка требует приватный ключ).
- `About()`: название, версия (Geist Mono), ОС, ссылки GitHub и «Все версии» (`open_link`), «Скопировать диагностику» (`copy_diagnostics` → `copy_text`, галочка «Скопировано»), блок обновлений по `UpdateState` («Проверить обновления», «Доступна версия X» + «Установить и перезапустить», прогресс, «Последняя версия», «Не удалось проверить обновления»).

- [ ] **Step 1: Тесты:**

```rust
#[test] fn first_check_is_due()
#[test] fn check_is_due_after_24_hours()         // 23 ч → false, 24 ч → true
#[test] fn clock_going_backwards_is_due()        // now < last → true
```
```ts
it("shows version and copies diagnostics")
it("offers install only when an update is available")
it("shows a calm message when the check fails")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать; сгенерировать ключ подписи (пароль не задаётся), публичный ключ — в конфиг, приватный **не** коммитить. **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Ручная проверка:** пока репозиторий приватный, проверка честно показывает «Не удалось проверить обновления», без зависаний и всплывающих ошибок. Сказать пользователю, где лежит `~/.tauri/utter.key`, и попросить сохранить копию (без него установленные версии не смогут обновиться).
- [ ] **Step 6: Commit + push** — `feat: about section and auto-update`.

---

### Task 13: Живая проверка, бюджеты и документы

**Files:**
- Modify: `docs/qa/macos-paste-matrix.md` (результаты), `docs/superpowers/specs/2026-10-06-utter-design.md` (M16: §6.2 — активный, а не listen-only tap; §6.5 — `gemini-3.5-flash-lite`; §6.4 — итог по `tauri-nspanel`; §5.1 — правило контраста), `docs/superpowers/HANDOFF.md`
- Исправления — в модулях, где найдены проблемы, через superpowers:systematic-debugging, каждое с регрессионным тестом.

- [ ] **Step 1:** `cargo test --lib`, `cargo clippy --all-targets` (0), `pnpm test`, `pnpm build` — всё зелёное; собрать `scripts/dev-bundle.sh`.
- [ ] **Step 2: Вставка на русской раскладке:** `scripts/dev-bundle.sh --debug` + `scripts/paste-matrix.sh …` при включённой русской раскладке — все PASS.
- [ ] **Step 3: Стресс (пользователь у Mac):** 100 диктовок подряд без перезапуска; 20 нажатий короче 0,3 с; 3 записи в очереди; сон → пробуждение → диктовка; отключение наушников во время записи. Ожидание: ни падений, ни зависших состояний пилюли или иконки трея, ни потерянных вставок.
- [ ] **Step 4: Бюджеты §8.4** по логу (`timing: key→pill painted`, `timing: start→first sample`, `timing: delivered`) — медиана и максимум за 20 диктовок; память и CPU в простое: `ps -axo pid,%cpu,rss,comm | grep -Ei 'utter|WebKit'` при закрытом и открытом окне настроек. Цели: < 50 мс, < 100 мс, ≤ 1,5 с (p50), < 80 МБ суммарно при закрытом окне.
- [ ] **Step 5: Визуальный обход** обеих тем × RU/EN × reduced motion/transparency: каждый раздел, онбординг, все состояния пилюли; найденные огрехи — исправить в этой задаче.
- [ ] **Step 6:** Обновить спецификацию (M16 и решения этого плана) и `HANDOFF.md`: статус Плана 2, новые решения, оставшиеся долги (M1, M4, M5, M9, M10, M12–M14, M17), что проверено и что нет, как начать План 3.
- [ ] **Step 7: Commit + push** — `test: Plan 2 live QA, budgets and docs`.

После Task 13: финальное ревью всей ветки свежим ревьюером на самой сильной модели, затем superpowers:finishing-a-development-branch (слияние в `main` — только с согласия пользователя). Иконка приложения и бренд — отдельным шагом через скил `brandkit` после этого плана.


---

## Изменения после утверждения дизайна «Капли» (2026-10-08)

Пользователь утвердил новый облик пилюли (спецификация §4.2, §5.2–5.4 переписаны; эталон — `docs/design/drop-prototype.html`) и отказался от звуков. Задачи 4 и 5 выполнены по старому дизайну; их результат заменяют задачи 4R и 5R ниже, звук убирает задача 2R. Выполняются **до** Task 6, в порядке 2R → 4R → 5R. В Task 7 переключателя «Звуки» нет.

### Task 2R: Убрать звуки

**Files:** Delete `src-tauri/src/sounds.rs`; Modify `src-tauri/src/{lib.rs,settings.rs}`, `src-tauri/src/app/{orchestrator.rs,services.rs}`, `src-tauri/Cargo.toml` (если фичи objc2 больше не нужны), `src/lib/types.ts`, `src/lib/store.test.ts`, `src/i18n/{ru,en}.json` (ключи `general.sounds*`).

- [ ] **Step 1: Тест** `settings::tests::old_file_with_sounds_field_still_loads` — файл `{"schema_version":1,"sounds":true,"cleanup":false}` загружается, `cleanup == false`. Удалить тесты `sounds_*` оркестратора и `sounds.rs`.
- [ ] **Step 2:** FAIL (поле ещё есть — тест проходит; поэтому сначала удалить поле `sounds` из `Settings` и убедиться, что тест компилируется и проходит, а старые тесты звука не компилируются → удалить их). **Step 3:** удалить `Sounds`, `Cue`, `TauriSounds`, поле `Deps.sounds`. **Step 4:** `cargo test --lib`, clippy 0, `pnpm test`.
- [ ] **Step 5: Commit + push** — `refactor: remove start/stop sounds (user decision)`.

### Task 4R: Капля — персонаж и остров (React)

**Files:**
- Create: `src/overlay/drop/springs.ts`, `src/overlay/drop/eye.ts`, `src/overlay/drop/effects.ts`, `src/overlay/drop/Drop.tsx`, `src/overlay/drop/Island.tsx`, `src/overlay/drop/drop.test.ts`, `src/overlay/drop/Island.test.tsx`
- Modify: `src/overlay/main.tsx`, `src/overlay/overlay.css`, `src/styles/app.css` (Manrope), `package.json` (`@fontsource-variable/manrope`), словари (`pill.copied_short` = «⌘V»)
- Delete: `src/overlay/{Pill.tsx,Pill.test.tsx,Wave.tsx,Check.tsx,pillMotion.ts,pillMotion.test.ts}`

**Interfaces:**
- `springs.ts`: `export interface Spring { x: number; v: number; t: number; k: number; c: number }`; `export function spring(x: number, omega: number): Spring` (k = ω², c = 2ω); `export function stepSpring(s: Spring, dt: number): void` (4 подшага).
- `eye.ts`: `export function eyePath(cx: number, cy: number, lid: number, smile: number): string` — формулы эталона (RX 7.6, RY 11.5).
- `effects.ts`: `export function satellite(t: number): { x: number; y: number; r: number; opacity: number; front: boolean }[]`, `export function glints(t: number): { x: number; y: number; scale: number; rotate: number; opacity: number }[]`, `export function clouds(t: number): { x: number; y: number; scale: number; opacity: number }[]` — чистые функции времени с параметрами эталона.
- `Drop.tsx`: `export type Mood = "idle" | "listening" | "thinking" | "happy" | "sad"`; `export function Drop(props: { mood: Mood; level: number; translate: boolean; reduced: boolean })` — SVG рисуется один раз, анимация — `requestAnimationFrame`, атрибуты меняются через refs (без ре-рендера React на кадр); цикл останавливается, когда все пружины в покое и эффектов нет.
- `Island.tsx`: `export function moodFor(s: OverlayState): Mood` и `export function Island(props: { state: OverlayState; level: number; now: number })` — чёрный остров 360 × 104 (`0 0 40 40`), пара Капля (58 × 60) + правый блок (таймер Manrope 600 21 px / текст ошибки / «⌘V») по центру, промежуток 44 px; таймер — только при записи; при `pending > 0` — маленькая цифра у таймера.

- [ ] **Step 1: Тесты:**

```ts
it("critically damped spring reaches the target without overshoot") // spring(0,7), t=1; 2 с шагами 1/60 → x ∈ [0.999,1], никогда > 1
it("closed eye is a thin crescent, open eye is tall")               // высота path при lid 1 < 4, при lid 0 > 20
it("smile lifts the eye above its centre")                           // все y пути при smile 1 ≤ cy + 2.5
it("satellite makes one lap and fades out")                          // opacity(0)=0, opacity(0.85)>0.5, opacity(1.7)=0; front меняется
it("clouds appear in sequence and vanish by 3.1 s")
it("moodFor maps overlay states")    // recording→listening, processing→thinking, done pasted→happy, done failed→sad, error→sad
it("island shows the timer only while recording and error text on errors")
it("translate mode turns rings cyan")   // при translate stroke колец → rgb(74,216,236) после схождения пружины
it("reduced motion keeps the character still")
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать, перенося формулы из эталона (не «на глаз»). **Step 4:** PASS, `pnpm build`.
- [ ] **Step 5: Визуальная сверка** с `docs/design/drop-prototype.html` в браузере (dev-панель состояний под `import.meta.env.DEV`): все пять настроений и оба режима.
- [ ] **Step 6: Commit + push** — `feat(overlay): the Drop character in a notch island`.

### Task 5R: Окно-остров у чёлки

**Files:** Modify `src-tauri/src/app/overlay.rs`, `src-tauri/src/app/orchestrator.rs` (`DONE_VISIBLE` 2200 мс, `ERROR_VISIBLE` 3000 мс), `src-tauri/tauri.conf.json` (окно `overlay` 440 × 170), `src/overlay/main.tsx`, `src/lib/ipc.ts`.

**Interfaces:**
- `pub struct Notch { pub width: f64, pub height: f64 }`; `pub fn island_origin(screen_x: f64, screen_y: f64, screen_w: f64, window_w: f64) -> (f64, f64)` — верх по центру экрана (чистая функция, в точках).
- macOS: размер выреза — `NSScreen.safeAreaInsets.top` и `auxiliaryTopLeftArea/RightArea` экрана под курсором; событие `overlay://screen` `{ notch: { width, height } | null }` перед показом; остров вырастает от размера выреза (или от 0 × 0 у верхнего края без выреза).
- Окно у верхнего края экрана (не рабочей области), уровень выше строки меню (оставить 101), `HIDE_AFTER` = 500 мс (втягивание острова).

- [ ] **Step 1: Тесты:** `island_origin_is_top_centre` (экран x=0, w=1512, окно 440 → (536, 0)); `island_origin_on_second_display` (x=1512, w=1920 → (2252, 0)).
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Живая проверка с пользователем:** остров вырастает из чёлки на MacBook, на внешнем мониторе — из-под верхнего края; Капля не срезана камерой (если срезана — уменьшить Каплю, форму острова не трогать); фокус остаётся в приложении; нагрузка CPU во время записи по `ps` — записать.
- [ ] **Step 7: Commit + push** — `feat(overlay): notch island window`.

### Task 5S: Обработка длинных записей по ходу речи (добавлена 2026-10-08, выполнена)

Решение пользователя после замеров (записи 13–14 с вставлялись то за 1,5, то за 4–5 с). Спецификация §3.4a.

- `audio/segment.rs`: `pub fn find_cut(mono: &[f32], rate: u32) -> Option<usize>` (MIN 10 с, пауза 400 мс, принудительный разрез на 25 с в самом тихом кадре последних 5 с); тесты: нет разреза до 10 с, разрез посреди паузы, короткие паузы не режутся, принудительный разрез, нет разреза до 25 с без пауз.
- `recorder.rs`: `AudioSource::peek(from) -> Option<Captured>` (по умолчанию `None`; у микрофона — `Cmd::Peek`, `None` после смены устройства).
- `orchestrator.rs`: `check_pieces` в тике раз в 500 мс (кроме режима абзацев), `Split { pieces, rest }` при отправке, `combine` склеивает по порядку, запасной путь «одним запросом» при `Err(Provider)` куска, `JobResult.requests` → `Usage::count_stt_request`; отмена/провал старта сбрасывают куски. Тесты: `long_recording_is_processed_in_pieces_while_speaking`, `failed_piece_falls_back_to_one_request`, `paragraphs_keep_one_request`, `cancelled_recording_drops_its_pieces`.
- Живая проверка: диктовка 30–60 с; в логе — строки `piece sent while speaking` и `total_ms` после отпускания.
