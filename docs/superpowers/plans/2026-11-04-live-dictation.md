# Диктовка на лету — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** с провайдером Gemini черновик появляется прямо в поле во время речи (предварительный текст через собственный способ ввода Utter) и утверждается чистовиком через ≤ 0,8 с после отпускания; при любом сбое — нынешний пакетный путь.

**Architecture:** Rust открывает WebSocket к `gemini-3.5-transcribe-live` в момент нажатия и раз в 100 мс досылает звук из `recorder.peek`; чистая логика `live/draft.rs` собирает черновик и чистовик; `output/ime` переключает раскладку на способ ввода «Utter» (Swift, InputMethodKit) и передаёт ему черновик/чистовик по Unix-сокету с проверкой подписи собеседника. Оркестратор выбирает путь «на лету» или пакетный и при сбое переходит на пакетный с уже записанным звуком.

**Tech Stack:** Rust (Tauri 2, tokio, `tokio-tungstenite` 0.24 с rustls), Swift 5 / InputMethodKit, Carbon TIS (FFI), Security.framework (FFI), React/i18n для текстов ошибок.

**Spec:** `docs/superpowers/specs/2026-11-04-live-dictation-design.md` (+ ссылки из `2026-10-06-utter-design.md` §3.4, §7, §8.4).

## Global Constraints

- Ветка `feat/premium-ui`; коммит + `git push` после каждой задачи; сообщения коммитов заканчиваются `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; TDD.
- Адреса и модели — только в `providers/defaults.rs`: `GEMINI_LIVE_URL = "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"`, `GEMINI_LIVE_MODEL = "gemini-3.5-transcribe-live"`, `GEMINI_TRANSLATE_MODEL` (значение — Task 3).
- Звук в Live API: PCM 16 бит, 16 000 Гц, моно, little-endian, куски по 1 600 отсчётов (100 мс), `mimeType: "audio/pcm;rate=16000"`.
- Режим распознавания: `cleanup = true` → `"SMART"`, иначе `"VERBATIM"`; `languageCodes` = `[<язык речи>-<РЕГИОН>]` (ru → `ru-RU`, en → `en-US`, иначе `<код>`), при «Авто» — `[]`.
- Тайм-ауты: соединение + `setupComplete` ≤ 3 с; после отпускания чистовика ждём ≤ 1,5 с; подключение поля после F19 ≤ 0,3 с; перевод хвоста после отпускания ≤ 3 с.
- Способ ввода: bundle id и `TISInputSourceID` `app.utter.desktop.inputmethod`, `InputMethodConnectionName` `app.utter.desktop.inputmethod_Connection`, имя «Utter», «пробуждение» — F19 (keyCode 80), черновик — только `NSUnderlineStyle.single` без цвета.
- Канал: `~/Library/Application Support/app.utter.desktop/im.sock`, папка 0700, сокет 0600, строки JSON; собеседник проверяется по подписи (identifier + тот же лист-сертификат).
- Ключ API никогда не попадает в лог: адрес соединения не логируется ни при каких ошибках.
- Свои синтетические нажатия — с `hotkey::macos::SYNTHETIC_MARK`.
- `cargo clippy --all-targets` — 0; `pnpm test`, `cargo test --lib` зелёные; `#[ignore]`-тесты с ключом — только с согласия пользователя.
- Раскладка пользователя возвращается при **каждом** исходе диктовки и при старте Utter.

## Review Focus

1. Новая запись, пока предыдущая ещё обрабатывается (очередь не пуста) → порядок вставки сохраняется: такая запись идёт пакетным путём (тест `live_waits_for_empty_queue`, Task 7).
2. Esc посреди диктовки → черновик стёрт, раскладка возвращена, ничего не вставлено (тест `cancel_clears_draft_and_restores_layout`, Task 6).
3. Долгая диктовка (несколько минут, 30+ кусков) в Переводе → не упираемся в бесплатный лимит запросов: переводы объединяются, не чаще раза в 4 с (тест `translation_coalesces_segments`, Task 3).
4. Соединение открылось, но Google ничего не прислал (как 2026-11-04) → после отпускания не позже 1,5 с срабатывает пакетный путь, а не вечное ожидание (тест `silent_server_falls_back`, Task 6).
5. Utter упал или был закрыт посреди диктовки → при следующем запуске раскладка пользователя возвращена, а не остаётся «Utter» (тест `startup_restores_saved_layout`, Task 5).

