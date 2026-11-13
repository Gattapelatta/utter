//! Connecting the Drop to agents: Utter's hooks in `~/.claude/settings.json` and `~/.codex/hooks.json`
//! (and taking out the older `notify` line in `~/.codex/config.toml`). Pure functions over the file's text; the caller reads and writes.
//! Utter's entries are recognised by `utter-agent` in the command; everything else is left as it is.

use serde_json::{json, Map, Value};
use std::path::Path;

pub const CLAUDE_EVENTS: [&str; 7] =
    ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Notification", "PermissionRequest", "Stop", "SessionEnd"];

/// The mark of Utter's own hooks and `notify` entry.
const MARK: &str = "utter-agent";

/// `PostToolUse` only matters for the task list (task-list tools, see `event::plan_update`).
const STEPS_MATCHER: &str = "TodoWrite|mcp__ccd_turn__update_plan|mcp__ccd_turn__wrap_up";

/// Seconds Claude Code waits for the permission hook (Utter itself gives up after 30 s on screen).
const PERMISSION_TIMEOUT: u64 = 120;

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// The agent's file is not valid JSON/TOML (or not the shape we expect): we leave it alone.
    Corrupt(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Detected {
    pub claude_code: bool,
    pub codex: bool,
}

// ---------- Claude Code: ~/.claude/settings.json ----------

/// Does nothing (exit 0, no output) when the helper is gone — Utter.app trashed or moved — so
/// Claude Code never shows a hook error for it (review 2026-11-10).
fn claude_command(helper: &str, event: &str) -> String {
    format!("[ -x \"{helper}\" ] && \"{helper}\" claude {event} || true")
}

fn parse_settings(current: Option<&str>) -> Result<Map<String, Value>, InstallError> {
    let text = current.unwrap_or("");
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(InstallError::Corrupt("not a JSON object".into())),
        Err(e) => Err(InstallError::Corrupt(e.to_string())),
    }
}

fn is_ours(hook: &Value) -> bool {
    hook.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARK))
}

/// Removes Utter's hooks; groups and events left empty go too, and so does an empty `hooks`.
fn remove_ours(settings: &mut Map<String, Value>) -> Result<(), InstallError> {
    let Some(hooks) = settings.get_mut("hooks") else { return Ok(()) };
    let hooks = hooks.as_object_mut().ok_or_else(|| InstallError::Corrupt("`hooks` is not an object".into()))?;
    for groups in hooks.values_mut() {
        let Some(groups) = groups.as_array_mut() else { continue };
        for group in groups.iter_mut() {
            if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                list.retain(|h| !is_ours(h));
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|l| !l.is_empty()));
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));
    if hooks.is_empty() {
        settings.remove("hooks");
    }
    Ok(())
}

/// The file's text; it ends with a newline only if the user's did (a new file does), so turning
/// Utter off gives back the very same bytes (live check 2026-11-10).
fn to_text(settings: Map<String, Value>, newline: bool) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Object(settings)).unwrap_or_default();
    if newline {
        out.push('\n');
    }
    out
}

fn ends_line(current: Option<&str>) -> bool {
    current.is_none_or(|t| t.trim().is_empty() || t.ends_with('\n'))
}

/// One agent's set of Utter hooks: which events, the command for each, and the `PostToolUse`
/// matcher for its task-list tools. Claude Code and Codex share the file format (Codex's hooks
/// reference, 2026-11-13).
struct HookSet<'a> {
    events: &'a [&'a str],
    command: &'a dyn Fn(&str) -> String,
    steps_matcher: &'a str,
}

fn hooks_install(current: Option<&str>, set: &HookSet) -> Result<String, InstallError> {
    let mut settings = parse_settings(current)?;
    remove_ours(&mut settings)?;
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().ok_or_else(|| InstallError::Corrupt("`hooks` is not an object".into()))?;
    for event in set.events {
        let mut hook = json!({"type": "command", "command": (set.command)(event)});
        if *event == "PermissionRequest" {
            hook["timeout"] = json!(PERMISSION_TIMEOUT);
        }
        let mut group = json!({"hooks": [hook]});
        if *event == "PostToolUse" {
            group = json!({"matcher": set.steps_matcher, "hooks": group["hooks"].take()});
        }
        let groups = hooks.entry(*event).or_insert_with(|| json!([]));
        groups
            .as_array_mut()
            .ok_or_else(|| InstallError::Corrupt(format!("`hooks.{event}` is not a list")))?
            .push(group);
    }
    Ok(to_text(settings, ends_line(current)))
}

