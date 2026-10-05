# Utter — План 1: ядро на macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Рабочее приложение для macOS в трее: клавиша → запись → Groq/OpenAI → чистый текст или английский перевод надёжно вставляется в активное поле; минимальный интерфейс (простая пилюля + служебная страница настроек).

**Architecture:** Tauri 2. Вся логика — в Rust-ядре (`src-tauri/src/`), разбитом на модули с чистыми, тестируемыми ядрами (`controller`, `audio::dsp`, `pipeline`, `jobs`, `hotkey::router`, `output::deliver`) и тонкими платформенными обёртками (`hotkey::macos`, `output::macos`, `recorder`, `permissions`). `app::orchestrator` связывает модули и общается с React-окнами через события и команды Tauri.

**Tech Stack:** Rust stable, Tauri 2.x, tokio, reqwest (rustls, multipart, json), cpal, rubato, hound, arboard, enigo, keyring 3, core-graphics, objc2 / objc2-app-kit / objc2-av-foundation, tauri-nspanel, tracing; фронтенд — pnpm, Vite, React 19, TypeScript, Tailwind 4, Zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-06-utter-design.md`

**Дальнейшие планы (пишутся после завершения этого):** План 2 — премиальный UI (§4.2–4.4, §5 спеки: пилюля с анимациями, настройки, онбординг, история, темы, звуки, i18n, автозапуск, автообновление). План 3 — Windows-реализации `hotkey`/`output`/`permissions`, CI/релизы, README, матрица Windows (§8–10).

## Global Constraints

- macOS 13+; приложение не показывается в Dock (`ActivationPolicy::Accessory`); `macOSPrivateApi: true`.
- Порог записи **0,3 с**; лимит **10 мин**; предупреждение за **30 с** до лимита.
- Запрос к провайдеру: таймаут **15 с**, для аудио > 2 мин — **15 с + 5 с за каждую минуту**; на 5xx — **один** повтор через **1 с**.
- Groq: base URL `https://api.groq.com/openai/v1`, STT `whisper-large-v3-turbo`, текст `openai/gpt-oss-120b`. OpenAI: `https://api.openai.com/v1`, STT `gpt-4o-mini-transcribe`, текст `gpt-4o-mini`. Все — только в `providers/defaults.rs`.
- STT: `temperature=0`, `language` передаётся, только если не «Авто».
- Абзацы: только если настройка включена **и** запись > **30 с**; разделитель `\n\n`.
- Аудио на отправку: WAV 16 кГц, mono, 16-bit; high-pass **80 Гц**; обрезка тишины с запасом **150 мс**; нормализация пика к **−1 dBFS**.
- История — **50** записей; аудио неудачных записей — не более **5**.
- Вставка: подтверждение записи в буфер до **200 мс**; ожидание отпускания модификаторов до **500 мс**; Cmd+V — keycode **9** с флагами **только Command**; «Печать» — порции по **20** символов, пауза **5 мс**, `\n` → Shift+Enter. Текст остаётся в буфере.
- API-ключи — только в Keychain (`keyring`), в логах и UI — только маска.
- Никаких `unwrap()`/`expect()` в рабочих путях (допустимы в тестах и в `main` до старта рантайма).
- Настройки хранятся собственным serde-JSON с атомарной записью (temp + rename) вместо `tauri-plugin-store` — так модуль тестируется без Tauri (спека §6.2 обновляется в задаче 1).
- Каждый коммит сразу пушится: `git push`.
- Сообщения коммитов заканчиваются строкой `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Автоповтор клавиши** (F13 зажата → ОС шлёт повторные KeyDown): в «Нажатии» запись не должна сразу остановиться, в «Удержании» — не должна перезапускаться. → тест `router_ignores_autorepeat` (задача 9).
2. **Caps Lock на macOS не присылает KeyUp** (только flagsChanged при каждом нажатии) → роутер выдаёт `Tap(mode)`, контроллер трактует `Tap` как нажатие в режиме «Нажатие» в любом режиме записи. → тесты `caps_lock_emits_tap`, `tap_toggles_in_hold_mode` (задачи 9, 3).
3. **Микрофон не стартовал / пропал** → контроллер не должен застрять в `Recording`. → тест `abort_returns_to_idle` (задача 3) + ошибка в пилюле (задача 12).
4. **LLM добавил обрамление** («Here is the translation:», кавычки, эхо тегов `<transcript>`) или ответил на вопрос вместо обработки → постобработка и жёсткий промпт. → тест `postprocess_strips_wrappers` (задача 6).
5. **Неудача посередине очереди** (задача 2 из 3 упала) → задачи 1 и 3 всё равно доставляются по порядку, 3 не ждёт вечно. → тест `failed_job_releases_order` (задача 7).

---

## Файловая структура

```
LICENSE                                MIT
package.json, pnpm-lock.yaml, vite.config.ts, tsconfig.json, index files
settings.html, overlay.html            два entry-point Vite
src/
  lib/ipc.ts                           типизированные обёртки invoke/listen
  lib/types.ts                         TS-зеркало Rust-типов (Settings, OverlayState, KeyId …)
  overlay/main.tsx, overlay/Pill.tsx   простая пилюля (полировка — План 2)
  settings/main.tsx, settings/DevSettings.tsx   служебная страница настроек
  styles/tokens.css                    токены цветов §5.1 (тёмная тема)
src-tauri/
  Cargo.toml, tauri.conf.json, capabilities/default.json
  src/main.rs                          вызывает utter_lib::run()
  src/lib.rs                           сборка Tauri-приложения, трей, окна
  src/types.rs                         Mode, KeyId — общие типы
  src/settings.rs
  src/secrets.rs
  src/controller.rs
  src/audio/mod.rs, audio/dsp.rs       чистая обработка звука
  src/recorder.rs                      захват cpal на своём потоке
  src/providers/mod.rs, providers/client.rs, providers/defaults.rs, providers/error.rs
  src/pipeline/mod.rs, pipeline/prompts/{cleanup,translate_business,translate_casual}.md, pipeline/hallucinations.txt
  src/jobs.rs
  src/hotkey/mod.rs, hotkey/router.rs, hotkey/macos.rs
  src/output/mod.rs, output/clipboard.rs, output/macos.rs
  src/history.rs, src/usage.rs
  src/permissions.rs
  src/app/mod.rs, app/orchestrator.rs, app/commands.rs, app/overlay.rs, app/diagnostics.rs
scripts/paste-matrix.sh                полуавтоматическая проверка вставки (TextEdit/Notes)
docs/qa/macos-paste-matrix.md          чек-лист ручной матрицы
```

---

### Task 0: Инструменты и каркас приложения

**Files:**
- Create: весь каркас Tauri (`pnpm create tauri-app`, шаблон React + TS), `LICENSE`, `.gitignore`, `settings.html`, `overlay.html`, `src-tauri/src/types.rs`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs`, `vite.config.ts`

