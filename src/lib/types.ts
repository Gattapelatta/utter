// Mirrors of the Rust types that cross the IPC boundary.

export type Mode = "Dictation" | "Translate";
export type TranslateStyle = "Business" | "Casual";
export type RecordMode = "Hold" | "Toggle";
export type InsertMethod = "Paste" | "Type";
export type ProviderKind = "Gemini" | "Groq" | "OpenAi";
export type Theme = "Dark" | "Light" | "System";
export type UiLanguage = "Auto" | "Ru" | "En";

export type KeyId =
  | "RightAlt"
  | "RightMeta"
  | "RightCtrl"
  | "RightShift"
  | "CapsLock"
  | "Fn"
  | "Escape"
  | { F: number }
  | { Raw: number };

export interface Settings {
  schema_version: number;
  record_mode: RecordMode;
  insert_method: InsertMethod;
  speech_language: string | null;
  cleanup: boolean;
  paragraphs: boolean;
  sounds: boolean;
  autostart: boolean;
  theme: Theme;
  ui_language: UiLanguage;
  translate_style: TranslateStyle;
  provider: ProviderKind;
  hotkeys: { dictation: KeyId; translate: KeyId };
  history_enabled: boolean;
  onboarding_done: boolean;
}

export type OverlayState =
  | { kind: "hidden" }
  | { kind: "recording"; mode: Mode; style: TranslateStyle; started_at_ms: number; warn: boolean; pending: number }
  | { kind: "processing"; pending: number }
  | { kind: "done"; outcome: "pasted" | "typed" | "copied" | "failed"; degraded: boolean }
  | { kind: "error"; message_key: string; action: string | null; minutes: number | null };

export type EntryStatus =
  | { kind: "done" }
  | { kind: "degraded" }
  | { kind: "nothing_heard" }
  | { kind: "failed"; message: string };

export interface HistoryEntry {
  id: number;
  created_at_ms: number;
  mode: Mode;
  text: string;
  raw: string;
  status: EntryStatus;
  audio_file: string | null;
}

export interface UsageDay {
  date: string;
  stt_requests: number;
  audio_secs: number;
  llm_requests: number;
  llm_tokens: number;
}

export interface PermissionsStatus {
  microphone: "Granted" | "Denied" | "NotDetermined";
  accessibility: boolean;
  hotkeys_running: boolean;
}

export function keyLabel(k: KeyId): string {
  if (typeof k === "object") return "F" in k ? `F${k.F}` : `Key ${k.Raw}`;
  const names: Record<string, string> = {
    RightAlt: "Right ⌥ Option",
    RightMeta: "Right ⌘ Command",
    RightCtrl: "Right ⌃ Control",
    RightShift: "Right ⇧ Shift",
    CapsLock: "Caps Lock",
    Fn: "fn",
    Escape: "Esc",
  };
  return names[k] ?? k;
}
