//! What `utter-agent` and Utter say to each other over a Unix socket: one JSON line from the
//! helper, and — for a permission request only — one JSON line back.
//!
//! This file is also compiled into the `utter-agent` binary (`#[path]`), so it uses nothing from
//! the rest of the crate: the helper must start in milliseconds, without AppKit or Tauri.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Tests (and only tests) point the helper and Utter at another socket.
pub const SOCKET_ENV: &str = "UTTER_AGENT_SOCKET";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Claude,
    Codex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Protocol version: 1.
    pub v: u8,
    pub source: Source,
    /// Claude Code's hook event name; `"notify"` for Codex.
    pub event: String,
    pub payload: Value,
    /// Parent, grandparent… of the helper: Utter finds the app the session runs in among them.
    pub ancestors: Vec<i32>,
    /// Only a permission request waits for an answer.
    pub wants_reply: bool,
}

impl Request {
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_default();
        line.push('\n');
        line
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    /// Claude Code asks in its own window, as if Utter were not there.
    NoDecision,
    Allow { always: bool },
    Deny,
}

impl Reply {
    pub fn to_line(self) -> String {
        let mut line = serde_json::to_string(&self).unwrap_or_default();
        line.push('\n');
        line
    }
}

/// `<app data>/agents.sock` — the same folder Tauri gives Utter (`app.utter.desktop`).
pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os(SOCKET_ENV) {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/app.utter.desktop/agents.sock")
}

/// What the `PermissionRequest` hook prints for Claude Code. `None` = print nothing (no decision).
/// «Always» echoes Claude Code's own suggested rule; without one it is a plain allow.
pub fn hook_output(reply: &Reply, suggestion: Option<&Value>) -> Option<String> {
    let decision = match reply {
        Reply::NoDecision => return None,
        Reply::Allow { always: true } if suggestion.is_some() => {
            json!({"behavior": "allow", "updatedPermissions": [suggestion]})
        }
        Reply::Allow { .. } => json!({"behavior": "allow"}),
        Reply::Deny => json!({"behavior": "deny", "message": "Denied in Utter"}),
    };
    let out = json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}});
    Some(out.to_string())
}

/// Parent pids of `pid`, nearest first, up to (not including) launchd.
pub fn ancestors(pid: i32) -> Vec<i32> {
    let mut chain = Vec::new();
    let mut current = pid;
    while chain.len() < 32 {
        let Some(parent) = parent_of(current) else { break };
        if parent <= 1 {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

#[cfg(target_os = "macos")]
fn parent_of(pid: i32) -> Option<i32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: `info` is a properly sized, writable proc_bsdinfo.
    let got = unsafe {
        libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
    };
    (got == size).then_some(info.pbi_ppid as i32)
}

#[cfg(not(target_os = "macos"))]
fn parent_of(_pid: i32) -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn suggestion() -> Value {
        json!({"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"git push origin main"}],
               "behavior":"allow","destination":"localSettings"})
    }

    fn parsed(out: Option<String>) -> Value {
        serde_json::from_str(&out.expect("output")).unwrap()
    }

    #[test]
    fn hook_output_allow_always_echoes_suggestion() {
        let out = parsed(hook_output(&Reply::Allow { always: true }, Some(&suggestion())));
        assert_eq!(
            out,
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest",
                "decision":{"behavior":"allow","updatedPermissions":[suggestion()]}}})
        );
    }

    #[test]
    fn hook_output_allow_once() {
        let out = parsed(hook_output(&Reply::Allow { always: false }, Some(&suggestion())));
        assert_eq!(out, json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}));
        // «Always» without a suggestion from Claude Code is the same as once: Utter invents no rules.
        let out = parsed(hook_output(&Reply::Allow { always: true }, None));
        assert_eq!(out["hookSpecificOutput"]["decision"], json!({"behavior":"allow"}));
    }

    #[test]
    fn hook_output_deny() {
        let out = parsed(hook_output(&Reply::Deny, None));
        assert_eq!(
            out["hookSpecificOutput"]["decision"],
            json!({"behavior":"deny","message":"Denied in Utter"})
        );
    }

    #[test]
    fn no_decision_prints_nothing() {
        assert_eq!(hook_output(&Reply::NoDecision, Some(&suggestion())), None);
    }

    #[test]
    fn request_and_reply_are_one_json_line_each() {
        let req = Request {
            v: 1,
            source: Source::Claude,
            event: "Stop".into(),
            payload: json!({"a": "line\nbreak"}),
            ancestors: vec![3, 2, 1],
            wants_reply: false,
        };
        let line = req.to_line();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), req);
        let reply = Reply::Allow { always: true }.to_line();
        assert_eq!(reply.matches('\n').count(), 1);
        assert_eq!(serde_json::from_str::<Reply>(&reply).unwrap(), Reply::Allow { always: true });
    }

    #[test]
    fn socket_path_follows_env() {
        // Only this test touches the variable.
        std::env::set_var(SOCKET_ENV, "/tmp/x.sock");
        assert_eq!(socket_path(), std::path::PathBuf::from("/tmp/x.sock"));
        std::env::remove_var(SOCKET_ENV);
        assert!(socket_path().ends_with("Library/Application Support/app.utter.desktop/agents.sock"));
    }

    #[test]
    fn ancestors_of_this_process_start_with_its_parent() {
        let chain = ancestors(std::process::id() as i32);
        assert!(!chain.is_empty());
        assert_eq!(chain[0], std::os::unix::process::parent_id() as i32);
        assert!(chain.len() < 64);
    }
}