**Interfaces:**
- Produces (`src-tauri/src/types.rs`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)] pub enum Mode { Dictation, Translate }`
  - `#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)] pub enum KeyId { RightAlt, RightMeta, RightCtrl, RightShift, CapsLock, Fn, F(u8), Escape, Raw(u32) }` — `RightAlt` = Right Option на Mac, `RightMeta` = Right Command; `Raw` — платформенный keycode любой другой клавиши.

- [ ] **Step 1: Установить инструменты (спросить у пользователя разрешение на загрузку перед этим шагом).** Rust через официальный `rustup` (`https://sh.rustup.rs`, профиль minimal), Node.js LTS — официальный архив для darwin-arm64 с `nodejs.org` в `~/.local/node`, pnpm через `corepack enable`. Проверка: `rustc --version`, `node --version`, `pnpm --version` печатают версии.
- [ ] **Step 2: Сгенерировать каркас** в корне репозитория: `pnpm create tauri-app` (identifier `app.utter.desktop`, product name `Utter`, React, TypeScript, pnpm), не затирая `docs/`. Добавить `LICENSE` (MIT, `Copyright (c) 2026 Gattapelatta`).
- [ ] **Step 3: Настроить окна в `tauri.conf.json`:**
  - `settings`: `url: "settings.html"`, 900×620, `visible: false`, `title: "Utter"`.
  - `overlay`: `url: "overlay.html"`, 360×72, `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `visible: false`, `focus: false`, `shadow: false`, `resizable: false`.
  - `app.macOSPrivateApi: true`; в `vite.config.ts` — multi-page input для `settings.html` и `overlay.html`.
- [ ] **Step 4: Трей и Accessory-режим в `lib.rs`:** в `setup` — `app.set_activation_policy(ActivationPolicy::Accessory)`; иконка трея (временная — иконка шаблона Tauri) с меню «Настройки…» (показать окно `settings` и `set_focus`) и «Выход». Подключить `tauri-plugin-single-instance` (повторный запуск показывает `settings`).
- [ ] **Step 5: Проверить запуск:** `pnpm tauri dev` → в menu bar есть иконка, в Dock нет; «Настройки…» открывает окно; «Выход» завершает процесс.
- [ ] **Step 6: Commit + push**

```bash
git add -A && git commit -m "chore: scaffold Tauri 2 tray app" && git push
```

---

### Task 1: Настройки

**Files:**
- Create: `src-tauri/src/settings.rs`
- Modify: `docs/superpowers/specs/2026-10-06-utter-design.md` (§6.2 `settings/`: «собственный serde-JSON с атомарной записью»)

**Interfaces:**
- Consumes: `Mode`, `KeyId`.
- Produces:
  - `pub enum RecordMode { Hold, Toggle }`, `pub enum TranslateStyle { Business, Casual }`, `pub enum ProviderKind { Groq, OpenAi }`, `pub enum InsertMethod { Paste, Type }`, `pub enum Theme { Dark, Light, System }`, `pub enum UiLanguage { Auto, Ru, En }` (все `Serialize/Deserialize`, `Clone, Copy, Debug, PartialEq`).
  - `pub struct Hotkeys { pub dictation: KeyId, pub translate: KeyId }`
  - `pub struct Settings { pub schema_version: u32, pub record_mode: RecordMode, pub insert_method: InsertMethod, pub speech_language: Option<String>, pub cleanup: bool, pub paragraphs: bool, pub sounds: bool, pub autostart: bool, pub theme: Theme, pub ui_language: UiLanguage, pub translate_style: TranslateStyle, pub provider: ProviderKind, pub hotkeys: Hotkeys, pub history_enabled: bool, pub onboarding_done: bool }` — `#[serde(default)]` на структуре.
  - `pub const SCHEMA_VERSION: u32 = 1;`
  - `impl Settings { pub fn defaults_for(system_lang: Option<&str>) -> Settings }`
  - `pub fn load(path: &Path, system_lang: Option<&str>) -> Settings` — нет файла / битый JSON → значения по умолчанию (битый файл переименовать в `settings.json.bak`).
  - `pub fn save(path: &Path, s: &Settings) -> std::io::Result<()>` — атомарно.
  - `pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()>` — общий хелпер (используют `history`, `usage`).

- [ ] **Step 1: Написать падающие тесты** (`#[cfg(test)] mod tests` в `settings.rs`, `tempfile`):

```rust
#[test] fn defaults_match_spec() {
    let s = Settings::defaults_for(Some("ru"));
    assert_eq!(s.record_mode, RecordMode::Hold);
    assert_eq!(s.insert_method, InsertMethod::Paste);
    assert_eq!(s.speech_language.as_deref(), Some("ru"));
    assert!(s.cleanup && !s.paragraphs && s.sounds && s.history_enabled);
    assert_eq!(s.theme, Theme::Dark);
    assert_eq!(s.translate_style, TranslateStyle::Business);
    assert_eq!(s.provider, ProviderKind::Groq);
    assert_eq!(s.hotkeys.dictation, KeyId::RightAlt);
    assert_eq!(s.hotkeys.translate, KeyId::RightMeta);
    assert!(!s.onboarding_done);
}
#[test] fn unknown_system_lang_means_auto() { assert_eq!(Settings::defaults_for(None).speech_language, None); }
#[test] fn roundtrip_save_load() { /* save изменённые настройки → load → assert_eq */ }
#[test] fn corrupt_file_falls_back_and_keeps_backup() { /* записать "{oops" → load == defaults, существует settings.json.bak */ }
#[test] fn missing_fields_take_defaults() { /* записать {"schema_version":1,"cleanup":false} → cleanup false, остальное по умолчанию */ }
```

`defaults_for` на Windows (План 3) вернёт `translate: KeyId::RightCtrl` — здесь `#[cfg(target_os = "macos")]`-ветка `RightMeta`, `#[cfg(windows)]`-ветка `RightCtrl`.

- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml settings::` → FAIL (не компилируется).
- [ ] **Step 3: Реализовать** типы и функции из Interfaces. `speech_language` по умолчанию — двухбуквенный код системного языка, если он есть в списке Whisper-языков (`["ru","en","uk","de","es","fr","it","pt","pl","tr","ja","zh","ko"]`, полный список Whisper добавит План 2), иначе `None`.
- [ ] **Step 4:** тесты → PASS.
- [ ] **Step 5: Commit + push** (`feat(core): settings model with atomic JSON storage`), включая правку спеки.

---

### Task 2: Секреты

**Files:** Create: `src-tauri/src/secrets.rs`

**Interfaces:**
- Produces:
  - `pub trait SecretStore: Send + Sync { fn get(&self, provider: ProviderKind) -> Option<String>; fn set(&self, provider: ProviderKind, key: &str) -> Result<(), SecretError>; fn delete(&self, provider: ProviderKind) -> Result<(), SecretError>; }`
  - `pub struct KeychainStore;` — `keyring::Entry::new("app.utter.desktop", "groq" | "openai")`.
  - `pub struct MemoryStore` (для тестов других модулей, `Mutex<HashMap>`).
  - `pub fn mask(key: &str) -> String` — первые 4 символа + `••••` + последние 4; ключ ≤ 8 символов → `••••`.
  - `#[derive(thiserror::Error, Debug)] pub enum SecretError { #[error("keychain: {0}")] Backend(String) }`