/// Every event has Utter's hook with exactly this command (and the current `PostToolUse` matcher).
fn hooks_installed(current: &str, set: &HookSet) -> bool {
    let Ok(settings) = parse_settings(Some(current)) else { return false };
    set.events.iter().all(|event| {
        let wanted = (set.command)(event);
        settings
            .get("hooks")
            .and_then(|h| h.get(*event))
            .and_then(Value::as_array)
            .is_some_and(|groups| {
                groups.iter().any(|g| {
                    let ours = g.get("hooks").and_then(Value::as_array).is_some_and(|hs| {
                        hs.iter().any(|h| h.get("command").and_then(Value::as_str) == Some(wanted.as_str()))
                    });
                    // An older Utter's `PostToolUse` matcher missed tools: rewrite it (spec 2026-11-13 §3.3).
                    ours && (*event != "PostToolUse" || g.get("matcher").and_then(Value::as_str) == Some(set.steps_matcher))
                })
            })
    })
}

fn hooks_uninstall(current: &str) -> Result<String, InstallError> {
    let mut settings = parse_settings(Some(current))?;
    remove_ours(&mut settings)?;
    Ok(to_text(settings, ends_line(Some(current))))
}

fn claude_set(helper: &str) -> impl Fn(&str) -> String + '_ {
    move |event| claude_command(helper, event)
}

/// Adds Utter's hook to every event in `CLAUDE_EVENTS` (replacing an older one, e.g. after the app moved).
pub fn claude_install(current: Option<&str>, helper: &str) -> Result<String, InstallError> {
    hooks_install(current, &HookSet { events: &CLAUDE_EVENTS, command: &claude_set(helper), steps_matcher: STEPS_MATCHER })
}

pub fn claude_uninstall(current: &str) -> Result<String, InstallError> {
    hooks_uninstall(current)
}

/// Every event has Utter's hook with exactly this helper path.
pub fn claude_installed(current: &str, helper: &str) -> bool {
    hooks_installed(current, &HookSet { events: &CLAUDE_EVENTS, command: &claude_set(helper), steps_matcher: STEPS_MATCHER })
}

// ---------- Codex: its hooks in ~/.codex/hooks.json ----------

pub const CODEX_EVENTS: [&str; 6] = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", "SessionEnd"];
/// Codex's plan tool.
const CODEX_STEPS_MATCHER: &str = "^update_plan$";

/// The hook runs the stable link Utter keeps pointing at itself, never Utter's own path: Codex
/// trusts a hook by the hash of its text, so the text must not change when Utter moves. No shell
/// syntax (how Codex runs it is not documented); quotes only when the path has a space.
fn codex_command(link: &str, event: &str) -> String {
    if link.contains(' ') {
        format!("\"{link}\" codex-hook {event}")
    } else {
        format!("{link} codex-hook {event}")
    }
}

fn codex_set(link: &str) -> impl Fn(&str) -> String + '_ {
    move |event| codex_command(link, event)
}

pub fn codex_hooks_install(current: Option<&str>, link: &str) -> Result<String, InstallError> {
    hooks_install(current, &HookSet { events: &CODEX_EVENTS, command: &codex_set(link), steps_matcher: CODEX_STEPS_MATCHER })
}

pub fn codex_hooks_installed(current: &str, link: &str) -> bool {
    hooks_installed(current, &HookSet { events: &CODEX_EVENTS, command: &codex_set(link), steps_matcher: CODEX_STEPS_MATCHER })
}

pub fn codex_hooks_uninstall(current: &str) -> Result<String, InstallError> {
    hooks_uninstall(current)
}

// ---------- Codex before 2026-11-13: the top-level `notify = [...]` in ~/.codex/config.toml ----------
// Only taken out now (`codex_uninstall`); `codex_install` stays for the tests that build such a file.

