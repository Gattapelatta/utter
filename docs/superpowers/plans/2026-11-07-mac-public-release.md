# Utter — публичный выпуск для Mac: план

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Utter можно скачать со страницы на GitHub, поставить на любой Mac и обновлять кнопкой; у него своя иконка и README с анимацией.

**Architecture:** Выпуск — `scripts/release.sh` на Mac пользователя: проверяемая логика в модулях TypeScript `scripts/release/*.ts` (Node 24 запускает `.ts` напрямую, тесты — Vitest), сборка `tauri build` universal, подпись «Utter Dev», `.dmg` через `hdiutil`, архив обновления подписывается `tauri signer sign`, публикация `gh`. Установка обновления в Rust ждёт простоя приложения. GitHub Actions только гоняет тесты.

**Tech Stack:** bash, TypeScript (Node 24, `node:crypto` для проверки подписи minisign), Vitest, Rust (tokio, tauri-plugin-updater), `gh`, `codesign`, `lipo`, `hdiutil`, Swift (ImageIO) для GIF.

**Spec:** `docs/superpowers/specs/2026-11-07-mac-public-release-design.md`

## Global Constraints

- Подпись «Utter Dev», идентификатор `app.utter.desktop` — во всех файлах выпуска (иначе разрешения у людей не переживут обновление).
- Сборка выпуска **без** `--features debug-eval`.
- Публикация на GitHub, открытие репозитория и каждый выпуск — только после явного «да» пользователя в чате.
- Ключ обновлений `~/.tauri/utter.key` (без пароля) и сертификат никогда не попадают в репозиторий, лог или вывод в чат.
- Выпуск GitHub — не «предварительный» (иначе `releases/latest/download/latest.json` его не видит); URL обновлений в `tauri.conf.json` не меняется: `https://github.com/Gattapelatta/utter/releases/latest/download/latest.json`.
- Первая публичная версия `0.2.0`, проверочная — `0.2.1`.
- Объяснения пользователю — очень простыми словами, всегда «что тебе сделать» (память `explain-like-a-child`).
- Внешний вид: менять только то, о чём просит пользователь (память `change-only-what-asked`).

## Review Focus

1. **Версию ввели как `v0.2.0` или с пробелом** → скрипт принимает `0.2.0`/`v0.2.0` одинаково, всё остальное («0.2», «latest») отклоняет понятным сообщением. → тест `parseVersion accepts v-prefix and rejects partial versions` (Task 1).
2. **Публикация оборвалась посередине (нет сети на загрузке)** → повторный запуск не создаёт второй тег и не публикует половину: тег уходит на GitHub только после самопроверки файлов, а при уже существующем теге скрипт останавливается и подсказывает `gh release upload`. → тест `preflight refuses an existing tag, local or remote` (Task 1).
3. **Mac на Intel** → в `latest.json` есть и `darwin-x86_64`, и `darwin-aarch64`, обе ведут на один универсальный архив. → тест `latest.json lists both Mac platforms with the same universal archive` (Task 1).
4. **Нажали «Установить» во время длинной записи в режиме «Нажатие»** → скачивание идёт, перезапуск ждёт конца записи и вставки текста. → тест `restart waits until recording and processing are over` (Task 3).
5. **Архив обновления подписан не тем ключом / файл подменён** → самопроверка перед публикацией ловит это до выпуска. → тест `verifyMinisign rejects a changed file and a foreign key` (Task 1).

---

## Файлы

- Create: `scripts/release/version.ts`, `scripts/release/latest.ts`, `scripts/release/inspect.ts`, `scripts/release/minisign.ts`, `scripts/release/preflight.ts`, `scripts/release/main.ts` + рядом `*.test.ts`
- Create: `scripts/release.sh`, `docs/releases/0.2.0.md`, `docs/releases/0.2.1.md`
- Modify: `src-tauri/src/app/updates.rs`, `src-tauri/src/app/services.rs` (запоминает `AppStatus`), `src-tauri/src/app/tray.rs` (новое состояние), `src/lib/types.ts`, `src/settings/sections/About.tsx`, `src/i18n/{ru,en}.json`
- Create: `docs/design/icon.svg`, `scripts/tools/svg2png.swift`; Modify: `src-tauri/icons/*`
- Create: `scripts/tools/mov2gif.swift`, `docs/media/island.gif`, `README.md`, `README.ru.md`, `LICENSE`, `.github/workflows/ci.yml`