- [ ] **Step 1: Тесты:**

```rust
#[test] fn mask_long_key() { assert_eq!(mask("gsk_abcdefghijkl1234"), "gsk_••••1234"); }
#[test] fn mask_short_key() { assert_eq!(mask("abc"), "••••"); }
#[test] fn memory_store_roundtrip() { /* set → get == Some, delete → None */ }
#[test] fn set_trims_whitespace() { /* set(" gsk_x \n") → get == "gsk_x" (для обоих store) */ }
```

- [ ] **Step 2:** запустить → FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Ручная проверка Keychain:** временный `#[ignore]`-тест `keychain_roundtrip` (set/get/delete реального `KeychainStore` с провайдером Groq) — `cargo test -- --ignored keychain_roundtrip` → PASS.
- [ ] **Step 6: Commit + push** (`feat(core): keychain-backed secret store`).

---

### Task 3: Контроллер (машина состояний)

**Files:** Create: `src-tauri/src/controller.rs`

**Interfaces:**
- Consumes: `Mode`, `RecordMode`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum HotkeyEvent { Down(Mode), Up(Mode), Tap(Mode), OtherKey, Escape }`
  - `pub enum CancelReason { TooShort, Escape, Chord, Aborted }`
  - `#[derive(Debug, PartialEq)] pub enum Command { Start(Mode), Submit { mode: Mode, duration: Duration }, Cancel(CancelReason), LimitWarning }`
  - `pub enum State { Idle, Recording { mode: Mode, started: Instant, warned: bool } }`
  - `pub const MIN_RECORDING: Duration = 300ms; MAX_RECORDING: Duration = 600s; WARN_BEFORE: Duration = 30s;`
  - `impl Controller { pub fn new(mode: RecordMode) -> Self; pub fn set_record_mode(&mut self, mode: RecordMode); pub fn handle(&mut self, ev: HotkeyEvent, now: Instant) -> Vec<Command>; pub fn tick(&mut self, now: Instant) -> Vec<Command>; pub fn abort(&mut self) -> Vec<Command>; pub fn state(&self) -> &State }`

Правила (из спеки §3.2–3.3):

| Состояние | Событие | Hold | Toggle |
|---|---|---|---|
| Idle | `Down(m)` / `Tap(m)` | → Recording(m), `Start(m)` | то же |
| Idle | прочее | ничего | ничего |
| Recording(m) | `Up(m)` | < 0,3 с → `Cancel(TooShort)`, иначе `Submit` | игнор |
| Recording(m) | `Down(m)` | игнор | < 0,3 с → `Cancel(TooShort)`, иначе `Submit` |
| Recording(m) | `Tap(m)` | как `Down(m)` в Toggle | как `Down(m)` в Toggle |
| Recording(m) | `Down(другой)`, `OtherKey` | `Cancel(Chord)` | игнор |
| Recording(m) | `Escape` | `Cancel(Escape)` | `Cancel(Escape)` |
| Recording | `tick`, прошло ≥ 9:30 и `!warned` | `LimitWarning` (один раз) | то же |
| Recording | `tick`, прошло ≥ 10:00 | `Submit` | то же |
| Recording | `abort()` | `Cancel(Aborted)` | то же |

Любая `Cancel`/`Submit` переводит в Idle. `set_record_mode` во время записи применяется к следующей записи.

- [ ] **Step 1: Тесты** (время — `Instant` + смещения):

```rust
#[test] fn hold_press_release_submits() // Down(D) t0, Up(D) t0+1s → [Start(D)], [Submit{D, 1s}]
#[test] fn hold_short_press_cancels()   // Up через 200ms → [Cancel(TooShort)]
#[test] fn hold_other_key_cancels_chord()
#[test] fn hold_second_hotkey_cancels_chord() // Down(D), Down(T) → Cancel(Chord)
#[test] fn toggle_two_presses_submit()  // Down, Up, Down через 2s → Submit{2s}
#[test] fn toggle_ignores_up_and_other_keys()
#[test] fn toggle_second_hotkey_ignored()
#[test] fn escape_cancels_in_both_modes()
#[test] fn tap_toggles_in_hold_mode()   // Hold: Tap(D) → Start, Tap(D) через 3s → Submit
#[test] fn limit_warning_once_then_auto_submit() // tick 570s → [LimitWarning], tick 580s → [], tick 600s → [Submit{600s}]
#[test] fn abort_returns_to_idle()      // Start → abort() → [Cancel(Aborted)], state Idle
#[test] fn idle_ignores_up_escape_other()
#[test] fn mode_change_applies_to_next_recording()
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать по таблице. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** (`feat(core): recording state machine`).

---

### Task 4: Обработка звука (чистые функции)

**Files:** Create: `src-tauri/src/audio/mod.rs`, `src-tauri/src/audio/dsp.rs`

**Interfaces:**
- Produces (`audio::dsp`):
  - `pub const TARGET_RATE: u32 = 16_000; FRAME_MS: u32 = 20; VOICE_RMS: f32 = 0.01; PAD_MS: u32 = 150;`
  - `pub fn downmix(interleaved: &[f32], channels: u16) -> Vec<f32>`
  - `pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32>` — rubato `SincFixedIn` целиком по буферу; при `from == to` возвращает копию.
  - `pub fn high_pass(samples: &mut [f32], rate: u32, cutoff_hz: f32)` — одно-полюсный RC-фильтр.
  - `pub fn rms(samples: &[f32]) -> f32`
  - `pub fn voiced_range(samples: &[f32], rate: u32) -> Option<Range<usize>>` — кадры по 20 мс, первый и последний с RMS > `VOICE_RMS`, расширение на 150 мс в обе стороны с клампом; нет таких кадров → `None`.
  - `pub fn normalize_peak(samples: &mut [f32], target_dbfs: f32)` — не усиливать больше чем в 10 раз (чтобы не раздувать шум).
  - `pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8>` — hound, mono, 16-bit, клип к [-1, 1].
- Produces (`audio`):
  - `pub struct Captured { pub samples: Vec<f32>, pub rate: u32, pub channels: u16 }` (interleaved)
  - `pub struct Prepared { pub wav: Vec<u8>, pub audio_secs: f32 }`
  - `pub fn prepare(c: Captured) -> Option<Prepared>` — downmix → high_pass(80 Гц) на исходной частоте → resample в 16 кГц → `voiced_range` (None → `None`, «Ничего не услышал») → normalize_peak(−1 dBFS) → encode_wav; `audio_secs` — длительность обрезанного фрагмента.

- [ ] **Step 1: Тесты:**

```rust
#[test] fn downmix_stereo_averages()      // [1.0,0.0, 0.5,0.5] → [0.5, 0.5]
#[test] fn resample_48k_to_16k_length()   // 48000 сэмплов → 16000 ± 32
#[test] fn high_pass_removes_dc()         // константа 0.5, 1 c @16k → |mean последней половины| < 0.01
#[test] fn silence_has_no_voiced_range()  // нули → None
#[test] fn voiced_range_pads_150ms()      // тишина 1с + синус 0.3 амплитуды 1с + тишина 1с @16k → range ≈ (16000-2400)..(32000+2400) ± 320
#[test] fn normalize_hits_minus_1_dbfs()  // пик 0.2 → пик ≈ 0.891 ± 0.005
#[test] fn normalize_caps_gain_at_10x()   // пик 0.01 → пик ≈ 0.1
#[test] fn wav_header_is_16k_mono_16bit() // hound::WavReader читает spec
#[test] fn ten_minutes_fit_groq_limit()   // encode_wav(600 c @16k).len() < 25 * 1024 * 1024
#[test] fn prepare_returns_none_for_silence()
#[test] fn prepare_reports_trimmed_duration()
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** (`feat(core): audio DSP pipeline`).

