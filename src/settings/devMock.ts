/**
 * Development only: lets `pnpm dev` render the settings window in a plain browser by
 * faking the Tauri core with default settings. Never bundled into release builds.
 */
const settings = {
  schema_version: 1,
  record_mode: "Hold",
  insert_method: "Paste",
  speech_language: null,
  cleanup: true,
  paragraphs: false,
  autostart: true,
  theme: new URLSearchParams(location.search).get("theme") === "light" ? "Light" : "Dark",
  ui_language: "Ru",
  translate_style: "Business",
  provider: "Gemini",
  hotkeys: { dictation: "RightAlt", translate: "RightMeta" },
  history_enabled: true,
  onboarding_done: true,
};

const now = Date.now();
const history = [
  { id: 4, created_at_ms: now - 120_000, mode: "Dictation", text: "Привет! Скинь, пожалуйста, ссылку на вчерашнюю презентацию, хочу ещё раз посмотреть слайды про бюджет на следующий квартал и сравнить с прошлым годом. Особенно интересует раздел про маркетинг и найм, там были вопросы.", raw: "привет скинь пожалуйста ссылку на вчерашнюю презентацию", status: { kind: "done" }, audio_file: null },
  { id: 3, created_at_ms: now - 900_000, mode: "Translate", text: "Hi team, quick update: the release is moving to Thursday because we found an issue in the payment flow.", raw: "всем привет релиз переносим на четверг", status: { kind: "done" }, audio_file: null },
  { id: 2, created_at_ms: now - 1_800_000, mode: "Dictation", text: "Созвонимся в пять?", raw: "созвонимся в пять", status: { kind: "degraded" }, audio_file: null },
  { id: 1, created_at_ms: now - 90_000_000, mode: "Translate", text: "", raw: "", status: { kind: "failed", message: "network" }, audio_file: "/tmp/a.wav" },
];

export function installDevMock() {
  let id = 0;
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    transformCallback: () => ++id,
    invoke: async (cmd: string, args?: { settings?: unknown }) => {
      if (cmd === "get_settings") return settings;
      if (cmd === "history_list") return history;
      if (cmd === "permissions_status") {
        const q = new URLSearchParams(location.search);
        return { microphone: q.get("mic") ?? "NotDetermined", accessibility: q.has("ax"), hotkeys_running: q.has("ax") };
      }
      if (cmd === "usage_today") return { date: "", stt_requests: 37, audio_secs: 744, llm_requests: 0, llm_tokens: 0 };
      if (cmd === "save_settings" && args?.settings) Object.assign(settings, args.settings);
      if (cmd === "plugin:event|listen") return ++id;
      return null;
    },
  };
}
