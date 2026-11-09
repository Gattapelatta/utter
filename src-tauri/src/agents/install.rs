//! Connecting the Drop to agents: Utter's hooks in `~/.claude/settings.json` and its `notify`
//! line in `~/.codex/config.toml`. Pure functions over the file's text; the caller reads and writes.
//! Utter's entries are recognised by `utter-agent` in the command; everything else is left as it is.

use serde_json::{json, Map, Value};
use std::path::Path;

pub const CLAUDE_EVENTS: [&str; 7] =
    ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Notification", "PermissionRequest", "Stop", "SessionEnd"];

/// The mark of Utter's own hooks and `notify` entry.
const MARK: &str = "utter-agent";

/// `PostToolUse` only matters for the task list (whole-list tools, see `event::steps`).
const STEPS_MATCHER: &str = "TodoWrite|mcp__ccd_turn__update_plan";

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

fn claude_command(helper: &str, event: &str) -> String {
    format!("\"{helper}\" claude {event}")
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

fn to_text(settings: Map<String, Value>) -> String {
    let mut out = serde_json::to_string_pretty(&Value::Object(settings)).unwrap_or_default();
    out.push('\n');
    out
}

/// Adds Utter's hook to every event in `CLAUDE_EVENTS` (replacing an older one, e.g. after the app moved).
pub fn claude_install(current: Option<&str>, helper: &str) -> Result<String, InstallError> {
    let mut settings = parse_settings(current)?;
    remove_ours(&mut settings)?;
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().ok_or_else(|| InstallError::Corrupt("`hooks` is not an object".into()))?;
    for event in CLAUDE_EVENTS {
        let mut hook = json!({"type": "command", "command": claude_command(helper, event)});
        if event == "PermissionRequest" {
            hook["timeout"] = json!(PERMISSION_TIMEOUT);
        }
        let mut group = json!({"hooks": [hook]});
        if event == "PostToolUse" {
            group = json!({"matcher": STEPS_MATCHER, "hooks": group["hooks"].take()});
        }
        let groups = hooks.entry(event).or_insert_with(|| json!([]));
        groups
            .as_array_mut()
            .ok_or_else(|| InstallError::Corrupt(format!("`hooks.{event}` is not a list")))?
            .push(group);
    }
    Ok(to_text(settings))
}

pub fn claude_uninstall(current: &str) -> Result<String, InstallError> {
    let mut settings = parse_settings(Some(current))?;
    remove_ours(&mut settings)?;
    Ok(to_text(settings))
}

/// Every event has Utter's hook with exactly this helper path.
pub fn claude_installed(current: &str, helper: &str) -> bool {
    let Ok(settings) = parse_settings(Some(current)) else { return false };
    CLAUDE_EVENTS.iter().all(|event| {
        let wanted = claude_command(helper, event);
        settings
            .get("hooks")
            .and_then(|h| h.get(*event))
            .and_then(Value::as_array)
            .is_some_and(|groups| {
                groups.iter().filter_map(|g| g.get("hooks").and_then(Value::as_array)).flatten().any(|h| {
                    h.get("command").and_then(Value::as_str) == Some(wanted.as_str())
                })
            })
    })
}

// ---------- Codex: the top-level `notify = [...]` in ~/.codex/config.toml ----------

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
    Ok((out, (!prev.is_empty()).then_some(prev)))
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
    fn claude_install_creates_file_when_missing() {
        let out = json_of(&claude_install(None, HELPER).unwrap());
        for event in CLAUDE_EVENTS {
            assert_eq!(commands(&out, event), vec![format!("\"{HELPER}\" claude {event}")], "{event}");
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
        assert_eq!(commands(&out, "Stop"), vec!["say done".to_string(), format!("\"{HELPER}\" claude Stop")]);
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
    fn claude_partial_install_is_not_installed() {
        let mut v = json_of(&claude_install(None, HELPER).unwrap());
        v["hooks"].as_object_mut().unwrap().remove("Stop");
        assert!(!claude_installed(&v.to_string(), HELPER));
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