---

### Task 5: Провайдеры (клиент API в формате OpenAI)

**Files:** Create: `src-tauri/src/providers/{mod.rs,client.rs,defaults.rs,error.rs}`

**Interfaces:**
- Consumes: `ProviderKind`.
- Produces:
  - `pub struct ProviderConfig { pub kind: ProviderKind, pub base_url: String, pub api_key: String, pub stt_model: String, pub text_model: String }`
  - `pub fn defaults::config_for(kind: ProviderKind, api_key: String) -> ProviderConfig`
  - `pub struct Completion { pub text: String, pub total_tokens: u32 }`
  - `#[derive(thiserror::Error, Debug, Clone, PartialEq)] pub enum ProviderError { Unauthorized, RateLimited { retry_after: Option<Duration> }, Network, Timeout, Server(u16), BadResponse(String) }`
  - `#[async_trait] pub trait SpeechToText: Send + Sync { async fn transcribe(&self, wav: Vec<u8>, language: Option<String>, audio_secs: f32) -> Result<String, ProviderError>; }`
  - `#[async_trait] pub trait TextModel: Send + Sync { async fn complete(&self, system: &str, user: &str) -> Result<Completion, ProviderError>; }`
  - `pub struct OpenAiCompatible` — `pub fn new(cfg: ProviderConfig) -> Self` (общий `reqwest::Client`: rustls, HTTP/2, `pool_idle_timeout` 90 с); `pub async fn prewarm(&self)` (HEAD/GET `{base}/models`, ошибки игнорируются); `pub async fn verify_key(&self) -> Result<(), ProviderError>` (GET `{base}/models`); реализует оба трейта.
  - `pub fn request_timeout(audio_secs: f32) -> Duration`
  - `pub fn parse_reset(value: &str) -> Option<Duration>` — понимает `"30"`, `"7.66s"`, `"2m59.56s"`, `"1h2m3s"`.

Запросы: STT — `POST {base}/audio/transcriptions`, multipart: `file` (`audio.wav`, `audio/wav`), `model`, `temperature=0`, `response_format=json`, `language` (опционально) → поле `text`. Текст — `POST {base}/chat/completions`, `{"model", "messages":[system,user], "temperature":0.2}` → `choices[0].message.content`, `usage.total_tokens`. Маппинг: 401/403 → `Unauthorized`; 429 → `RateLimited` (`retry-after`, иначе `x-ratelimit-reset-requests`, иначе `x-ratelimit-reset-tokens`, через `parse_reset`); 5xx → один повтор через 1 с, потом `Server(code)`; таймаут reqwest → `Timeout`; connect-ошибки → `Network`; неожиданный JSON → `BadResponse`.

- [ ] **Step 1: Сверить модели.** Открыть актуальные страницы моделей и лимитов Groq и OpenAI; если модель из Global Constraints устарела — взять рекомендованного провайдером преемника, обновить `defaults.rs`, Global Constraints этого плана и §6.5 спеки.
- [ ] **Step 2: Тесты** (`wiremock`, `#[tokio::test]`):

```rust
#[tokio::test] async fn transcribe_sends_multipart_and_returns_text() // проверка model, temperature=0, language=ru, Authorization: Bearer k
#[tokio::test] async fn transcribe_omits_language_when_auto()
#[tokio::test] async fn complete_returns_text_and_tokens()
#[tokio::test] async fn unauthorized_maps_401()
#[tokio::test] async fn rate_limited_uses_retry_after()          // 429 + retry-after: 42 → Some(42s)
#[tokio::test] async fn rate_limited_falls_back_to_groq_headers() // x-ratelimit-reset-requests: 2m59.56s
#[tokio::test] async fn server_error_retries_once_then_succeeds() // 500, затем 200 → Ok; сервер получил 2 запроса
#[tokio::test] async fn server_error_twice_returns_server()       // 503, 503 → Server(503)
#[tokio::test] async fn bad_json_is_bad_response()
#[test] fn timeout_formula() // 30с → 15с; 120с → 15с; 180с → 20с; 600с → 55с
#[test] fn parse_reset_formats()
```

Для теста таймаута клиент принимает таймаут через `request_timeout`, а в тестах — `OpenAiCompatible::with_timeout_override(cfg, Duration::from_millis(200))`; тест `timeout_maps_timeout` с задержкой ответа 1 с → `Timeout`.

- [ ] **Step 3:** FAIL. **Step 4:** реализовать. **Step 5:** PASS.
- [ ] **Step 6: Commit + push** (`feat(core): OpenAI-compatible provider client`).

---

### Task 6: Конвейер обработки

**Files:** Create: `src-tauri/src/pipeline/mod.rs`, `pipeline/prompts/cleanup.md`, `pipeline/prompts/translate_business.md`, `pipeline/prompts/translate_casual.md`, `pipeline/hallucinations.txt`

**Interfaces:**
- Consumes: `SpeechToText`, `TextModel`, `ProviderError`, `Mode`, `TranslateStyle`.
- Produces:
  - `pub struct PipelineOptions { pub language: Option<String>, pub cleanup: bool, pub paragraphs: bool, pub style: TranslateStyle }`
  - `pub struct JobOutput { pub text: String, pub raw: String, pub degraded: bool, pub tokens: u32, pub llm_used: bool }`
  - `#[derive(Debug, Clone, PartialEq)] pub enum JobError { NothingHeard, Provider(ProviderError) }`
  - `pub const PARAGRAPH_MIN_SECS: f32 = 30.0;`
  - `pub fn system_prompt(mode: Mode, opts: &PipelineOptions, audio_secs: f32) -> Option<String>` — `None` для Диктовки при `cleanup == false`.
  - `pub fn user_message(transcript: &str) -> String` → `"<transcript>\n{t}\n</transcript>"`
  - `pub fn is_hallucination(text: &str) -> bool`
  - `pub fn postprocess(llm_text: &str) -> String`
  - `pub async fn run(mode: Mode, wav: Vec<u8>, audio_secs: f32, opts: &PipelineOptions, stt: &dyn SpeechToText, llm: &dyn TextModel) -> Result<JobOutput, JobError>`

