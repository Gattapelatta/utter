//! Agent events in one shape: Claude Code hook input (stdin JSON) and Codex `notify` payloads.
//! Formats checked against the Claude Code hooks reference on 2026-11-10.

use crate::app::tray::Lang;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    ClaudeCode,
    Codex,
}

/// One agent session: Claude Code's `session_id`, Codex's `thread-id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    pub agent: Agent,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// The user gave the agent a prompt: it works.
    Started,
    /// What the agent is doing, short: «Правит billing.ts».
    Action(String),
    /// Its task list: `done` of `total`.
    Steps { done: u32, total: u32 },
    /// It stopped and waits for the user (the message Claude Code shows).
    Waiting(String),
    /// It asks to run a tool. `suggestion` is Claude Code's own "don't ask again" rule, if any.
    Permission { tool: String, detail: String, suggestion: Option<Value> },
    Finished { last_message: String },
    Ended,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentEvent {
    pub key: SessionKey,
    /// The folder name of the session's working directory.
    pub project: String,
    pub kind: EventKind,
}

/// Longest command or argument shown in an action, in characters (the rest becomes «…»).
const DETAIL_MAX: usize = 40;

/// Tools that go through the permission flow but are questions to the user, not permissions.
const QUESTION_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn project_of(cwd: Option<&str>) -> String {
    let cwd = cwd.unwrap_or("").trim_end_matches('/');
    cwd.rsplit('/').next().unwrap_or("").to_string()
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

fn shorten(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= DETAIL_MAX {
        return s;
    }
    let mut out: String = s.chars().take(DETAIL_MAX - 1).collect();
    out.push('…');
    out
}

/// The tool's main argument, short: a file name, a command, a pattern.
fn tool_detail(tool: &str, input: &Value) -> String {
    let field = match tool {
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            return text(input, "file_path").or(text(input, "notebook_path")).map(basename).unwrap_or("").to_string()
        }
        "Bash" => "command",
        "Grep" | "Glob" => "pattern",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => return String::new(),
    };
    text(input, field).map(shorten).unwrap_or_default()
}

fn action(tool: &str, input: &Value, lang: Lang) -> Option<String> {
    if tool.starts_with("mcp__") {
        return None;
    }
    let verb = match (tool, lang) {
        ("Read", Lang::Ru) => "Читает",
        ("Read", Lang::En) => "Reading",
        ("Edit" | "Write" | "MultiEdit" | "NotebookEdit", Lang::Ru) => "Правит",
        ("Edit" | "Write" | "MultiEdit" | "NotebookEdit", Lang::En) => "Editing",
        ("Bash", Lang::Ru) => "Запускает",
        ("Bash", Lang::En) => "Running",
        ("Grep" | "Glob", Lang::Ru) => "Ищет",
        ("Grep" | "Glob", Lang::En) => "Searching",
        _ => return Some(tool.to_string()),
    };
    let detail = tool_detail(tool, input);
    Some(if detail.is_empty() { verb.to_string() } else { format!("{verb} {detail}") })
}

/// `TodoWrite` (`todos`) and the desktop app's plan tool (`items`) both send the whole list.
fn steps(input: &Value) -> Option<EventKind> {
    let list = input.get("todos").or(input.get("items"))?.as_array()?;
    let done = list.iter().filter(|t| text(t, "status") == Some("completed")).count() as u32;
    Some(EventKind::Steps { done, total: list.len() as u32 })
}

/// One Claude Code hook input. `None` for events and inputs the Drop does not show.
pub fn parse_claude(hook_event: &str, payload: &Value, lang: Lang) -> Option<AgentEvent> {
    let id = text(payload, "session_id").filter(|s| !s.is_empty())?;
    let tool = text(payload, "tool_name").unwrap_or("");
    let input = payload.get("tool_input").unwrap_or(&Value::Null);
    let kind = match hook_event {
        "UserPromptSubmit" => EventKind::Started,
        "PreToolUse" => EventKind::Action(action(tool, input, lang)?),
        "PostToolUse" => steps(input)?,
        "Notification" => match text(payload, "notification_type") {
            Some("idle_prompt" | "agent_needs_input") => {
                EventKind::Waiting(text(payload, "message").unwrap_or("").to_string())
            }
            _ => return None,
        },
        "PermissionRequest" if QUESTION_TOOLS.contains(&tool) => EventKind::Waiting(String::new()),
        "PermissionRequest" => EventKind::Permission {
            tool: tool.to_string(),
            detail: tool_detail(tool, input),
            suggestion: payload.get("permission_suggestions").and_then(|s| s.get(0)).cloned(),
        },
        "Stop" => EventKind::Finished { last_message: text(payload, "last_assistant_message").unwrap_or("").to_string() },
        "SessionEnd" => EventKind::Ended,
        _ => return None,
    };
    Some(AgentEvent {
        key: SessionKey { agent: Agent::ClaudeCode, id: id.to_string() },
        project: project_of(text(payload, "cwd")),
        kind,
    })
}