---

### Task 1: Логика выпуска — версии, `latest.json`, проверка файлов и подписи

**Files:** Create `scripts/release/{version,latest,inspect,minisign,preflight}.ts` и `*.test.ts` рядом. Vitest уже находит `*.test.ts` в любом месте (проверить, что `vite.config.ts` не ограничивает `include` — иначе добавить `scripts/**`).

**Interfaces — Produces:**
- `version.ts`: `parseVersion(input: string): string` (бросает `Error` с текстом «версия вида 0.2.0»); `isNewer(next: string, current: string): boolean` (семвер x.y.z, числами); `setVersion(files: { tauriConf: string; cargoToml: string; packageJson: string }, v: string): { tauriConf: string; cargoToml: string; packageJson: string }` — чистая: строки в → строки out; в `Cargo.toml` меняет только `version` в `[package]`.
- `latest.ts`: `buildLatestJson(a: { version: string; notes: string; pubDate: Date; signature: string; url: string }): LatestJson` где `LatestJson = { version: string; notes: string; pub_date: string; platforms: Record<"darwin-aarch64" | "darwin-x86_64", { signature: string; url: string }> }`; `assetUrl(version: string, file: string): string` → `https://github.com/Gattapelatta/utter/releases/download/v<version>/<file>`.
- `inspect.ts`: `parseCodesign(stderr: string): { authority: string | null; identifier: string | null }` (вывод `codesign -dv`), `parseLipo(out: string): string[]` (вывод `lipo -archs`).
- `minisign.ts`: `verifyMinisign(file: Buffer, sigFileText: string, pubKeyText: string): boolean` — формат Tauri: `.sig` и `pubkey` из `tauri.conf.json` — это base64 текстовых файлов minisign; алгоритм `ED` (BLAKE2b-512 от файла, затем Ed25519) или `Ed` (Ed25519 от файла); проверить и глобальную подпись комментария. Ed25519 — `crypto.verify(null, msg, keyObject, sig)` с ключом из 32 байт (обернуть в SPKI DER: префикс `302a300506032b6570032100`), BLAKE2b — `crypto.createHash("blake2b512")`. Совпадение id ключа (8 байт) обязательно.
- `preflight.ts`: `type Facts = { branch: string; dirty: boolean; head: string; originMain: string; tagLocal: boolean; tagRemote: boolean; current: string; next: string; notes: string | null; identity: boolean; updaterKey: boolean; targets: string[] }`; `preflight(f: Facts): string[]` — список проблем простыми словами (пусто = можно выпускать).

- [ ] **Step 1: Тесты (failing)**

```ts
it("parseVersion accepts v-prefix and rejects partial versions", () => {
  expect(parseVersion("0.2.0")).toBe("0.2.0");
  expect(parseVersion(" v0.2.1 ")).toBe("0.2.1");
  for (const bad of ["0.2", "latest", "1.0.0-beta", ""]) expect(() => parseVersion(bad)).toThrow(/0\.2\.0/);
});
it("isNewer compares numbers, not text", () => {
  expect(isNewer("0.10.0", "0.9.9")).toBe(true);
  expect(isNewer("0.2.0", "0.2.0")).toBe(false);
});
it("setVersion changes only the app's own version", () => { /* tauri.conf.json "version", package.json "version", [package] version в Cargo.toml; version зависимостей (`serde = { version = "1" }`) не трогается */ });
it("latest.json lists both Mac platforms with the same universal archive", () => {
  const j = buildLatestJson({ version: "0.2.0", notes: "n", pubDate: new Date("2026-11-08T10:00:00Z"), signature: "SIG", url: assetUrl("0.2.0", "Utter.app.tar.gz") });
  expect(Object.keys(j.platforms).sort()).toEqual(["darwin-aarch64", "darwin-x86_64"]);
  expect(j.platforms["darwin-x86_64"]).toEqual(j.platforms["darwin-aarch64"]);
  expect(j.platforms["darwin-aarch64"].url).toBe("https://github.com/Gattapelatta/utter/releases/download/v0.2.0/Utter.app.tar.gz");
  expect(j.pub_date).toBe("2026-11-08T10:00:00.000Z");
});
it("parseCodesign reads the signer and the identifier", () => { /* образец вывода `codesign -dv` с Authority=Utter Dev и Identifier=app.utter.desktop; ad-hoc вывод → authority null */ });
it("parseLipo lists architectures", () => expect(parseLipo("x86_64 arm64\n")).toEqual(["x86_64", "arm64"]));
it("verifyMinisign accepts a real Tauri signature", () => { /* фикстура: `pnpm tauri signer generate` во временную папку (без пароля), `pnpm tauri signer sign` маленького файла — сохранить три файла в scripts/release/fixtures/ (тестовый ключ, НЕ utter.key) */ });
it("verifyMinisign rejects a changed file and a foreign key", () => { /* один байт файла изменён → false; pubkey другого тестового ключа → false */ });
it("preflight refuses an existing tag, local or remote", () => { /* tagLocal → проблема; tagRemote → проблема с подсказкой про `gh release upload` */ });
it("preflight lists every problem in plain words", () => { /* не main, грязное дерево, head ≠ originMain, версия не выше, notes null или пустые, нет identity, нет ключа, нет x86_64-apple-darwin → по одной строке на каждую */ });
```