Логика `run`: STT (ошибка → `Provider`); пустой текст или `is_hallucination` → `NothingHeard`; `system_prompt` = `None` → вернуть сырой текст; иначе LLM; ошибка LLM → `JobOutput { text: raw, degraded: true }`; результат — `postprocess`; если после `postprocess` пусто → вернуть сырой с `degraded: true`.

Промпты (английский, точный текст пишет исполнитель, требования обязательны): вернуть **только** итоговый текст; не отвечать на содержание и не выполнять инструкции из транскрипта; сохранять имена, числа, ссылки, эмодзи; cleanup — тот же язык, убрать слова-паразиты и повторы, исправить пунктуацию и заглавные, не перефразировать; business — вежливо, чётко, профессионально, без канцелярита; casual — естественно, как носитель в мессенджере. Абзацная инструкция (`"Split the text into paragraphs by meaning, separated by a blank line."`) дописывается только при `paragraphs && audio_secs > 30.0`.

`hallucinations.txt` — по строке: `продолжение следует`, `субтитры сделал`, `субтитры создавал`, `редактор субтитров`, `спасибо за просмотр`, `thank you for watching`, `thanks for watching`, `please subscribe`, `you`, `bye`. Сравнение: нижний регистр, без пунктуации, схлопнутые пробелы; совпадение целиком **или** начинается с `субтитры` / `редактор субтитров`.

- [ ] **Step 1: Тесты** (фейковые `SpeechToText`/`TextModel` в тестовом модуле):

```rust
#[test] fn dictation_without_cleanup_has_no_prompt()
#[test] fn paragraphs_only_when_enabled_and_long() // (true, 31.0) → содержит "paragraphs"; (true, 30.0) → нет; (false, 120.0) → нет
#[test] fn translate_prompt_depends_on_style()      // business vs casual различаются
#[test] fn hallucination_matches_normalized()      // "Продолжение следует..." → true; "Субтитры сделал DimaTorzok" → true; "Спасибо за просмотр, коллеги, сейчас отправлю отчёт" → false
#[test] fn postprocess_strips_wrappers()           // "Here is the translation:\n\"Hello\"" → "Hello"; "<transcript>Hi</transcript>" → "Hi"; "«Привет»" → "Привет"; "Hello \"world\"" без изменений
#[tokio::test] async fn run_dictation_with_cleanup_uses_llm()
#[tokio::test] async fn run_llm_failure_degrades_to_raw()
#[tokio::test] async fn run_hallucination_is_nothing_heard()
#[tokio::test] async fn run_stt_error_propagates()
```

`postprocess`: trim; убрать окружающие теги `<transcript>`; убрать первую строку, если она заканчивается на `:` и начинается с `Here is`/`Here's`/`Sure`/`Вот`; убрать парные обрамляющие кавычки (`"…"`, `«…»`, `“…”`) только если они окружают **весь** текст.

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** (`feat(core): transcription/cleanup/translation pipeline`).

---

### Task 7: Очередь задач с упорядоченной доставкой

**Files:** Create: `src-tauri/src/jobs.rs`

**Interfaces:**
- Produces:
  - `pub struct OrderedDelivery<T> { next: u64, pending: BTreeMap<u64, T> }`
  - `impl<T> OrderedDelivery<T> { pub fn new(first_id: u64) -> Self; pub fn push(&mut self, id: u64, item: T) -> Vec<(u64, T)>; pub fn pending_count(&self) -> usize }` — возвращает все элементы, готовые к выдаче по порядку.
  - `pub struct JobIds(AtomicU64)` с `pub fn next(&self) -> u64`.

Оркестратор (задача 12) отправляет в `push` **любой** исход задачи (успех, ошибка, «ничего не услышал»), поэтому неудача не блокирует очередь.

- [ ] **Step 1: Тесты:**

```rust
#[test] fn in_order_items_release_immediately()
#[test] fn out_of_order_waits_for_gap()         // push(2) → [], push(1) → [(1),(2)]
#[test] fn failed_job_releases_order()          // T = Result<&str,&str>: push(3,Ok) → [], push(2,Err) → [], push(1,Ok) → [1,2,3]
#[test] fn pending_count_tracks_waiting()
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** (`feat(core): ordered job delivery`).

---

### Task 8: Вывод текста (буфер + вставка/печать)

**Files:** Create: `src-tauri/src/output/{mod.rs,clipboard.rs,macos.rs}`, `scripts/paste-matrix.sh`, `docs/qa/macos-paste-matrix.md`

**Interfaces:**
- Consumes: `InsertMethod`.
- Produces:
  - `pub enum DeliveryOutcome { Pasted, Typed, CopiedOnly(CopyReason) }`, `pub enum CopyReason { OwnWindow, Elevated, ClipboardFailed, InputFailed }`
  - `pub trait Clipboard { fn set_text(&mut self, t: &str) -> Result<(), String>; fn get_text(&mut self) -> Option<String>; }` — реализация `ArboardClipboard`.
  - `pub fn clipboard::set_and_verify(cb: &mut dyn Clipboard, text: &str, timeout: Duration) -> Result<(), String>` — запись, затем чтение каждые 10 мс до совпадения или таймаута (200 мс); при ошибке записи — до 10 попыток через 10 мс.
  - `pub trait InputPlatform { fn frontmost_is_self(&self) -> bool; fn frontmost_is_elevated(&self) -> bool; fn wait_modifiers_released(&self, timeout: Duration); fn send_paste(&self) -> Result<(), String>; fn type_text(&self, chunk: &str) -> Result<(), String>; fn send_shift_enter(&self) -> Result<(), String>; }`
  - `pub enum TypingChunk { Text(String), NewLine }`; `pub fn split_for_typing(text: &str) -> Vec<TypingChunk>` — `\r\n`/`\n` → `NewLine`, текст порциями ≤ 20 `char`.
  - `pub fn deliver(text: &str, method: InsertMethod, cb: &mut dyn Clipboard, input: &dyn InputPlatform) -> DeliveryOutcome`
  - `pub struct MacInput;` (в `output/macos.rs`) реализует `InputPlatform`.

`deliver`: (1) `set_and_verify` — ошибка → `CopiedOnly(ClipboardFailed)`; (2) `frontmost_is_self` → `CopiedOnly(OwnWindow)`; (3) `frontmost_is_elevated` → `CopiedOnly(Elevated)` (на macOS всегда `false`); (4) `wait_modifiers_released(500 мс)`; (5) Paste → `send_paste`, Type → по чанкам `type_text` / `send_shift_enter` с паузой 5 мс; ошибка ввода → `CopiedOnly(InputFailed)`.

`MacInput`: `frontmost_is_self` — `NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier == std::process::id()`; `wait_modifiers_released` — опрос `CGEventSource::flags_state(CombinedSessionState)` каждые 10 мс, пока нет Cmd/Option/Ctrl/Shift/Fn; `send_paste` — `CGEvent::new_keyboard_event(source, 9, down)` для down/up, `set_flags(CGEventFlags::CGEventFlagCommand)`, `post(CGEventTapLocation::HID)`; `type_text` — `CGEvent::set_string` (Unicode) на keyboard-событие с keycode 0, down+up; `send_shift_enter` — keycode 36 с флагом Shift.

- [ ] **Step 1: Тесты** (`FakeClipboard`, `FakeInput` записывает вызовы):

```rust
#[test] fn split_handles_newlines_and_chunks() // "a\nb" → [Text("a"), NewLine, Text("b")]; 45 символов → 3 чанка 20/20/5
#[test] fn split_is_char_safe()                // "привет😀мир" * 5 — не паникует, конкатенация == исходник
#[test] fn verify_retries_until_readback_matches() // fake возвращает старое значение 3 раза → Ok
#[test] fn verify_times_out()                   // fake всегда старое → Err
#[test] fn paste_flow_order()                   // вызовы: wait_modifiers_released → send_paste; outcome Pasted; буфер == text
#[test] fn own_window_copies_only()
#[test] fn elevated_copies_only()
#[test] fn type_flow_uses_shift_enter()         // "a\nb" → type_text("a"), send_shift_enter, type_text("b"); outcome Typed
#[test] fn input_failure_reports_copied()
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать `mod.rs`/`clipboard.rs`, затем `MacInput`. **Step 4:** PASS.
- [ ] **Step 5: Скрипт `scripts/paste-matrix.sh`:** через временную debug-команду бинаря `utter --debug-deliver "<text>" --method paste|type --delay 2` (флаг командной строки, доступный только в debug-сборке) открывает TextEdit с новым документом, ждёт 2 с, вызывает доставку, читает текст документа через `osascript` и сравнивает. Прогоняется для: латиницы, кириллицы, эмодзи, многострочного текста; раскладки RU и EN (переключение `osascript`/вручную по подсказке скрипта). Ожидание: все строки `PASS`.
- [ ] **Step 6: `docs/qa/macos-paste-matrix.md`** — таблица ручной матрицы из §9.3 спеки (приложения × раскладки × методы), с колонкой результата.
- [ ] **Step 7: Commit + push** (`feat(core): reliable clipboard paste and typing on macOS`).