/// A top-level `notify` statement: its strings and the lines it spans (inclusive).
type NotifyAt = (Vec<String>, usize, usize);

/// Reads TOML strings of an array starting at `s` (just after `=`). Returns them and the
/// byte length consumed up to and including `]`.
fn parse_array(s: &str) -> Result<(Vec<String>, usize), InstallError> {
    let corrupt = |why: &str| InstallError::Corrupt(format!("notify: {why}"));
    let mut chars = s.char_indices().peekable();
    while chars.peek().is_some_and(|(_, c)| *c == ' ' || *c == '\t') {
        chars.next();
    }
    if chars.next().map(|(_, c)| c) != Some('[') {
        return Err(corrupt("not a list"));
    }
    let mut out = Vec::new();
    while let Some((i, c)) = chars.next() {
        match c {
            ']' => return Ok((out, i + 1)),
            ' ' | '\t' | '\r' | '\n' | ',' => {}
            '#' => {
                while chars.peek().is_some_and(|(_, c)| *c != '\n') {
                    chars.next();
                }
            }
            '\'' => {
                let mut v = String::new();
                loop {
                    match chars.next() {
                        Some((_, '\'')) => break,
                        Some((_, ch)) => v.push(ch),
                        None => return Err(corrupt("unclosed string")),
                    }
                }
                out.push(v);
            }
            '"' => {
                let mut v = String::new();
                loop {
                    match chars.next() {
                        Some((_, '"')) => break,
                        Some((_, '\\')) => match chars.next() {
                            Some((_, 'n')) => v.push('\n'),
                            Some((_, 't')) => v.push('\t'),
                            Some((_, 'u')) => {
                                let hex: String = (0..4).filter_map(|_| chars.next().map(|(_, h)| h)).collect();
                                let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                                v.push(ch.ok_or_else(|| corrupt("bad escape"))?);
                            }
                            Some((_, ch)) => v.push(ch),
                            None => return Err(corrupt("unclosed string")),
                        },
                        Some((_, ch)) => v.push(ch),
                        None => return Err(corrupt("unclosed string")),
                    }
                }
                out.push(v);
            }
            _ => return Err(corrupt("not a list of strings")),
        }
    }
    Err(corrupt("unclosed list"))
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).filter(|&i| i < text.len()).collect()
}

fn read_notify(text: &str) -> Result<Option<NotifyAt>, InstallError> {
    let starts = line_starts(text);
    for (n, &start) in starts.iter().enumerate() {
        let line = &text[start..starts.get(n + 1).copied().unwrap_or(text.len())];
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            return Ok(None); // the first table: top-level keys end here
        }
        let Some(rest) = trimmed.strip_prefix("notify") else { continue };
        let Some(value) = rest.trim_start().strip_prefix('=') else { continue };
        let at = start + (line.len() - value.len());
        let (values, used) = parse_array(&text[at..])?;
        let end_byte = at + used - 1;
        let end_line = starts.iter().rposition(|&s| s <= end_byte).unwrap_or(n);
        return Ok(Some((values, n, end_line)));
    }
    Ok(None)
}

fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn notify_line(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|v| toml_string(v)).collect();
    format!("notify = [{}]\n", items.join(", "))
}

/// The previous command inside Utter's chain: `[helper, "codex", prev…]`.
fn chained(values: &[String]) -> Option<Vec<String>> {
    if values.first().is_some_and(|v| v.contains(MARK)) {
        Some(values.iter().skip(2).cloned().collect())
    } else {
        None
    }
}

/// Replaces lines `from..=to` of `text` with `with` (or inserts it at the top when there is no range).
fn splice(text: &str, range: Option<(usize, usize)>, with: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::new();
    match range {
        None => {
            out.push_str(with);
            out.push_str(text);
        }
        Some((from, to)) => {
            for (i, line) in lines.iter().enumerate() {
                if i == from {
                    out.push_str(with);
                }
                if i < from || i > to {
                    out.push_str(line);
                }
            }
        }
    }
    out
}

