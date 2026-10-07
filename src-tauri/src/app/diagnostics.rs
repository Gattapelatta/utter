//! Logging to a local file and a privacy-safe diagnostics report.

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use tracing_appender::non_blocking::WorkerGuard;

use crate::settings::Settings;

const LOG_FILE: &str = "utter.log";
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const KEEP_LOGS: usize = 5;
const REPORT_LINES: usize = 200;

fn key_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(gsk_|sk-)[A-Za-z0-9_\-]{8,}|AIza[0-9A-Za-z_\-]{20,}").unwrap_or_else(|_| Regex::new("$^").expect("trivial regex")))
}

pub fn redact(text: &str) -> String {
    key_pattern().replace_all(text, "[redacted]").into_owned()
}

/// Rotates `utter.log` → `utter.log.1` … when it exceeds 1 MB (5 files kept).
fn rotate(dir: &Path) {
    let current = dir.join(LOG_FILE);
    let big = std::fs::metadata(&current).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false);
    if !big {
        return;
    }
    for i in (1..KEEP_LOGS - 1).rev() {
        let _ = std::fs::rename(dir.join(format!("{LOG_FILE}.{i}")), dir.join(format!("{LOG_FILE}.{}", i + 1)));
    }
    let _ = std::fs::rename(&current, dir.join(format!("{LOG_FILE}.1")));
}

pub fn init_logging(log_dir: &Path) -> Option<WorkerGuard> {
    std::fs::create_dir_all(log_dir).ok()?;
    rotate(log_dir);
    let file = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join(LOG_FILE)).ok()?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).try_init().ok()?;
    Some(guard)
}

pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {}", redact(&info.to_string()));
        default(info);
    }));
}

pub fn report(settings: &Settings, log_dir: &Path) -> String {
    let log = std::fs::read_to_string(log_dir.join(LOG_FILE)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    let tail = lines[lines.len().saturating_sub(REPORT_LINES)..].join("\n");
    let settings_json = serde_json::to_string_pretty(settings).unwrap_or_default();
    redact(&format!(
        "Utter {}\nOS: {} {}\n\nSettings:\n{}\n\nLog (last {} lines):\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        settings_json,
        REPORT_LINES,
        tail
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_masks_api_keys() {
        assert_eq!(redact("key=gsk_abcdefghijklmnop ok"), "key=[redacted] ok");
        assert_eq!(redact("Bearer sk-proj-ABCDEFGH12345678"), "Bearer [redacted]");
        assert_eq!(redact("nothing here"), "nothing here");
        assert_eq!(redact("key AIzaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q ok"), "key [redacted] ok");
    }

    #[test]
    fn report_contains_no_secrets() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("utter.log"), "line 1\nsent gsk_secretsecret123 to groq\n").unwrap();
        let r = report(&crate::settings::Settings::defaults_for(Some("ru")), dir.path());
        assert!(!r.contains("gsk_secretsecret123"));
        assert!(r.contains("[redacted]"));
        assert!(r.contains("record_mode"));
        assert!(r.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn report_keeps_last_200_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log: String = (1..=300).map(|i| format!("line {i}\n")).collect();
        std::fs::write(dir.path().join("utter.log"), log).unwrap();
        let r = report(&crate::settings::Settings::default(), dir.path());
        assert!(r.contains("line 300") && r.contains("line 101"));
        assert!(!r.contains("line 100\n"));
    }
}