---

### Task 1: Протокол Live API и звук для него (чистая логика)

**Files:**
- Create: `src-tauri/src/providers/gemini_live.rs` (только чистые функции в этой задаче), `src-tauri/src/live/mod.rs`, `src-tauri/src/live/pcm.rs`
- Modify: `src-tauri/src/providers/mod.rs` (`pub mod gemini_live;`), `src-tauri/src/providers/defaults.rs` (`GEMINI_LIVE_URL`, `GEMINI_LIVE_MODEL`), `src-tauri/src/lib.rs` (`mod live;`)
- Test: в тех же файлах (`mod tests`); фикстуры — `src-tauri/tests-data/live-*.json` (сообщения сервера из проверки 2026-11-04: `setupComplete`, `interimInputTranscription`, `inputTranscription`, `turnComplete`, ошибка)

**Interfaces:**
- Produces:
  - `pub enum TranscribeMode { Smart, Verbatim }` (`gemini_live.rs`)
  - `pub fn setup_message(model: &str, mode: TranscribeMode, language: Option<&str>) -> serde_json::Value`
  - `pub fn audio_message(pcm: &[i16]) -> serde_json::Value`, `pub fn end_message() -> serde_json::Value`
  - `#[derive(Debug, Clone, PartialEq)] pub enum LiveEvent { Ready, Interim(String), Segment(String), TurnComplete, Error(String) }`
  - `pub fn parse_server(text: &str) -> Vec<LiveEvent>` — одно сообщение сервера может нести несколько полей
  - `pub fn language_code(lang: Option<&str>) -> Option<String>` — `ru`→`ru-RU`, `en`→`en-US`, прочие — как есть, `None` при «Авто»
  - `pub struct Chunker` (`live/pcm.rs`): `pub fn new(rate: u32, channels: u16) -> Self`, `pub fn push(&mut self, interleaved: &[f32]) -> Vec<Vec<i16>>` — даунмикс + ресемплинг (`audio::dsp::downmix`, `audio::dsp::resample`) до 16 кГц и нарезка по 1 600 отсчётов, остаток хранится; `pub fn flush(&mut self) -> Option<Vec<i16>>`

- [ ] **Step 1: Failing tests:** `setup_smart_ru` (JSON равен `{"setup":{"model":"models/gemini-3.5-transcribe-live","generationConfig":{"responseModalities":["TEXT"]},"inputAudioTranscription":{"mode":"SMART","languageCodes":["ru-RU"]}}}`), `setup_auto_language_is_empty_list`, `audio_message_is_base64_le_pcm` (2 отсчёта `[1, -2]` → base64 `AQD+/w==`), `parse_interim_and_final_fixtures` (по фикстурам), `parse_ignores_unknown_fields`, `parse_error_message`, `language_codes`, `chunker_48k_stereo_makes_100ms_chunks` (0,35 с стерео 48 кГц → 3 куска по 1 600, `flush` → 800 отсчётов), `chunker_keeps_remainder_between_pushes`.
- [ ] **Step 2:** `cargo test --manifest-path src-tauri/Cargo.toml --lib gemini_live pcm` → FAIL (нет модулей).
- [ ] **Step 3:** реализовать по Interfaces (`serde_json::json!`, `base64::STANDARD`).
- [ ] **Step 4:** тесты → PASS, clippy → 0.
- [ ] **Step 5:** commit `feat(live): Live API messages and 16 kHz audio chunks` + push.

---

### Task 2: Потоковая сессия по WebSocket

**Files:**
- Modify: `src-tauri/src/providers/gemini_live.rs` (сессия), `src-tauri/Cargo.toml` (`tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }`, `futures-util = "0.3"`)
- Test: `gemini_live.rs` `mod tests` (локальный сервер на `tokio-tungstenite::accept_async` поверх `TcpListener` 127.0.0.1:0)

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `pub struct LiveConfig { pub url: String, pub key: String, pub model: String, pub mode: TranscribeMode, pub language: Option<String> }`
  - `pub struct LiveSession { pub audio: tokio::sync::mpsc::UnboundedSender<Vec<i16>>, pub events: tokio::sync::mpsc::UnboundedReceiver<LiveEvent> }`; `impl LiveSession { pub fn finish(&self) }` — посылает `end_message` после всего звука в очереди.
  - `pub async fn connect(cfg: LiveConfig) -> Result<LiveSession, ProviderError>` — соединение с ключом в заголовке `x-goog-api-key` (если Task 3 покажет, что сервер его не принимает, — параметр `key`, адрес не логируется); `setup` первым; звук, пришедший в `audio` до `Ready`, копится и уходит сразу после `setupComplete`; ≤ 3 с на `Ready`, иначе `ProviderError::Timeout`; закрытие сервером → `LiveEvent::Error`.
  - `#[async_trait] pub trait LiveConnector: Send + Sync { async fn connect(&self, cfg: LiveConfig) -> Result<LiveSession, ProviderError>; }` и `pub struct GeminiLive;` — реальная реализация.