/// Puts Utter first in `notify` and passes the event on to the previous command.
/// Returns the new text and the previous command (`None` when there was none).
#[cfg(test)]
pub fn codex_install(current: Option<&str>, helper: &str) -> Result<(String, Option<Vec<String>>), InstallError> {
    let text = current.unwrap_or("");
    let found = read_notify(text)?;
    let prev = match &found {
        Some((values, ..)) => chained(values).unwrap_or_else(|| values.clone()),
        None => Vec::new(),
    };
    let mut values = vec![helper.to_string(), "codex".to_string()];
    values.extend(prev.iter().cloned());
    let out = splice(text, found.map(|(_, a, b)| (a, b)), &notify_line(&values));
    // Codex rejects its whole config over one mistake (a second `notify`, a key this line reader
    // did not see): write only what a real TOML parser reads back as exactly ours.
    let ours = toml::Value::Array(values.iter().cloned().map(toml::Value::String).collect());
    match out.parse::<toml::Table>() {
        Ok(t) if t.get("notify") == Some(&ours) => Ok((out, (!prev.is_empty()).then_some(prev))),
        Ok(_) => Err(InstallError::Corrupt("notify was not where Utter looked for it".into())),
        Err(e) => Err(InstallError::Corrupt(e.to_string())),
    }
}

/// Gives `notify` back: the chain in the file, else `prev` (saved at install), else no line at all.
/// A `notify` that is not Utter's is left alone.
pub fn codex_uninstall(current: &str, prev: Option<&[String]>) -> String {
    let Ok(Some((values, from, to))) = read_notify(current) else { return current.to_string() };
    let Some(chain) = chained(&values) else { return current.to_string() };
    let restore = if chain.is_empty() { prev.map(<[String]>::to_vec).unwrap_or_default() } else { chain };
    let line = if restore.is_empty() { String::new() } else { notify_line(&restore) };
    splice(current, Some((from, to)), &line)
}

#[cfg(test)]
pub fn codex_installed(current: &str, helper: &str) -> bool {
    matches!(read_notify(current), Ok(Some((v, ..))) if v.first().map(String::as_str) == Some(helper) && v.get(1).map(String::as_str) == Some("codex"))
}

// ---------- Which agents are on this Mac ----------

pub fn detect(home: &Path) -> Detected {
    let user_apps = home.join("Applications");
    let mut found = detect_in(home, Path::new("/Applications"));
    found.claude_code |= user_apps.join("Claude.app").exists();
    found
}

