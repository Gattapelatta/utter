import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  HistoryEntry,
  KeyId,
  OverlayState,
  PermissionsStatus,
  ProviderKind,
  Settings,
  UsageDay,
} from "./types";

export const ipc = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  setApiKey: (provider: ProviderKind, key: string) => invoke<void>("set_api_key", { provider, key }),
  apiKeyMask: (provider: ProviderKind) => invoke<string | null>("api_key_mask", { provider }),
  verifyKey: (provider: ProviderKind) => invoke<void>("verify_key", { provider }),
  captureKey: () => invoke<KeyId>("capture_key"),
  permissionsStatus: () => invoke<PermissionsStatus>("permissions_status"),
  requestMicrophone: () => invoke<boolean>("request_microphone"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  historyList: () => invoke<HistoryEntry[]>("history_list"),
  historyRetry: (id: number) => invoke<void>("history_retry", { id }),
  usageToday: () => invoke<UsageDay | null>("usage_today"),
  copyDiagnostics: () => invoke<string>("copy_diagnostics"),
  overlayAction: (action: string) => invoke<void>("overlay_action", { action }),
};

export const events = {
  overlayState: (cb: (s: OverlayState) => void): Promise<UnlistenFn> =>
    listen<OverlayState>("overlay://state", (e) => cb(e.payload)),
  overlayLevel: (cb: (level: number) => void): Promise<UnlistenFn> =>
    listen<number>("overlay://level", (e) => cb(e.payload)),
};