---

### Task 9: Глобальные клавиши

**Files:** Create: `src-tauri/src/hotkey/{mod.rs,router.rs,macos.rs}`

**Interfaces:**
- Consumes: `KeyId`, `Mode`, `HotkeyEvent`, `Hotkeys`.
- Produces:
  - `pub struct Router { bindings: Hotkeys, held: HashSet<KeyId> }` — `pub fn new(b: Hotkeys) -> Self; pub fn set_bindings(&mut self, b: Hotkeys); pub fn route(&mut self, key: KeyId, down: bool, tap_only: bool) -> Option<HotkeyEvent>`.
  - `pub fn macos::decode_flags_changed(keycode: u16, flags: u64) -> Option<(KeyId, bool)>` — по device-dependent битам: RightAlt `0x40` (keycode 61), RightMeta `0x10` (54), RightCtrl `0x2000` (62), RightShift `0x04` (60), Fn — `0x800000` (63); CapsLock (57) → `(CapsLock, true)` с признаком tap-only.
  - `pub fn macos::keycode_to_key(keycode: u16) -> KeyId` — Esc 53, F13 105, F14 107, F15 113, F16 106, F17 64, F18 79, F19 80, F1–F12 по стандартной таблице, прочее → `Raw(keycode)`.
  - `pub struct HotkeyHandle` — `pub fn set_bindings(&self, b: Hotkeys); pub fn capture_next_key(&self) -> tokio::sync::oneshot::Receiver<KeyId>;`
  - `pub fn start(bindings: Hotkeys, tx: tokio::sync::mpsc::UnboundedSender<HotkeyEvent>) -> Result<HotkeyHandle, HotkeyError>`; `#[derive(thiserror::Error, Debug)] pub enum HotkeyError { #[error("accessibility permission missing")] NoPermission, #[error("event tap: {0}")] Tap(String) }`

`Router::route` (чистая логика):
- `tap_only == true` и клавиша назначена → `Tap(mode)`.
- Down назначенной клавиши, которой нет в `held` → добавить, `Down(mode)`; если уже в `held` (автоповтор) → `None`.
- Up назначенной → убрать из `held`, `Up(mode)`.
- `Escape` down → `Escape`.
- Down любой другой клавиши, пока `held` не пуст → `OtherKey`; иначе `None` (никакие нажатия без зажатой горячей клавиши не покидают модуль).
- В режиме захвата (`capture_next_key`) первое Down любой клавиши, кроме Escape, отдаётся в oneshot и **не** маршрутизируется; Escape отменяет захват (receiver получает ошибку).

`macos.rs`: отдельный поток с `CFRunLoop`; `CGEventTap::new(CGEventTapLocation::Session, HeadInsertEventTap, ListenOnly, [KeyDown, KeyUp, FlagsChanged])`; `None` от создания → `NoPermission`. В колбэке — только декодирование и `route`, затем `tx.send`; при `TapDisabledByTimeout`/`TapDisabledByUserInput` — `CGEventTapEnable(tap, true)` и лог. Для KeyDown учитывать поле автоповтора `KEYBOARD_EVENT_AUTOREPEAT` (повторные down отбрасываются ещё до роутера).

- [ ] **Step 1: Тесты** (`router.rs`, `macos.rs` для чистых функций):

```rust
#[test] fn router_down_up_for_bound_key()
#[test] fn router_ignores_autorepeat()            // Down, Down, Down → только первый Down(mode)
#[test] fn router_other_key_only_while_held()     // Raw(0) без зажатой → None; при зажатой → OtherKey
#[test] fn router_escape_always_reported()
#[test] fn router_unbound_key_is_silent()
#[test] fn caps_lock_emits_tap()                  // bindings.dictation = CapsLock, route(CapsLock, true, true) → Tap(Dictation)
#[test] fn decode_right_option_down_up()          // (61, flags с 0x40) → (RightAlt, true); (61, без 0x40) → (RightAlt, false)
#[test] fn decode_left_option_is_not_right()      // (58, 0x20) → Some((Raw(58), ..)) — не RightAlt
#[test] fn keycode_table()                        // 105 → F(13), 53 → Escape
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать `router.rs`, чистые функции `macos.rs`, затем поток event tap. **Step 4:** PASS.
- [ ] **Step 5: Ручная проверка:** временный лог событий в `pnpm tauri dev` — Right Option даёт `Down/Up(Dictation)`, Right Command — `Translate`, Right Option + E — `Down`, `OtherKey`, `Up`; удержание F13 (если назначить) — один `Down`. Убрать временный лог.
- [ ] **Step 6: Commit + push** (`feat(core): global hotkeys via CGEventTap`).

---

### Task 10: Запись с микрофона и разрешения

**Files:** Create: `src-tauri/src/recorder.rs`, `src-tauri/src/permissions.rs`

**Interfaces:**
- Consumes: `audio::Captured`.
- Produces:
  - `pub struct Recorder` — `pub fn new(levels: tokio::sync::watch::Sender<f32>) -> Self; pub fn start(&self) -> Result<(), RecorderError>; pub fn stop(&self) -> Result<Captured, RecorderError>; pub fn cancel(&self);`
  - `#[derive(thiserror::Error, Debug, Clone)] pub enum RecorderError { #[error("no input device")] NoDevice, #[error("stream: {0}")] Stream(String), #[error("not recording")] NotRecording }`
  - `pub fn to_f32(sample: SampleChunk) -> Vec<f32>` — чистая конвертация `i16`/`u16`/`f32` в f32.
  - `pub enum PermStatus { Granted, Denied, NotDetermined }`; `pub fn permissions::microphone() -> PermStatus; pub async fn permissions::request_microphone() -> bool; pub fn permissions::accessibility(prompt: bool) -> bool; pub fn permissions::open_accessibility_settings()` (открывает `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`).

