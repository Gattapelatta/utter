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

export function installDevMock() {
  let id = 0;
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    transformCallback: () => ++id,
    invoke: async (cmd: string, args?: { settings?: unknown }) => {
      if (cmd === "get_settings") return settings;
      if (cmd === "save_settings" && args?.settings) Object.assign(settings, args.settings);
      if (cmd === "plugin:event|listen") return ++id;
      return null;
    },
  };
}
