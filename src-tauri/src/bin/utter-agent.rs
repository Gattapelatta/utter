//! `utter-agent` — run by AI agents, tells Utter what they do.
//!
//!   utter-agent claude <HookEvent>          Claude Code hook: the event's JSON on stdin
//!   utter-agent codex-hook <HookEvent>      Codex hook: the same, Codex's fields
//!   utter-agent codex <prev…> <json>        Codex `notify` (Utter before 2026-11-13): then runs the previous command
//!
//! Never slows the agent down: no Utter, or a socket that does not take the event within
//! 300 ms → exit 0 silently. Only `PermissionRequest` waits for an answer, and prints nothing
//! unless the user decided in Utter.

#[path = "../agents/wire.rs"]
#[allow(dead_code)]
mod wire;

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use wire::{Reply, Request, Source};

/// Connecting and handing over an event.
const SEND_TIMEOUT: Duration = Duration::from_millis(300);
/// Waiting for the user's answer; Claude Code's own limit for this hook is 120 s.
const REPLY_TIMEOUT: Duration = Duration::from_secs(110);

fn send(req: &Request) -> Option<UnixStream> {
    let mut stream = UnixStream::connect(wire::socket_path()).ok()?;
    stream.set_write_timeout(Some(SEND_TIMEOUT)).ok()?;
    stream.write_all(req.to_line().as_bytes()).ok()?;
    Some(stream)
}

fn wait_reply(stream: UnixStream) -> Reply {
    if stream.set_read_timeout(Some(REPLY_TIMEOUT)).is_err() {
        return Reply::NoDecision;
    }
    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(n) if n > 0 => serde_json::from_str(&line).unwrap_or(Reply::NoDecision),
        _ => Reply::NoDecision,
    }
}

fn request(source: Source, event: &str, payload: serde_json::Value, wants_reply: bool) -> Request {
    Request {
        v: 1,
        source,
        event: event.to_string(),
        payload,
        ancestors: wire::ancestors(std::process::id() as i32),
        wants_reply,
    }
}

/// A Claude Code or Codex hook. Codex gets no «Always» (it fails closed on
/// `updatedPermissions`) and JSON from `Stop`, which it requires.
fn hook(source: Source, event: &str) {
    let codex = source == Source::Codex;
    if codex && event == "Stop" {
        println!("{}", wire::stop_output());
    }
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&input) else { return };
    let wants_reply = event == "PermissionRequest";
    let suggestion = if codex { None } else { payload.get("permission_suggestions").and_then(|s| s.get(0)).cloned() };
    let Some(stream) = send(&request(source, event, payload, wants_reply)) else { return };
    if !wants_reply {
        return;
    }
    if let Some(out) = wire::hook_output(&wait_reply(stream), suggestion.as_ref()) {
        println!("{out}");
    }
}

fn codex(args: &[String]) {
    let Some((json, prev)) = args.split_last() else { return };
    let payload = serde_json::from_str(json).unwrap_or_else(|_| serde_json::Value::String(json.clone()));
    let _ = send(&request(Source::Codex, "notify", payload, false));
    if let Some((program, rest)) = prev.split_first() {
        use std::os::unix::process::CommandExt;
        // Replaces this process; returns only if the previous command cannot start.
        let _ = std::process::Command::new(program).args(rest).arg(json).exec();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((mode, rest)) if mode == "claude" || mode == "codex-hook" => {
            let source = if mode == "claude" { Source::Claude } else { Source::Codex };
            if let Some(event) = rest.first() {
                hook(source, event);
            }
        }
        Some((mode, rest)) if mode == "codex" => codex(rest),
        _ => {}
    }
}
