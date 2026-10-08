import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  FreeLimits,
  HistoryEntry,
  KeyId,
  Mode,
  OverlayState,
  PermissionsStatus,
  ProviderKind,
  Settings,
  TrialOutcome,
  UsageDay,
} from "./types";

export const ipc = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  setApiKey: (provider: ProviderKind, key: string) => invoke<void>("set_api_key", { provider, key }),
  apiKeyMask: (provider: ProviderKind) => invoke<string | null>("api_key_mask", { provider }),
  verifyKey: (provider: ProviderKind) => invoke<void>("verify_key", { provider }),
  captureKey: () => invoke<KeyId>("capture_key"),
  cancelCapture: () => invoke<void>("cancel_capture"),
  setTrial: (on: boolean) => invoke<void>("set_trial", { on }),
  freeLimits: (provider: ProviderKind) => invoke<FreeLimits | null>("free_limits", { provider }),
  permissionsStatus: () => invoke<PermissionsStatus>("permissions_status"),
  requestMicrophone: () => invoke<boolean>("request_microphone"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  historyList: () => invoke<HistoryEntry[]>("history_list"),
  historyRetry: (id: number) => invoke<void>("history_retry", { id }),
  usageToday: () => invoke<UsageDay | null>("usage_today"),
  copyDiagnostics: () => invoke<string>("copy_diagnostics"),
  overlayAction: (action: string) => invoke<void>("overlay_action", { action }),
  settingsReady: () => invoke<void>("settings_ready"),
  openSettings: (route: string | null) => invoke<void>("open_settings", { route }),
  openLink: (link: "gemini_key" | "groq_key" | "openai_key" | "github" | "releases") => invoke<void>("open_link", { link }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  openMicrophoneSettings: () => invoke<void>("open_microphone_settings"),
  overlayPainted: () => invoke<void>("overlay_painted"),
  overlayFrames: (avg: number, p95: number, max: number, count: number) =>
    invoke<void>("overlay_frames", { avg, p95, max, count }),
};

export const events = {
  overlayState: (cb: (s: OverlayState) => void): Promise<UnlistenFn> =>
    listen<OverlayState>("overlay://state", (e) => cb(e.payload)),
  overlayLevel: (cb: (level: number) => void): Promise<UnlistenFn> =>
    listen<number>("overlay://level", (e) => cb(e.payload)),
  overlayScreen: (cb: (s: { notch: { width: number; height: number } | null }) => void): Promise<UnlistenFn> =>
    listen<{ notch: { width: number; height: number } | null }>("overlay://screen", (e) => cb(e.payload)),
  hotkey: (cb: (e: { mode: Mode; down: boolean }) => void): Promise<UnlistenFn> =>
    listen<{ mode: Mode; down: boolean }>("ui://hotkey", (e) => cb(e.payload)),
  trial: (cb: (o: TrialOutcome) => void): Promise<UnlistenFn> => listen<TrialOutcome>("ui://trial", (e) => cb(e.payload)),
  history: (cb: () => void): Promise<UnlistenFn> => listen("ui://history", () => cb()),
  settings: (cb: (s: Settings) => void): Promise<UnlistenFn> => listen<Settings>("ui://settings", (e) => cb(e.payload)),
  navigate: (cb: (route: string) => void): Promise<UnlistenFn> => listen<string>("ui://navigate", (e) => cb(e.payload)),
};