/// One Codex `notify` payload; only `agent-turn-complete` exists.
pub fn parse_codex(payload: &Value) -> Option<AgentEvent> {
    if text(payload, "type") != Some("agent-turn-complete") {
        return None;
    }
    let id = text(payload, "thread-id").filter(|s| !s.is_empty())?;
    Some(AgentEvent {
        key: SessionKey { agent: Agent::Codex, id: id.to_string() },
        project: project_of(text(payload, "cwd")),
        kind: EventKind::Finished { last_message: text(payload, "last-assistant-message").unwrap_or("").to_string() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tray::Lang;
    use serde_json::{json, Value};

    fn fixture(name: &str) -> Value {
        let text = match name {
            "user_prompt_submit" => include_str!("fixtures/claude_user_prompt_submit.json"),
            "pre_tool_use" => include_str!("fixtures/claude_pre_tool_use.json"),
            "post_tool_use_todo" => include_str!("fixtures/claude_post_tool_use_todo.json"),
            "notification" => include_str!("fixtures/claude_notification.json"),
            "permission_request" => include_str!("fixtures/claude_permission_request.json"),
            "stop" => include_str!("fixtures/claude_stop.json"),
            "session_end" => include_str!("fixtures/claude_session_end.json"),
            "codex" => include_str!("fixtures/codex_turn_complete.json"),
            _ => unreachable!(),
        };
        serde_json::from_str(text).unwrap()
    }

    fn claude(event: &str, file: &str) -> AgentEvent {
        parse_claude(event, &fixture(file), Lang::Ru).expect(event)
    }

    #[test]
    fn parses_each_claude_event() {
        let ev = claude("UserPromptSubmit", "user_prompt_submit");
        assert_eq!(ev.key, SessionKey { agent: Agent::ClaudeCode, id: "abc123".into() });
        assert_eq!(ev.kind, EventKind::Started);
        assert_eq!(claude("PreToolUse", "pre_tool_use").kind, EventKind::Action("Правит billing.ts".into()));
        assert_eq!(
            claude("Notification", "notification").kind,
            EventKind::Waiting("Claude is waiting for your input".into())
        );
        assert!(matches!(claude("PermissionRequest", "permission_request").kind, EventKind::Permission { .. }));
        assert!(matches!(claude("Stop", "stop").kind, EventKind::Finished { .. }));
        assert_eq!(claude("SessionEnd", "session_end").kind, EventKind::Ended);
    }

    #[test]
    fn todo_write_gives_steps() {
        assert_eq!(claude("PostToolUse", "post_tool_use_todo").kind, EventKind::Steps { done: 3, total: 5 });
    }

    #[test]
    fn plan_items_give_steps_too() {
        let p = json!({"session_id":"s","cwd":"/p","tool_name":"mcp__ccd_turn__update_plan",
            "tool_input":{"items":[{"content":"a","status":"completed"},{"content":"b","status":"in_progress"}]}});
        assert_eq!(parse_claude("PostToolUse", &p, Lang::En).unwrap().kind, EventKind::Steps { done: 1, total: 2 });
    }

    #[test]
    fn other_post_tool_use_is_none() {
        let p = json!({"session_id":"s","cwd":"/p","tool_name":"Edit","tool_input":{"file_path":"/a"}});
        assert_eq!(parse_claude("PostToolUse", &p, Lang::En), None);
    }

    #[test]
    fn pre_tool_use_makes_short_action() {
        let action = |tool: &str, input: Value, lang| {
            let p = json!({"session_id":"s","cwd":"/p","tool_name":tool,"tool_input":input});
            match parse_claude("PreToolUse", &p, lang).map(|e| e.kind) {
                Some(EventKind::Action(a)) => Some(a),
                _ => None,
            }
        };
        assert_eq!(action("Edit", json!({"file_path":"/a/b/billing.ts"}), Lang::Ru).unwrap(), "Правит billing.ts");
        assert_eq!(action("Write", json!({"file_path":"/a/b/billing.ts"}), Lang::En).unwrap(), "Editing billing.ts");
        assert_eq!(action("Read", json!({"file_path":"/x/y.rs"}), Lang::Ru).unwrap(), "Читает y.rs");
        assert_eq!(action("Bash", json!({"command":"npm test"}), Lang::Ru).unwrap(), "Запускает npm test");
        assert_eq!(action("Grep", json!({"pattern":"TODO"}), Lang::En).unwrap(), "Searching TODO");
        let long = "cargo test --manifest-path src-tauri/Cargo.toml --lib agents";
        let got = action("Bash", json!({"command": long}), Lang::En).unwrap();
        assert!(got.starts_with("Running cargo test"));
        assert!(got.ends_with('…'));
        assert_eq!(got.trim_start_matches("Running ").chars().count(), 40);
        assert_eq!(action("WebFetch", json!({"url":"https://x.y"}), Lang::En).unwrap(), "WebFetch");
        // MCP tool names are internal plumbing (`mcp__server__tool`): not shown.
        assert_eq!(action("mcp__ccd_turn__update_plan", json!({}), Lang::En), None);
    }

    #[test]
    fn permission_keeps_first_suggestion() {
        let ev = claude("PermissionRequest", "permission_request");
        let EventKind::Permission { tool, detail, suggestion } = ev.kind else { panic!() };
        assert_eq!(tool, "Bash");
        assert_eq!(detail, "git push origin main");
        assert_eq!(suggestion.unwrap()["type"], "addRules");
    }

    #[test]
    fn permission_without_suggestion_has_none() {
        let p = json!({"session_id":"s","cwd":"/p","tool_name":"Edit","tool_input":{"file_path":"/a/b/c.ts"}});
        let EventKind::Permission { detail, suggestion, .. } = parse_claude("PermissionRequest", &p, Lang::En).unwrap().kind
        else {
            panic!()
        };
        assert_eq!(detail, "c.ts");
        assert_eq!(suggestion, None);
    }

    #[test]
    fn question_tools_wait_instead_of_asking_permission() {
        // AskUserQuestion / ExitPlanMode go through the permission flow but are questions to the user:
        // «Разрешить» would make no sense there.
        let p = json!({"session_id":"s","cwd":"/p","tool_name":"AskUserQuestion","tool_input":{}});
        assert!(matches!(parse_claude("PermissionRequest", &p, Lang::En).unwrap().kind, EventKind::Waiting(_)));
    }

    #[test]
    fn notification_types_other_than_waiting_are_none() {
        let p = json!({"session_id":"s","cwd":"/p","message":"m","notification_type":"permission_prompt"});
        assert_eq!(parse_claude("Notification", &p, Lang::En), None);
        let p = json!({"session_id":"s","cwd":"/p","message":"m","notification_type":"agent_needs_input"});
        assert_eq!(parse_claude("Notification", &p, Lang::En).unwrap().kind, EventKind::Waiting("m".into()));
    }

    #[test]
    fn stop_uses_last_assistant_message() {
        assert_eq!(
            claude("Stop", "stop").kind,
            EventKind::Finished { last_message: "I've fixed the rounding in billing.ts. Tests pass.".into() }
        );
        let p = json!({"session_id":"s","cwd":"/p"});
        assert_eq!(parse_claude("Stop", &p, Lang::En).unwrap().kind, EventKind::Finished { last_message: String::new() });
    }

    #[test]
    fn codex_turn_complete() {
        let ev = parse_codex(&fixture("codex")).unwrap();
        assert_eq!(ev.key, SessionKey { agent: Agent::Codex, id: "th_42".into() });
        assert_eq!(ev.project, "shop");
        assert_eq!(ev.kind, EventKind::Finished { last_message: "Fixed the rounding bug in billing.ts.".into() });
        assert_eq!(parse_codex(&json!({"type":"something-else","thread-id":"t"})), None);
    }

    #[test]
    fn unknown_event_is_none() {
        assert_eq!(parse_claude("SubagentStop", &fixture("stop"), Lang::En), None);
        // No session id → nothing to attach the event to.
        assert_eq!(parse_claude("Stop", &json!({"cwd":"/p"}), Lang::En), None);
    }

    #[test]
    fn project_is_cwd_basename() {
        assert_eq!(claude("Stop", "stop").project, "shop");
        let p = json!({"session_id":"s","cwd":"/Users/me/Projects/shop/"});
        assert_eq!(parse_claude("Stop", &p, Lang::En).unwrap().project, "shop");
        let p = json!({"session_id":"s"});
        assert_eq!(parse_claude("Stop", &p, Lang::En).unwrap().project, "");
    }

    #[test]
    fn agent_serializes_snake_case() {
        assert_eq!(serde_json::to_value(Agent::ClaudeCode).unwrap(), "claude_code");
        assert_eq!(serde_json::to_value(Agent::Codex).unwrap(), "codex");
    }
}