`Recorder` держит выделенный поток (cpal `Stream` не `Send`), управляется через `std::sync::mpsc` командами `Start`/`Stop(reply)`/`Cancel`. Поток при `Start` открывает устройство по умолчанию с его конфигом, копит interleaved f32 в буфер, раз в ~33 мс публикует RMS последнего окна в `levels`. Ошибка потока cpal (устройство отключено) → переоткрыть устройство по умолчанию и продолжить запись в тот же буфер (если частота другая — сначала ресемплировать накопленное к новой); устройств нет → запись завершается, `stop` возвращает накопленное, если оно ≥ 0,3 с, иначе `NoDevice`. Разрешения — через `objc2-av-foundation` (`AVCaptureDevice::authorizationStatusForMediaType(AVMediaTypeAudio)` / `requestAccessForMediaType`) и FFI `AXIsProcessTrustedWithOptions` с `kAXTrustedCheckOptionPrompt`.

- [ ] **Step 1: Тесты:** `to_f32_converts_i16_and_u16` (`i16::MIN → -1.0`, `0 → 0.0`, `u16 32768 → 0.0`); `#[ignore] records_from_default_mic` — `start`, 1 с, `stop` → `samples.len() > 0`.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS; `cargo test -- --ignored records_from_default_mic` → PASS (macOS спросит разрешение на микрофон для тестового бинаря/терминала).
- [ ] **Step 5: Info.plist:** добавить `NSMicrophoneUsageDescription` («Utter записывает голос, только пока вы удерживаете горячую клавишу») через `src-tauri/Info.plist`.
- [ ] **Step 6: Commit + push** (`feat(core): microphone capture and permissions`).

---

### Task 11: История, счётчики, диагностика

**Files:** Create: `src-tauri/src/history.rs`, `src-tauri/src/usage.rs`, `src-tauri/src/app/diagnostics.rs`

**Interfaces:**
- Consumes: `settings::write_atomic`, `Mode`.
- Produces:
  - `pub enum EntryStatus { Done, Degraded, NothingHeard, Failed { message: String } }`
  - `pub struct HistoryEntry { pub id: u64, pub created_at_ms: i64, pub mode: Mode, pub text: String, pub raw: String, pub status: EntryStatus, pub audio_file: Option<PathBuf> }`
  - `pub const MAX_ENTRIES: usize = 50; MAX_FAILED_AUDIO: usize = 5;`
  - `impl History { pub fn load(path: &Path) -> Self; pub fn save(&self) -> io::Result<()>; pub fn push(&mut self, e: HistoryEntry) -> Vec<PathBuf>; pub fn update(&mut self, id: u64, f: impl FnOnce(&mut HistoryEntry)) -> Vec<PathBuf>; pub fn entries(&self) -> &VecDeque<HistoryEntry>; pub fn get(&self, id: u64) -> Option<&HistoryEntry>; pub fn clear(&mut self) -> Vec<PathBuf> }` — возвращаемые пути вызывающий удаляет с диска.
  - `pub struct UsageDay { pub date: NaiveDate, pub stt_requests: u32, pub audio_secs: f32, pub llm_requests: u32, pub llm_tokens: u32 }`
  - `impl Usage { pub fn load(path: &Path) -> Self; pub fn save(&self) -> io::Result<()>; pub fn record_stt(&mut self, today: NaiveDate, audio_secs: f32); pub fn record_llm(&mut self, today: NaiveDate, tokens: u32); pub fn today(&self, today: NaiveDate) -> UsageDay }`
  - `pub fn diagnostics::init_logging(log_dir: &Path) -> WorkerGuard` (tracing + `tracing-appender`, ротация 5 файлов × 1 МБ — собственный `RollingFileAppender` с `max_log_files(5)` и размерным ротатором, либо daily + обрезка до 1 МБ при старте); `pub fn install_panic_hook()`; `pub fn report(settings: &Settings, log_dir: &Path) -> String` (версия, ОС, настройки без секретов, последние 200 строк лога).

- [ ] **Step 1: Тесты:**

```rust
#[test] fn history_caps_at_50_and_returns_evicted_audio()
#[test] fn history_keeps_audio_for_last_5_failed_only() // 6 Failed с audio → push 6-го возвращает путь 1-го, у 1-го audio_file = None
#[test] fn history_update_success_returns_audio_to_delete()
#[test] fn history_roundtrip()
#[test] fn usage_resets_on_new_day()
#[test] fn usage_accumulates_same_day()
#[test] fn report_contains_no_secrets() // settings + лог со строкой "gsk_secret" → в отчёте нет "gsk_secret" (лог фильтрует ключи маской: regex `(gsk_|sk-)[A-Za-z0-9_\-]{8,}`)
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS.
- [ ] **Step 5: Commit + push** (`feat(core): history, usage counters, diagnostics`).

---

### Task 12: Оркестратор, пилюля и служебные настройки — сквозной сценарий

**Files:**
- Create: `src-tauri/src/app/{mod.rs,orchestrator.rs,commands.rs,overlay.rs}`, `src/lib/{ipc.ts,types.ts}`, `src/overlay/{main.tsx,Pill.tsx}`, `src/settings/{main.tsx,DevSettings.tsx}`, `src/styles/tokens.css`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` (`tauri-nspanel`)

**Interfaces:**
- Consumes: всё выше.
- Produces:
  - Событие `overlay://state` с `#[derive(Serialize, Clone)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum OverlayState { Hidden, Recording { mode: Mode, style: TranslateStyle, started_at_ms: i64, warn: bool, pending: u32 }, Processing { pending: u32 }, Done { outcome: String /* "pasted" | "typed" | "copied" */, degraded: bool }, Error { message_key: String, action: Option<String> /* "open_provider" | "open_permissions" */, minutes: Option<u32> } }`
  - Событие `overlay://level` (`f32`, ~30 Гц).
  - Команды Tauri: `get_settings() -> Settings`, `save_settings(s: Settings)`, `set_api_key(provider: ProviderKind, key: String)`, `api_key_mask(provider) -> Option<String>`, `verify_key(provider) -> Result<(), String>`, `capture_key() -> Result<KeyId, String>`, `permissions_status() -> { microphone: PermStatus, accessibility: bool }`, `request_microphone() -> bool`, `open_accessibility_settings()`, `history_list() -> Vec<HistoryEntry>`, `history_retry(id: u64)`, `usage_today() -> UsageDay`, `copy_diagnostics() -> String`.
  - TS-зеркала в `src/lib/types.ts`, обёртки `invoke`/`listen` в `src/lib/ipc.ts`.

