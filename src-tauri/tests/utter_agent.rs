//! The `utter-agent` helper as agents run it: a separate process, a socket that may or may not answer.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const HELPER: &str = env!("CARGO_BIN_EXE_utter-agent");

const PERMISSION: &str = r#"{"session_id":"abc","cwd":"/p","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"git push origin main"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"git push origin main"}],"behavior":"allow","destination":"localSettings"}]}"#;

fn run(socket: &std::path::Path, args: &[&str], stdin: &str) -> (Output, Duration) {
    let start = Instant::now();
    let mut child = Command::new(HELPER)
        .args(args)
        .env("UTTER_AGENT_SOCKET", socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out, start.elapsed())
}

#[test]
fn no_utter_exits_fast() {
    let dir = tempfile::tempdir().unwrap();
    let (out, took) = run(&dir.path().join("none.sock"), &["claude", "PermissionRequest"], PERMISSION);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(took < Duration::from_millis(300), "{took:?}");
}

#[test]
fn stale_socket_file_exits_fast() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sock");
    drop(UnixListener::bind(&path).unwrap()); // the file stays, nobody listens
    assert!(path.exists());
    let (out, took) = run(&path, &["claude", "PermissionRequest"], PERMISSION);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(took < Duration::from_millis(300), "{took:?}");
}

#[test]
fn silent_utter_does_not_hold_ordinary_events() {
    // Utter accepts but never reads: an ordinary event must still return at once.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sock");
    let _listener = UnixListener::bind(&path).unwrap();
    let (out, took) = run(&path, &["claude", "Stop"], r#"{"session_id":"a","last_assistant_message":"done"}"#);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(took < Duration::from_millis(300), "{took:?}");
}

#[test]
fn permission_waits_for_reply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        let req: serde_json::Value = serde_json::from_str(&line).unwrap();
        std::thread::sleep(Duration::from_millis(500)); // the user thinks
        (&stream).write_all(b"{\"reply\":\"allow\",\"always\":true}\n").unwrap();
        req
    });
    let (out, took) = run(&path, &["claude", "PermissionRequest"], PERMISSION);
    let req = server.join().unwrap();
    assert_eq!(req["source"], "claude");
    assert_eq!(req["event"], "PermissionRequest");
    assert_eq!(req["wants_reply"], true);
    assert_eq!(req["payload"]["tool_input"]["command"], "git push origin main");
    assert!(req["ancestors"].as_array().is_some_and(|a| !a.is_empty()));
    assert!(took >= Duration::from_millis(500));
    let printed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(printed["hookSpecificOutput"]["decision"]["behavior"], "allow");
    assert_eq!(
        printed["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["rules"][0]["ruleContent"],
        "git push origin main"
    );
}

#[test]
fn permission_without_decision_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        (&stream).write_all(b"{\"reply\":\"no_decision\"}\n").unwrap();
    });
    let (out, _) = run(&path, &["claude", "PermissionRequest"], PERMISSION);
    server.join().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn codex_runs_previous_notify() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agents.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        serde_json::from_str::<serde_json::Value>(&line).unwrap()
    });
    let seen = dir.path().join("seen.txt");
    let script = dir.path().join("prev.sh");
    std::fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", seen.display())).unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let json = r#"{"type":"agent-turn-complete","thread-id":"t1","cwd":"/p","last-assistant-message":"ok"}"#;
    let (out, _) = run(&path, &["codex", script.to_str().unwrap(), "turn-ended", json], "");
    assert!(out.status.success());
    let req = server.join().unwrap();
    assert_eq!(req["source"], "codex");
    assert_eq!(req["event"], "notify");
    assert_eq!(req["payload"]["thread-id"], "t1");
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), format!("turn-ended\n{json}\n"));
}

#[test]
fn codex_without_utter_still_runs_previous_notify() {
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.txt");
    let script = dir.path().join("prev.sh");
    std::fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", seen.display())).unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (out, _) = run(&dir.path().join("none.sock"), &["codex", script.to_str().unwrap(), "{}"], "");
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "{}\n");
}