- [ ] **Step 1: Failing tests (поддельный сервер):** `sends_setup_first_then_audio`, `audio_before_ready_is_held_and_flushed` (сервер отвечает `setupComplete` через 200 мс; все куски дошли по порядку), `events_arrive_in_order` (сервер шлёт interim, interim, final → те же события), `finish_sends_stream_end_after_audio`, `no_setup_complete_times_out` (≤ 3 с → `Timeout`), `server_close_is_error_event`, `key_is_sent_in_header_not_url` (сервер видит заголовок, путь без `key=`).
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Живая проверка (`#[ignore]`, с согласия пользователя):** `live_phrase_latency` — `tests-data/phrase-ru.wav` в реальном темпе через `GeminiLive`; печатает первый `Interim`, `release→Segment`; ожидание: ≤ 1 с и ≤ 0,8 с; если заголовок не принят — записать в журнал правило и перейти на параметр.
- [ ] **Step 6:** commit `feat(live): streaming session over WebSocket` + push.

---

### Task 3: Сборщик черновика и перевод по кускам

**Files:**
- Create: `src-tauri/src/live/draft.rs`, `src-tauri/src/live/translate.rs`
- Modify: `src-tauri/src/providers/defaults.rs` (`GEMINI_TRANSLATE_MODEL`), `src-tauri/src/providers/bench.rs` (живой замер моделей перевода)

**Interfaces:**
- Produces (`draft.rs`, чистая логика):
  - `#[derive(Default)] pub struct DraftComposer`; `pub fn on_interim(&mut self, text: &str)`, `pub fn on_segment(&mut self, text: &str)`, `pub fn draft(&self) -> String`, `pub fn final_text(&self) -> String`, `pub fn segments(&self) -> &[String]`.
  - Правило: `on_segment` фиксирует последний черновой текст этого куска в черновике (чистовик в черновик не подставляется); `draft()` = зафиксированные + текущий черновой, через пробел; `final_text()` = чистовые куски + хвостовой черновой, если для него чистовика нет.
  - `pub struct DraftThrottle` — `pub fn due(&mut self, now: Instant, draft: &str) -> bool`: `true` не чаще раза в 100 мс и только при изменении текста.
- Produces (`translate.rs`):
  - `pub struct SegmentTranslator` — `pub fn new(provider: Arc<dyn TextModel>, system: String) -> Self`, `pub fn push(&mut self, segment: String, now: Instant)`, `pub fn poll(&mut self, now: Instant)` (запускает перевод накопленных непереведённых кусков, если в полёте нет запроса и прошло ≥ 4 с с прошлого запуска; запрос без ответа 3 с бросается, его куски возвращаются в очередь), `pub async fn finish(self) -> Result<String, ProviderError>` (переводит остаток сразу, ждёт ≤ 3 с, склеивает по порядку). Системный промпт — `pipeline::system_prompt(Mode::Translate, …)`, текст — `pipeline::user_message`.
- Отличие от спецификации §5.6 (там «каждый кусок сразу, параллельно»): переводы объединяются и идут не чаще раза в 4 с — у моделей бесплатные лимиты 5–15 запросов в минуту (2026-11-04: `gemini-3.6-flash` вернул 429 на 6-м запросе за минуту), а в долгой диктовке кусков 30+. Записать как `Ruling` и поправить §5.6 в Task 8.
- `GEMINI_TRANSLATE_MODEL` — модель с наименьшей медианой в Step 6 среди `gemini-3.5-flash-lite`, `gemini-3.6-flash`, `gemini-3.7-flash`, `gemini-3.8-flash`, у которой бесплатный лимит ≥ 10 запросов в минуту (ответ 429 сообщает лимит).