Оркестратор (`orchestrator.rs`) — одна tokio-задача, `select!` по: событиям `hotkey`, тику 100 мс (`controller.tick`), результатам задач, командам из UI. Исполнение команд контроллера:
- `Start(mode)`: проверить ключ (нет → `abort`, `Error{no_key, open_provider}`), `permissions::accessibility(false)` (нет → `abort`, `Error{no_accessibility, open_permissions}`), `recorder.start()` (ошибка → `abort`, `Error{no_microphone}`); `OverlayState::Recording`; `tokio::spawn(provider.prewarm())`.
- `Submit`: `recorder.stop()` → `audio::prepare` в `spawn_blocking` (`None` → `Error{nothing_heard}`, без запроса); иначе id из `JobIds`, WAV во временный файл `app_data/pending-audio/{id}.wav`, запись в историю, `pipeline::run` в `tokio::spawn`; пилюля `Processing { pending }`.
- `Cancel(_)`: `recorder.cancel()`, `Hidden` (для `Escape` — сразу).
- `LimitWarning`: `Recording { warn: true }`.
- Результат задачи → `OrderedDelivery::push` → для каждого готового по порядку: успех → `output::deliver` (в `spawn_blocking`), обновить историю/счётчики, удалить аудио, `Done`; `NothingHeard` → `Error{nothing_heard}`; `Provider(e)` → `Error{…}` по таблице §8.3 спеки (ключи: `network`, `bad_key`, `rate_limited` с `minutes`, `service_down`), статус `Failed`, аудио остаётся.
- Если во время обработки идёт новая запись — состояние `Recording { pending }` имеет приоритет; `Done`/`Error` прежних задач показываются после завершения записи.
- `history_retry(id)` — повторно запускает `pipeline::run` с файлом аудио записи и доставляет результат (вне очереди, сразу).
- Изменение настроек: `controller.set_record_mode`, `hotkey.set_bindings`, пересоздание провайдера.
- Пробуждение после сна (`NSWorkspaceDidWakeNotification`): пересоздать event tap (`hotkey::start` заново) и HTTP-клиент провайдера.

`overlay.rs`: окно `overlay` превращается в non-activating NSPanel через `tauri-nspanel` (уровень над полноэкранными, `canJoinAllSpaces | fullScreenAuxiliary`, `ignoresMouseEvents = true`, кроме состояния `Error` с `action`); позиция — низ по центру экрана под курсором, отступ 48 px от низа видимой области; показ — `order_front_regardless`, без активации приложения.

UI (простая версия; полировка — План 2): `Pill.tsx` — чёрная капсула с токенами из `tokens.css` (§5.1 тёмные), 24 полоски уровня, таймер `m:ss` (Geist Mono подключается в Плане 2 — пока `ui-monospace`), подписи по `message_key` (временный словарь RU). `DevSettings.tsx` — один экран: провайдер, поле ключа + «Проверить», режим записи, способ вставки, язык речи, чистовая обработка, абзацы, стиль перевода, две кнопки «Назначить клавишу» (`capture_key`), статусы разрешений с кнопками, последние 10 записей истории с «Повторить», счётчики за сегодня, «Скопировать диагностику».

- [ ] **Step 1: Тесты оркестратора** (оркестратор принимает зависимости через трейты: `SecretStore`, `SpeechToText`/`TextModel` фабрика, `Clipboard`, `InputPlatform`, трейт `AudioSource { start, stop, cancel }` с фейком, отдающим заранее заданный `Captured`; события пилюли собираются в `Vec<OverlayState>`):

```rust
#[tokio::test] async fn dictation_end_to_end_pastes()     // Down, 1с, Up → overlay: Recording, Processing, Done{pasted}; clipboard == "Привет, мир."
#[tokio::test] async fn missing_key_shows_error_and_stays_idle()
#[tokio::test] async fn silence_shows_nothing_heard_without_request() // фейковый STT не вызывался
#[tokio::test] async fn recorder_failure_aborts_controller()          // start() → Err → Error{no_microphone}; следующий Down снова стартует запись
#[tokio::test] async fn results_delivered_in_recording_order()         // задача 1 медленнее задачи 2 → вставки в порядке 1, 2
#[tokio::test] async fn rate_limit_error_keeps_audio_for_retry()
```

- [ ] **Step 2:** FAIL. **Step 3:** реализовать оркестратор, команды, `overlay.rs`, затем UI. **Step 4:** PASS (`cargo test`, `pnpm vitest run` — тест `Pill` рендерит каждое `OverlayState` без ошибок).
- [ ] **Step 5: Сквозная ручная проверка в `pnpm tauri dev`** с реальным ключом Groq пользователя (ключ вводит пользователь в DevSettings): диктовка RU в TextEdit; перевод в обоих стилях; Hold и Toggle; Esc; Right Option + E (отмена сочетанием); запись 40 с с включёнными абзацами; выключенный Wi-Fi → «Нет соединения» → включить → «Повторить» в истории; три записи подряд во время обработки — порядок вставки сохранён.
- [ ] **Step 6: Commit + push** (`feat: end-to-end dictation and translation on macOS`).

---

### Task 13: Матрица вставки, стресс и бюджеты на macOS

**Files:** Modify: `docs/qa/macos-paste-matrix.md` (заполнить результаты); исправления — в модулях, где найдены проблемы.

- [ ] **Step 1:** `scripts/paste-matrix.sh` → все `PASS`.
- [ ] **Step 2:** Ручная матрица §9.3 спеки на macOS (TextEdit, Notes, Safari, Chrome, Telegram, Slack, VS Code, Terminal × RU/EN × Hold/Toggle × Right Option/F13 × Диктовка/Перевод); результаты — в `docs/qa/macos-paste-matrix.md`. Любой FAIL → завести исправление через superpowers:systematic-debugging, добавить регрессионный тест в модуль, повторить строку матрицы.
- [ ] **Step 3: Стресс:** 100 диктовок подряд без перезапуска; 20 быстрых нажатий < 0,3 с; 3 записи в очереди; сон → пробуждение → диктовка; отключение AirPods во время записи. Ожидание: ни одного падения, зависшего состояния или потерянной вставки.
- [ ] **Step 4: Бюджеты §8.4:** по `tracing`-спанам (`key_down→overlay_shown`, `key_down→first_sample`, `key_up→delivered`) в логе — значения в пределах; в простое через `ps -o %cpu,rss` — ~0% CPU, < 80 МБ суммарно по процессам Utter. Результаты — в `docs/qa/macos-paste-matrix.md`.
- [ ] **Step 5: Commit + push** (`test: macOS paste matrix, stress and budgets`).