- [ ] **Step 2:** `pnpm vitest run scripts/release` — FAIL (модулей нет).
- [ ] **Step 3:** Реализовать модули по Interfaces; тестовые ключи — только в `scripts/release/fixtures/` с пометкой «тестовый ключ, не для выпусков».
- [ ] **Step 4:** `pnpm vitest run scripts/release` — PASS; `pnpm test` — всё зелёное.
- [ ] **Step 5: Commit + push** — `feat(release): version, latest.json, signature and preflight checks`.

### Task 2: Скрипт выпуска `scripts/release.sh` и пробный выпуск

**Files:** Create `scripts/release.sh`, `scripts/release/main.ts`, `docs/releases/0.2.0.md` (заметки EN + RU, коротко: «Первый публичный выпуск»).

**Interfaces:** Consumes всё из Task 1. `main.ts` — команды `facts` (собирает `Facts` через `git`, `security find-identity -v -p codesigning`, `rustup target list --installed`, `gh`), `bump <v>`, `latest <v> <sigfile> <notesfile> <out>`, `verify <app> <tar.gz> <sig>` (codesign Authority = «Utter Dev», Identifier = `app.utter.desktop`, lipo `x86_64 arm64`, minisign против `plugins.updater.pubkey`). Выход ≠ 0 и понятный текст при любой проблеме.

- [ ] **Step 1:** Установить цель: `rustup target add x86_64-apple-darwin` (сообщить пользователю, что это загрузка ~200 МБ).
- [ ] **Step 2:** `release.sh <версия> [--dry-run]`:
  1. `node scripts/release/main.ts facts <версия>` → при проблемах — вывод и выход;
  2. тесты: `cargo test --lib`, `cargo clippy --all-targets -- -D warnings`, `pnpm test`;
  3. без `--dry-run`: `main.ts bump` → коммит `release: v<версия>`;
  4. `pnpm tauri build --target universal-apple-darwin --bundles app` (без `--features`);
  5. `codesign --force --deep --sign "Utter Dev" --identifier app.utter.desktop <app>` (если заданы переменные Developer ID — ветка подписи этим сертификатом с `--options runtime` и `xcrun notarytool submit --wait`; сейчас только проверка «переменных нет → Utter Dev»);
  6. во временной папке `dist/v<версия>/`: `.dmg` — `hdiutil create -volname Utter -srcfolder <папка с Utter.app и ссылкой /Applications> -format UDZO`; архив — `tar -czf Utter.app.tar.gz -C <bundle dir> Utter.app`; подпись — `pnpm tauri signer sign --private-key-path ~/.tauri/utter.key --password "" Utter.app.tar.gz`;
  7. `main.ts verify` по приложению, распакованному из архива, и по приложению из смонтированного `.dmg`;
  8. `main.ts latest` → `latest.json`;
  9. без `--dry-run`: `git tag v<версия>`, `git push origin main v<версия>`, `gh release create v<версия> dist/v<версия>/{Utter_<версия>_universal.dmg,Utter.app.tar.gz,latest.json} --title "Utter <версия>" --notes-file docs/releases/<версия>.md`.