fn detect_in(home: &Path, apps: &Path) -> Detected {
    Detected {
        claude_code: home.join(".claude").is_dir() || apps.join("Claude.app").exists(),
        codex: home.join(".codex").is_dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const HELPER: &str = "/Applications/Utter.app/Contents/MacOS/utter-agent";
    const MOVED: &str = "/Users/me/Apps/Utter.app/Contents/MacOS/utter-agent";
    const SKY: &str = "/Applications/OpenAI.app/Contents/Helpers/SkyComputerUseClient";

    fn json_of(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn commands(v: &Value, event: &str) -> Vec<String> {
        v["hooks"][event]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .map(|h| h["command"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn old_steps_matcher_counts_as_not_installed() {
        // Spec 2026-11-13 §3.3: an older Utter's hook missed `wrap_up`; it must be rewritten on launch.
        let new = claude_install(None, HELPER).unwrap();
        assert!(new.contains("mcp__ccd_turn__wrap_up"));
        let old = new.replace("|mcp__ccd_turn__wrap_up", "");
        assert!(!claude_installed(&old, HELPER));
        assert!(claude_installed(&claude_install(Some(&old), HELPER).unwrap(), HELPER));
    }

    #[test]
    fn claude_install_creates_file_when_missing() {
        let out = json_of(&claude_install(None, HELPER).unwrap());
        for event in CLAUDE_EVENTS {
            assert_eq!(commands(&out, event), vec![claude_command(HELPER, event)], "{event}");
        }
        let perm = &out["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(perm["timeout"], 120);
        assert!(claude_installed(&out.to_string(), HELPER));
        // An empty file is the same as no file.
        assert!(claude_installed(&claude_install(Some("  \n"), HELPER).unwrap(), HELPER));
    }

    #[test]
    fn claude_install_keeps_foreign_hooks() {
        let current = json!({
            "model": "opus",
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]},
            "permissions": {"allow": ["Bash(ls)"]}
        })
        .to_string();
        let out = json_of(&claude_install(Some(&current), HELPER).unwrap());
        assert_eq!(commands(&out, "Stop"), vec!["say done".to_string(), claude_command(HELPER, "Stop")]);
        assert_eq!(out["model"], "opus");
        assert_eq!(out["permissions"]["allow"][0], "Bash(ls)");
    }

    #[test]
    fn claude_install_keeps_key_order() {
        let current = r#"{"zeta": 1, "alpha": 2}"#;
        let out = claude_install(Some(current), HELPER).unwrap();
        assert!(out.find("zeta").unwrap() < out.find("alpha").unwrap());
    }

    #[test]
    fn claude_install_is_idempotent() {
        let once = claude_install(None, HELPER).unwrap();
        let twice = claude_install(Some(&once), HELPER).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn claude_reinstall_updates_moved_helper_path() {
        let old = claude_install(None, HELPER).unwrap();
        assert!(!claude_installed(&old, MOVED));
        let out = claude_install(Some(&old), MOVED).unwrap();
        assert!(claude_installed(&out, MOVED));
        assert!(!out.contains(HELPER));
    }

    #[test]
    fn claude_uninstall_removes_only_ours() {
        let current = json!({"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "say done"}]}
        ]}})
        .to_string();
        let installed = claude_install(Some(&current), HELPER).unwrap();
        let out = json_of(&claude_uninstall(&installed).unwrap());
        assert_eq!(out, json_of(&current));
        // Nothing but ours → the `hooks` key goes away.
        let out = json_of(&claude_uninstall(&claude_install(Some("{\"a\":1}"), HELPER).unwrap()).unwrap());
        assert_eq!(out, json!({"a": 1}));
    }

    #[test]
    fn a_file_without_a_final_newline_comes_back_without_one() {
        // Live check 2026-11-10: off → the user's file came back with one extra newline.
        let current = "{\n  \"model\": \"opus\"\n}";
        let installed = claude_install(Some(current), HELPER).unwrap();
        assert!(!installed.ends_with('\n'));
        assert_eq!(claude_uninstall(&installed).unwrap(), current);
        // A new file ends with a newline, like any text file.
        assert!(claude_install(None, HELPER).unwrap().ends_with('\n'));
    }

    #[test]
    fn claude_partial_install_is_not_installed() {
        let mut v = json_of(&claude_install(None, HELPER).unwrap());
        v["hooks"].as_object_mut().unwrap().remove("Stop");
        assert!(!claude_installed(&v.to_string(), HELPER));
    }

    #[test]
    fn claude_hook_does_nothing_without_helper() {
        // Review 2026-11-10: Utter.app trashed → the hook must not fail every Claude Code event.
        let out = json_of(&claude_install(None, "/nonexistent/utter-agent").unwrap());
        let cmd = commands(&out, "Stop")[0].clone();
        let run = std::process::Command::new("/bin/sh").args(["-c", &cmd]).output().unwrap();
        assert!(run.status.success());
        assert!(run.stdout.is_empty());
        assert!(run.stderr.is_empty());
    }

    #[test]
    fn claude_corrupt_json_is_error() {
        assert!(matches!(claude_install(Some("{oops"), HELPER), Err(InstallError::Corrupt(_))));
        assert!(matches!(claude_install(Some("[1,2]"), HELPER), Err(InstallError::Corrupt(_))));
        assert!(matches!(claude_install(Some(r#"{"hooks": 5}"#), HELPER), Err(InstallError::Corrupt(_))));
        assert!(matches!(claude_uninstall("{oops"), Err(InstallError::Corrupt(_))));
        assert!(!claude_installed("{oops", HELPER));
    }

    fn notify_of(text: &str) -> Vec<String> {
        read_notify(text).unwrap().unwrap().0
    }

    const LINK: &str = "/Users/me/.codex/utter/utter-agent";

    #[test]
    fn codex_hooks_install_creates_file() {
        let out = json_of(&codex_hooks_install(None, LINK).unwrap());
        for event in CODEX_EVENTS {
            assert_eq!(commands(&out, event), vec![format!("{LINK} codex-hook {event}")], "{event}");
        }
        assert_eq!(out["hooks"]["PostToolUse"][0]["matcher"], "^update_plan$");
        assert_eq!(out["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], 120);
        assert!(codex_hooks_installed(&out.to_string(), LINK));
        assert!(!codex_hooks_installed("{}", LINK));
    }

    #[test]
    fn codex_hooks_keep_foreign_and_do_not_duplicate() {
        let current = r#"{"description":"mine","hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
        let once = codex_hooks_install(Some(current), LINK).unwrap();
        let twice = codex_hooks_install(Some(&once), LINK).unwrap();
        assert_eq!(once, twice);
        let v = json_of(&twice);
        assert_eq!(v["description"], "mine");
        assert_eq!(commands(&v, "Stop"), vec!["say done".to_string(), format!("{LINK} codex-hook Stop")]);
    }

    #[test]
    fn codex_hooks_uninstall_gives_back_the_same_text() {
        let current = "{\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"command\": \"say done\",\n            \"type\": \"command\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";
        let installed = codex_hooks_install(Some(current), LINK).unwrap();
        assert_eq!(codex_hooks_uninstall(&installed).unwrap(), current);
    }

    #[test]
    fn codex_hooks_same_text_for_any_helper() {
        // Codex trusts a hook by the hash of its definition: the text must not follow Utter's path
        // (spec 2026-11-13 agents-look-and-codex-hooks §3.3). It names only the stable link.
        let out = codex_hooks_install(None, LINK).unwrap();
        assert!(!out.contains("Utter.app"));
        assert!(!out.contains("[ -x"), "no shell syntax: how Codex runs the command is not documented");
        let spaced = codex_hooks_install(None, "/Users/my name/.codex/utter/utter-agent").unwrap();
        assert!(spaced.contains(r#"\"/Users/my name/.codex/utter/utter-agent\" codex-hook Stop"#));
    }

    #[test]
    fn codex_install_chains_previous_notify() {
        let current = format!("model = \"gpt-5\"\nnotify = [\"{SKY}\", \"turn-ended\"]\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n");
        let (out, prev) = codex_install(Some(&current), HELPER).unwrap();
        assert_eq!(notify_of(&out), vec![HELPER, "codex", SKY, "turn-ended"]);
        assert_eq!(prev, Some(vec![SKY.to_string(), "turn-ended".to_string()]));
        assert!(out.starts_with("model = \"gpt-5\"\n"));
        assert!(out.ends_with("[projects.\"/x\"]\ntrust_level = \"trusted\"\n"));
        assert!(codex_installed(&out, HELPER));
        assert!(!codex_installed(&current, HELPER));
    }

    #[test]
    fn codex_install_without_notify_adds_top_level_line() {
        let current = "model = \"gpt-5\"\n\n[tui]\nnotifications = true\n";
        let (out, prev) = codex_install(Some(current), HELPER).unwrap();
        assert_eq!(prev, None);
        let notify_at = out.find("notify = [").unwrap();
        assert!(notify_at < out.find("[tui]").unwrap());
        assert_eq!(notify_of(&out), vec![HELPER, "codex"]);
        // The `notifications` key inside [tui] is not mistaken for ours.
        assert!(out.contains("[tui]\nnotifications = true\n"));
        let (out, _) = codex_install(None, HELPER).unwrap();
        assert_eq!(notify_of(&out), vec![HELPER, "codex"]);
    }

    #[test]
    fn codex_multiline_notify() {
        let current = format!("notify = [\n  \"{SKY}\",\n  \"turn-ended\", # why\n]\nmodel = \"o3\"\n");
        let (out, prev) = codex_install(Some(&current), HELPER).unwrap();
        assert_eq!(prev, Some(vec![SKY.to_string(), "turn-ended".to_string()]));
        assert_eq!(notify_of(&out), vec![HELPER, "codex", SKY, "turn-ended"]);
        assert!(out.ends_with("model = \"o3\"\n"));
        assert_eq!(out.matches("notify").count(), 1);
    }

    #[test]
    fn codex_notify_in_a_table_is_not_top_level() {
        let current = "[profiles.x]\nnotify = [\"other\"]\n";
        let (out, prev) = codex_install(Some(current), HELPER).unwrap();
        assert_eq!(prev, None);
        assert!(out.ends_with(current));
    }

    #[test]
    fn codex_install_is_idempotent() {
        let current = format!("notify = [\"{SKY}\", \"turn-ended\"]\n");
        let (once, _) = codex_install(Some(&current), HELPER).unwrap();
        let (twice, prev) = codex_install(Some(&once), MOVED).unwrap();
        assert_eq!(prev, Some(vec![SKY.to_string(), "turn-ended".to_string()]));
        assert_eq!(notify_of(&twice), vec![MOVED, "codex", SKY, "turn-ended"]);
    }

    #[test]
    fn codex_unclosed_notify_is_error() {
        assert!(matches!(codex_install(Some("notify = [\"a\",\n"), HELPER), Err(InstallError::Corrupt(_))));
        assert!(matches!(codex_install(Some("notify = 5\n"), HELPER), Err(InstallError::Corrupt(_))));
    }

    #[test]
    fn codex_quoted_notify_key_is_not_duplicated() {
        // A second `notify` would make Codex reject its whole config: refuse instead.
        for current in ["\"notify\" = [\"x\"]\n", "\u{feff}notify = [\"x\"]\n"] {
            match codex_install(Some(current), HELPER) {
                Err(InstallError::Corrupt(_)) => {}
                Ok((out, _)) => assert_eq!(out.matches("notify").count(), 1, "{out}"),
            }
        }
    }

    #[test]
    fn codex_broken_toml_is_left_alone() {
        assert!(matches!(codex_install(Some("model = \n[[["), HELPER), Err(InstallError::Corrupt(_))));
    }

    #[test]
    fn codex_uninstall_restores_previous() {
        let current = format!("model = \"gpt-5\"\nnotify = [\"{SKY}\", \"turn-ended\"]\n[tui]\n");
        let (installed, prev) = codex_install(Some(&current), HELPER).unwrap();
        assert_eq!(codex_uninstall(&installed, prev.as_deref()), current);
        // The chain in the file is enough when the saved value is lost.
        assert_eq!(codex_uninstall(&installed, None), current);
    }

    #[test]
    fn codex_uninstall_without_previous_removes_line() {
        let current = "model = \"gpt-5\"\n[tui]\n";
        let (installed, prev) = codex_install(Some(current), HELPER).unwrap();
        assert_eq!(codex_uninstall(&installed, prev.as_deref()), current);
    }

    #[test]
    fn codex_uninstall_leaves_foreign_notify() {
        let current = format!("notify = [\"{SKY}\", \"turn-ended\"]\n");
        assert_eq!(codex_uninstall(&current, Some(&["x".to_string()])), current);
    }

    #[test]
    fn codex_strings_with_quotes_and_brackets() {
        let current = "notify = [\"/a b/c\", 'lit]eral', \"q\\\"x\"]\n";
        let (out, prev) = codex_install(Some(current), HELPER).unwrap();
        assert_eq!(prev, Some(vec!["/a b/c".to_string(), "lit]eral".to_string(), "q\"x".to_string()]));
        assert_eq!(notify_of(&out), vec![HELPER, "codex", "/a b/c", "lit]eral", "q\"x"]);
    }

    #[test]
    fn detect_finds_dirs() {
        let home = tempfile::tempdir().unwrap();
        let apps = tempfile::tempdir().unwrap();
        assert_eq!(detect_in(home.path(), apps.path()), Detected { claude_code: false, codex: false });
        std::fs::create_dir(home.path().join(".codex")).unwrap();
        assert_eq!(detect_in(home.path(), apps.path()), Detected { claude_code: false, codex: true });
        std::fs::create_dir(apps.path().join("Claude.app")).unwrap();
        assert_eq!(detect_in(home.path(), apps.path()), Detected { claude_code: true, codex: true });
        let home2 = tempfile::tempdir().unwrap();
        std::fs::create_dir(home2.path().join(".claude")).unwrap();
        assert!(detect_in(home2.path(), home.path()).claude_code);
    }
}