- [ ] **Step 1: Failing tests (`draft.rs`):** `draft_grows_with_interims`, `segment_freezes_its_draft_not_its_final` (interim «по продажам», segment «по продам» → `draft()` содержит «по продажам», `final_text()` — «по продам»), `final_includes_tail_without_segment`, `throttle_limits_to_100ms_and_changes`.
- [ ] **Step 2: Failing tests (`translate.rs`, фейковый `TextModel` с задержками):** `translations_join_in_order` (второй ответ приходит раньше первого → порядок сохранён), `translation_coalesces_segments` (10 кусков за 20 с → не больше 6 запросов, каждый следующий ≥ 4 с после предыдущего), `finish_translates_rest_at_once`, `finish_gives_up_after_3s` (→ `Err(Timeout)`), `slow_translation_is_retried_with_next` (запрос без ответа 3 с бросается, его куски уходят в следующий вместе с новыми; порядок сохранён).
- [ ] **Step 3:** FAIL. **Step 4:** реализовать. **Step 5:** PASS, clippy 0.
- [ ] **Step 6: Живой замер (`#[ignore]`, с согласия):** `translate_models_latency` в `bench.rs` — 5 коротких русских фраз × 4 модели, медиана и лимит из 429; выбрать модель по правилу выше, записать в `defaults.rs` с комментарием-цифрами.
- [ ] **Step 7:** commit `feat(live): draft composer and coalesced translation` + push.

---

### Task 4: Способ ввода Utter (Swift) и его сборка

**Files:**
- Create: `macos/UtterIM/Sources/main.swift`, `macos/UtterIM/Sources/Controller.swift`, `macos/UtterIM/Sources/Channel.swift`, `macos/UtterIM/Sources/PeerCheck.swift`, `macos/UtterIM/Info.plist`, `scripts/build-im.sh`
- Modify: `scripts/dev-bundle.sh` (сначала `scripts/build-im.sh`; внешний бандл подписывать **без** `--deep`, затем `codesign --verify --deep --strict`), `src-tauri/tauri.conf.json` (`bundle.macOS.files`: `"Resources/UtterIM.app": "../macos/UtterIM/build/UtterIM.app"`), `.gitignore` (`macos/UtterIM/build/`)

**Interfaces:**
- Produces (протокол канала, общий с Task 5; одна JSON-строка на сообщение, `\n`):
  - Utter → способ ввода: `{"t":"draft","text":…}`, `{"t":"commit","text":…}`, `{"t":"clear"}`
  - способ ввода → Utter: `{"t":"hello","version":<int>}`, `{"t":"active","bundle":…}`, `{"t":"inactive"}`
- Поведение — спецификация §5.3: `handle` пропускает клавиши, глотает keyCode 80; `setMarkedText` с `[.underlineStyle: single]`; `commit` → `insertText`; `clear`/`commitComposition` → `setMarkedText("")`; при `activateServer` применяет последний полученный черновик.
- `Channel.swift`: клиент к сокету из Global Constraints, переподключение раз в 1 с, если Utter не запущен; **перед первым сообщением** — проверка подписи сервера (`PeerCheck.swift`: `LOCAL_PEERTOKEN` → `SecCodeCopyGuestWithAttributes(kSecGuestAttributeAudit)` → `SecCodeCheckValidity` с требованием `identifier "app.utter.desktop" and certificate leaf = H"<sha1 листа своего сертификата>"`); при несовпадении — закрыть и не принимать сообщения.
- `scripts/build-im.sh`: `swiftc -O` → `macos/UtterIM/build/UtterIM.app`, подпись `codesign --sign "Utter Dev" --identifier app.utter.desktop.inputmethod`.

- [ ] **Step 1:** `scripts/build-im.sh` собирает и подписывает; `codesign -dv` показывает `Identifier=app.utter.desktop.inputmethod`, `Authority=Utter Dev`.
- [ ] **Step 2:** `scripts/dev-bundle.sh` → в `Utter.app/Contents/Resources/UtterIM.app` лежит способ ввода; `codesign --verify --deep --strict Utter.app` → OK; у внешнего бандла по-прежнему `Identifier=app.utter.desktop`.
- [ ] **Step 3: Ручная проверка канала** (временный Rust-тест `#[ignore] im_channel_echo` из Task 5 или `nc -U`, когда сервер готов): способ ввода, запущенный вручную, присылает `hello`; посторонний процесс-сервер (например, `nc -lU`) отвергается — в логе способа ввода `peer rejected`.
- [ ] **Step 4:** commit `feat(ime): Utter input method (marked-text draft) and its build` + push.