- [ ] **Step 3: Пробный выпуск** на `feat/release-mac`: временно разрешить ветку флагом `--allow-branch` (только с `--dry-run`) → `scripts/release.sh 0.2.0 --dry-run`. Ожидание: все проверки пройдены, в `dist/v0.2.0/` три файла, `verify` зелёный.
- [ ] **Step 4: Проверка `.dmg` вживую:** открыть `.dmg`, приложение запускается из него (у пользователя Utter уже стоит — запускать копию после `pkill -x utter`, затем вернуть рабочую сборку); `dist/` в `.gitignore`.
- [ ] **Step 5: Commit + push** — `feat(release): one-command release for Mac (universal, Utter Dev, updater archive)`.

### Task 3: Обновление ставится только в простое

**Files:** Modify `src-tauri/src/app/updates.rs`, `src-tauri/src/app/services.rs`, `src-tauri/src/app/tray.rs`, `src/lib/types.ts`, `src/settings/sections/About.tsx`, `src/i18n/{ru,en}.json`.

**Interfaces — Produces:**
- `UpdateState::WaitingForIdle` (serde `waiting_for_idle`) — TS `{ kind: "waiting_for_idle" }`; `can_check` → false для него.
- `pub struct Activity(pub std::sync::Mutex<AppStatus>)` (managed state), обновляется в `TauriUi::emit(UiEvent::Status(s))`.
- `pub async fn wait_until_idle(status: impl Fn() -> AppStatus, poll: Duration)` — возвращается, когда `status()` = `Idle` два опроса подряд (чтобы не перезапуститься между отпусканием и началом обработки).
- `install`: `update.download(..)` → если не `Idle` — `publish(WaitingForIdle)` и `wait_until_idle(.., 250 мс)` → `update.install(bytes)` → `Ready` → `app.restart()`.
- Тексты: `about.update_waiting` — RU «Установлю после диктовки», EN «Will install after dictation»; трей — тот же текст вместо пункта «Установить».

- [ ] **Step 1: Тесты:** Rust (`tokio::test(start_paused)`): `restart waits until recording and processing are over` (статусы Recording → Processing → Idle по времени: возврат только после двух опросов `Idle`); `idle at once returns at once`; `can_check_is_false_while_waiting`. TS: `About shows "will install after dictation" while waiting`.
- [ ] **Step 2:** FAIL. **Step 3:** реализовать. **Step 4:** `cargo test --lib`, clippy 0, `pnpm test` — PASS.
- [ ] **Step 5: Commit + push** — `fix(updates): restart only when no dictation is running`.

### Task 4: Иконка

**Files:** Create `docs/design/icon.svg`, `scripts/tools/svg2png.swift`; Modify `src-tauri/icons/*` (через `pnpm tauri icon`).

- [ ] **Step 1:** Три варианта иконки в виджете `visualize` (форма Капли — `BODY` и глаза `EYES` из `src/overlay/drop/Drop.tsx`; квадрат-«скругл» 1024, поле 824): A — белая Капля на чёрном, B — чёрная на светлом, C — белая с мягким ореолом на тёмно-синем. Показать в размерах 512 и 32 рядом (как в Dock и в списке).
- [ ] **Step 2:** Пользователь выбирает; правки — только названные им.
- [ ] **Step 3:** `icon.svg` → PNG 1024: `swift scripts/tools/svg2png.swift docs/design/icon.svg /tmp/icon.png 1024` (WKWebView-снимок или `NSImage` из SVG, macOS 14+ читает SVG); `pnpm tauri icon /tmp/icon.png`.
- [ ] **Step 4:** Проверка: `scripts/dev-bundle.sh`, иконка видна в Finder и в «О приложении» (`screencapture`); тесты не ломаются.
- [ ] **Step 5: Commit + push** — `feat(brand): app icon — the Drop`.

### Task 5: README, анимация, лицензия, проверка на GitHub

**Files:** Create `scripts/tools/mov2gif.swift`, `docs/media/island.gif`, `README.md`, `README.ru.md`, `LICENSE` (MIT, © 2026 Gattapelatta), `.github/workflows/ci.yml`.

