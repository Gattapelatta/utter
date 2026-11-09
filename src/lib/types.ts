// Mirrors of the Rust types that cross the IPC boundary.

export type Mode = "Dictation" | "Translate" | "Task";
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
  autostart: boolean;
  theme: Theme;
  ui_language: UiLanguage;
  translate_style: TranslateStyle;
  provider: ProviderKind;
  /** `task`: the «Задача» key (spec 2026-11-09 §3.7); absent only in older fixtures. */
  hotkeys: { dictation: KeyId; translate: KeyId; task?: KeyId };
  history_enabled: boolean;
  onboarding_done: boolean;
  notch_drop: boolean;
  /** Absent only in fixtures older than the agents (spec 2026-11-09); Utter always sends them. */
  agents?: AgentSettings;
  agents_codex_prev_notify?: string[] | null;
}

export interface AgentSettings {
  claude_code: boolean;
  codex: boolean;
  summary: boolean;
}

export const AGENTS_OFF: AgentSettings = { claude_code: false, codex: false, summary: true };

/** Rust `agents::service::Link`. */
export type AgentLink = { kind: "off" } | { kind: "on" } | { kind: "missing" } | { kind: "broken"; message: string };

export interface AgentsStatus {
  detected: { claude_code: boolean; codex: boolean };
  claude_code: AgentLink;
  codex: AgentLink;
}

export type OverlayState =
  | { kind: "hidden" }
  | { kind: "recording"; mode: Mode; style: TranslateStyle; started_at_ms: number; warn: boolean; pending: number }
  | { kind: "processing"; pending: number; since_ms: number }
  | { kind: "done"; outcome: "pasted" | "typed" | "copied" | "failed" | "shown"; degraded: boolean }
  | { kind: "error"; message_key: string; action: string | null; minutes: number | null; provider: string | null }
  | { kind: "cancelled" };

/** The live draft in the island's window: recognised pieces and the piece still being revised. */
export type DraftParts = { settled: string; interim: string };

export type AppStatus = "idle" | "recording" | "processing";

export type TrialOutcome =
  | { kind: "text"; mode: Mode; text: string }
  | { kind: "error"; message_key: string };

/** Daily free-tier limits; fields match UsageDay. */
export interface FreeLimits {
  stt_requests: number | null;
  audio_secs: number | null;
  llm_requests: number | null;
  llm_tokens: number | null;
}

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

/** The updater's state (Rust `app::updates::UpdateState`). */
export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up_to_date" }
  | { kind: "available"; version: string; notes: string | null }
  | { kind: "downloading"; percent: number }
  | { kind: "waiting_for_idle" }
  | { kind: "ready" }
  | { kind: "failed" };

/** Agents the Drop watches (Rust `agents::*`, spec 2026-11-09). */
export type AgentKind = "claude_code" | "codex";
export type SessionKey = { agent: AgentKind; id: string };
export type SessionState = "permission" | "waiting" | "working" | "done";

export interface AgentRow {
  key: SessionKey;
  agent: AgentKind;
  project: string;
  state: SessionState;
  steps: [number, number] | null;
  /** At most two, the newest last. */
  actions: string[];
  summary: string | null;
  seen: boolean;
}

export type AgentCard =
  /** `rule`: what «Всегда» allows from now on, in words. */
  | { kind: "permission"; id: number; key: SessionKey; tool: string; detail: string; can_always: boolean; rule?: string | null }
  | { kind: "done"; key: SessionKey }
  | { kind: "waiting"; key: SessionKey }
  | { kind: "after_pause"; done: number; waiting: number };

export interface AgentView {
  rows: AgentRow[];
  card: AgentCard | null;
  paused: boolean;
}