---

### Task 5: Канал, установка способа ввода и переключение раскладки (Rust, macOS)

**Files:**
- Create: `src-tauri/src/output/ime/mod.rs`, `src-tauri/src/output/ime/channel.rs`, `src-tauri/src/output/ime/tis.rs`, `src-tauri/src/output/ime/install.rs`, `src-tauri/src/output/ime/peer.rs`
- Modify: `src-tauri/src/output/mod.rs` (`#[cfg(target_os = "macos")] pub mod ime;`), `src-tauri/src/lib.rs` (установка + возврат раскладки при старте), (раскладка пользователя на время диктовки хранится в файле `<data dir>/input-source` — одна строка с id источника; файл удаляется в `end`)

**Interfaces:**
- Consumes: протокол Task 4.
- Produces:
  - `pub trait DraftField: Send + Sync { fn begin(&self) -> bool; fn draft(&self, text: &str); fn commit(&self, text: &str) -> bool; fn clear(&self); fn end(&self); fn focus_changed(&self) -> bool; }` (`output/ime/mod.rs`; на не-macOS — `NoDraftField`, где `begin` → `false`)
  - `begin`: запомнить текущий источник (`tis::current_id()`, записать в `<data dir>/input-source`), `tis::select(IM_ID)`, послать F19 с `SYNTHETIC_MARK`, ждать `active` ≤ 0,3 с → `true`/`false`; `commit` → `false`, если после `begin` пришёл `inactive` или `active` другой программы; `end` — вернуть сохранённый источник и удалить файл (вызывается всегда); `focus_changed` — тот же признак, что у `commit`.
  - `channel.rs`: `pub fn encode(msg: &ToIm) -> String`, `pub fn decode(line: &str) -> Option<FromIm>` (enum'ы по протоколу Task 4); сервер на `tokio::net::UnixListener`, права 0700/0600, один клиент, проверка подписи клиента (`peer.rs`).
  - `peer.rs`: `pub fn requirement(identifier: &str, leaf_sha1_hex: &str) -> String` (→ `identifier "app.utter.desktop.inputmethod" and certificate leaf = H"…"`), `pub fn own_leaf_sha1() -> Option<String>`, `pub fn check_peer(fd: RawFd, requirement: &str) -> bool`.
  - `tis.rs`: `pub fn current_id() -> Option<String>`, `pub fn select(id: &str) -> bool`, `pub fn register(bundle: &Path) -> bool`, `pub fn enable(id: &str) -> bool`, `pub fn is_enabled(id: &str) -> bool`.
  - `install.rs`: `pub fn needs_install(bundled: &Path, installed: &Path) -> bool` (нет установленного или `CFBundleVersion` меньше), `pub fn install(app: &AppHandle) -> Result<(), String>` (копия в `~/Library/Input Methods/`, `register`, `enable`, «разминка»: `select` → F19 → вернуть).
  - `pub fn restore_after_crash(data_dir: &Path, tis: &dyn Tis)` — при старте Utter: если файл `input-source` есть, а текущий источник — `IM_ID`, выбрать сохранённый; файл удалить.

- [ ] **Step 1: Failing tests:** `encode_decode_roundtrip` (все сообщения), `decode_rejects_garbage`, `requirement_string`, `needs_install_by_version` (нет / старше / та же), `startup_restores_saved_layout` (фейковый TIS: текущий = `IM_ID`, в файле `com.apple.keylayout.RussianWin` → выбран он, файл удалён), `end_always_restores` (фейковый TIS и канал: `begin` → ошибка канала → `end` всё равно возвращает источник), `commit_fails_after_focus_moved` (`active notion.id` → `active com.tdesktop.Telegram` → `commit` → `false`), `socket_permissions` (папка 0700, сокет 0600).
- [ ] **Step 2:** FAIL. **Step 3:** реализовать (TIS и Security — `extern "C"` к `Carbon`/`Security`; фейки — через трейт `Tis` внутри модуля). **Step 4:** PASS, clippy 0.
- [ ] **Step 5: Ручная проверка с собранным Utter:** после старта в меню раскладок есть «Utter»; `#[ignore] im_draft_live` (с согласия) — `begin`, `draft("проверка")`, `commit("Проверка.")`, `end` в TextEdit: виден подчёркнутый черновик, затем текст; раскладка вернулась.
- [ ] **Step 6:** commit `feat(ime): channel, install and input source switching` + push.

---

### Task 6: Диктовка на лету — сборка частей

**Files:**
- Modify: `src-tauri/src/live/mod.rs`
- Test: `src-tauri/src/live/mod.rs` `mod tests` (фейковые `LiveConnector`, `DraftField`, `TextModel`)

**Interfaces:**
- Consumes: `LiveConnector`, `LiveConfig`, `LiveEvent` (Task 2), `DraftComposer`, `DraftThrottle`, `SegmentTranslator` (Task 3), `DraftField` (Task 5), `Chunker` (Task 1).
- Produces:
  - `pub struct LiveRequest { pub mode: Mode, pub cfg: LiveConfig, pub translate: Option<(Arc<dyn TextModel>, String)>, pub rate: u32, pub channels: u16 }`
  - `pub enum LiveOutcome { Committed { text: String, requests: u32 }, NotCommitted { text: String, requests: u32, focus_moved: bool }, Fallback { reason: String } }` — `NotCommitted`: текст готов, но в поле не утверждён: `focus_moved = false` — поле не подключилось (оркестратор вставит обычной доставкой), `true` — фокус ушёл посреди диктовки (только в буфер обмена, §4.3); `Fallback`: пакетный путь.
  - `pub struct LiveHandle` — `pub fn audio(&self, interleaved: &[f32])`, `pub fn release(&self)`, `pub fn cancel(&self)`; `pub fn spawn(req: LiveRequest, connector: Arc<dyn LiveConnector>, field: Arc<dyn DraftField>, done: impl FnOnce(LiveOutcome) + Send + 'static) -> LiveHandle`.
  - Поведение — таблица §4.3 спецификации: `field.begin()` при старте (`false` → черновик не показывается, но распознавание идёт); черновик → `field.draft` по `DraftThrottle`; `release` → `finish()`, ждать `TurnComplete` или 1,5 с; Диктовка: чистовик = `final_text()`; Перевод: `SegmentTranslator::finish`; `field.commit` (`false` или `begin` был `false` → `NotCommitted`); `field.end()` при **любом** исходе; ошибка соединения/429/`Error` до чистовика → `field.clear()`, `Fallback`.
  - Лог: `timing: live setup_ms`, `timing: live first interim_ms`, `timing: live release→final_ms`, `timing: live release→committed_ms`, `live fallback reason=…`.

- [ ] **Step 1: Failing tests:** `dictation_drafts_then_commits_final` (события interim×3, segment, release → `draft` вызывался, `commit` с `final_text`, `Committed`), `translate_commits_english` (фейковый перевод → `commit("Hello…")`), `cancel_clears_draft_and_restores_layout` (`cancel` → `clear`, `end`, никакого `commit`, `done` не вызывается с текстом), `connect_failure_falls_back` (→ `Fallback`, `end` вызван), `silent_server_falls_back` (соединение есть, событий нет; после `release` ≤ 1,5 с → `Fallback`), `server_error_mid_speech_clears_and_falls_back`, `focus_moved_gives_not_committed` (`focus_moved: true`), `no_field_still_returns_text` (`begin` → `false` → `NotCommitted { focus_moved: false }` с текстом), `audio_is_chunked_and_streamed`.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать (одна задача tokio на диктовку; `select!` по событиям, звуку, `release`/`cancel`, тайм-аутам). **Step 4:** PASS, clippy 0.
- [ ] **Step 5:** commit `feat(live): live dictation from first word to committed final` + push.

---

### Task 7: Оркестратор, честные ошибки и тексты

**Files:**
- Modify: `src-tauri/src/app/orchestrator.rs` (новые зависимости, выбор пути, `Input::LiveDone`), `src-tauri/src/lib.rs` (реальные `GeminiLive` и `ImeField` в `Deps`), `src/i18n/ru.json`, `src/i18n/en.json`
- Test: `orchestrator.rs` `mod tests` (существующий `rig` + фейковые `LiveConnector` и `DraftField`)

**Interfaces:**
- Consumes: `LiveHandle`, `LiveOutcome`, `spawn` (Task 6), `DraftField` (Task 5), `GeminiLive` (Task 2).
- Produces:
  - `Deps` + `pub live: Arc<dyn LiveConnector>`, `pub draft: Arc<dyn DraftField>`; `Input::LiveDone(Box<LiveOutcome>)`.
  - `Deliverer` + `fn copy_only(&self, text: &str) -> DeliveryOutcome` (кладёт в буфер обмена, возвращает `DeliveryOutcome::Copied`; реализация в `app/services.rs` рядом с `deliver`).
  - Выбор пути в `start`: «на лету», если провайдер — Gemini, очередь пуста (`in_flight == 0`) и ОС — macOS; иначе — как сейчас. При «на лету» нарезка `check_pieces` не работает (весь звук и так уходит потоком).
  - `tick`: новые отсчёты `audio.peek(offset)` → `LiveHandle::audio`.
  - `submit`: запись останавливается как сейчас (звук сохраняется для запасного пути и истории), `release()`; `LiveDone`: `Committed` → история/счётчики/«Готово» без вызова `output.deliver`; `NotCommitted { focus_moved: false }` → `output.deliver(text)` (исход — «Вставлено»/«Скопировано», как сейчас); `NotCommitted { focus_moved: true }` → `output.copy_only(text)` → «Скопировано — ⌘V»; `Fallback` → `spawn_job` с сохранённым звуком.
  - Esc/отмена → `cancel()`.
  - Ошибки: `ProviderError::Timeout` → ключ `slow_provider`; `BadResponse` с «User location is not supported» → `region_blocked`; `Network` → `network` (как сейчас).
  - Тексты: ru `"errors.slow_provider": "Gemini отвечает слишком долго"`, `"errors.region_blocked": "Gemini недоступен в твоём регионе — нужен VPN"`; en `"errors.slow_provider": "Gemini is taking too long"`, `"errors.region_blocked": "Gemini isn't available in your region — use a VPN"`.

- [ ] **Step 1: Failing tests:** `gemini_dictation_goes_live` (фейк `LiveConnector` вызван, `output.deliver` не вызван, история содержит чистовик), `live_waits_for_empty_queue` (идёт обработка прошлой записи → новая идёт пакетным путём), `live_fallback_runs_batch_with_recorded_audio`, `not_committed_is_delivered`, `focus_moved_copies_to_clipboard` (`copy_only` вызван, `deliver` — нет), `esc_cancels_live`, `groq_stays_batch`, `timeout_is_slow_provider`, `location_error_is_region_blocked`, плюс i18n-тест ключей RU/EN (существующий).
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** весь `cargo test --lib` и `pnpm test` — PASS, clippy 0.
- [ ] **Step 5:** commit `feat(live): orchestrator picks the live path; honest provider errors` + push.

---

### Task 8: Живая проверка, бюджеты и документы

**Files:**
- Modify (если нужна подгонка): модули задач 2–7; `docs/superpowers/specs/2026-11-04-live-dictation-design.md` (итоговые цифры, выбранная модель перевода), `docs/superpowers/HANDOFF.md`, `CLAUDE.md` (новые неочевидные правила)

- [ ] **Step 1:** сборка `pkill -x utter; scripts/dev-bundle.sh`, предупредить про Связку ключей, `open Utter.app`; в меню раскладок появился «Utter» без похода в Системные настройки.
- [ ] **Step 2: С пользователем:** по 3 диктовки в Notion, Claude, Telegram, TextEdit, Safari (Диктовка и Перевод); смена окна посреди диктовки; Esc; поле пароля; выключенный VPN; сон → пробуждение → диктовка. Ожидание: ни потерянного текста, ни «застрявшей» раскладки, честные ошибки.
- [ ] **Step 3: Бюджеты по логу** (медиана и максимум за ≥ 10 диктовок): `live first interim_ms` → черновик отстаёт ≤ 0,7 с; `release→committed_ms` ≤ 800 (Диктовка), ≤ 1600 (Перевод). Не уложились — `systematic-debugging`, цифры не подгонять.
- [ ] **Step 4:** простой: `top` 2 мин — utter и страница острова в прежних бюджетах (§8.4), способ ввода 0 %.
- [ ] **Step 5:** честный отчёт пользователю (что проверено, что нет); обновить спецификацию (цифры), HANDOFF, CLAUDE.md; commit `docs: live dictation — live check and handoff` + push.