- [ ] **Step 1: GIF:** dev-сборка, `screencapture -v -V 8 -R <область 440×230 у чёлки>` во время `debug-draft.txt` с фразой RU → `swift scripts/tools/mov2gif.swift in.mov docs/media/island.gif --fps 30 --scale 1` (ImageIO, `kCGImagePropertyGIFDelayTime`, цикл). Цель ≤ 3 МБ; если больше — 20 к/с. Показать пользователю, нравится ли.
- [ ] **Step 2: README** по §3.5 спецификации (EN и RU, ссылки друг на друга, GIF первым, установка в 3 шага + «Всё равно открыть» + разрешения + ключ Gemini, приватность, вопросы: лимиты, VPN, Связка ключей после обновления, Intel не проверен; планы: Windows). Ссылка «Скачать» — `https://github.com/Gattapelatta/utter/releases/latest`.
- [ ] **Step 3: CI** `.github/workflows/ci.yml`: `on: [push, pull_request]`, `runs-on: macos-latest`, шаги: checkout, pnpm (corepack), Node 24, Rust stable + кэш, `pnpm install --frozen-lockfile`, `pnpm test`, `pnpm build`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml --lib`. Проверить, что `#[ignore]`-тесты (ключи, микрофон) не запускаются.
- [ ] **Step 4:** Push → CI на GitHub зелёный (`gh run watch`); README смотрится на GitHub (`gh` / браузерная панель).
- [ ] **Step 5: Commit + push** — `docs: README (EN/RU) with the island animation, MIT license, CI`.

### Task 6: Запасная копия подписи и открытие репозитория

- [ ] **Step 1: Копия сертификата (делает пользователь):** пошаговая инструкция простыми словами: «Связка ключей» → «Мои сертификаты» → «Utter Dev» → «Экспортировать» → файл `.p12` с паролем → сохранить файл и пароль в «Пароли» рядом с «utter - key». Я не трогаю пароль и файл.
- [ ] **Step 2: Проверка истории на секреты:** `git log -p --all` по шаблонам `AIza[0-9A-Za-z_-]{35}`, `gsk_[0-9A-Za-z]{20,}`, `sk-[0-9A-Za-z]{20,}`, `BEGIN [A-Z ]*PRIVATE KEY`, строки из `~/.tauri/utter.key` (сравнить, не печатая), `password|secret|token` в изменённых файлах настроек. Ожидание: только тестовые ключи из `scripts/release/fixtures/` и заведомо поддельные из тестов. Найдено настоящее — стоп, рассказать пользователю.
- [ ] **Step 3: Финальный обзор ветки** свежим проверяющим (самая сильная модель; задачи 1–5), важные находки — по TDD; затем слияние `feat/release-mac` в `main` — с согласия пользователя (`finishing-a-development-branch`), CI на `main` зелёный. Выпуски Task 7 идут уже с `main`.
- [ ] **Step 4: «Да» пользователя → открыть:** `gh repo edit Gattapelatta/utter --visibility public --accept-visibility-change-consequences`; проверить, что страница открывается без входа.

### Task 7: Первые выпуски и живая проверка обновления

**Files:** Create `docs/releases/0.2.1.md`.

- [ ] **Step 1: «Да» пользователя → `scripts/release.sh 0.2.0`.** Ожидание: выпуск на GitHub с тремя файлами; `curl -sL https://github.com/Gattapelatta/utter/releases/latest/download/latest.json` → версия 0.2.0.
- [ ] **Step 2: Установка «как новый человек»:** убрать рабочую сборку из запуска (`pkill -x utter`), скачать `.dmg` браузером, перетащить в «Программы» (заменить), первый запуск → блокировка → «Всё равно открыть» (пользователь, по шагам) → Utter работает, диктовка вставляется.
- [ ] **Step 3: «Да» → `scripts/release.sh 0.2.1`** (заметки: «Проверка обновлений»). В Utter 0.2.0: «О приложении» → «Проверить обновления» → «Установить» → перезапуск → версия 0.2.1, диктовка работает **без** нового разрешения «Универсального доступа»; вопрос Связки ключей — ожидаем, «Разрешать всегда».
- [ ] **Step 4:** Обновить `HANDOFF.md`, `CLAUDE.md` («Current focus»), спецификацию (что проверено), память; commit + push (документы — прямо в `main`).

Task 7 работает на `main` (финальный обзор и слияние — в Task 6); правки по итогам живой проверки — на новой ветке `fix/*` с тестами.
