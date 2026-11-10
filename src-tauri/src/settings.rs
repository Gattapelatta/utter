use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::KeyId;

pub const SCHEMA_VERSION: u32 = 1;

/// Languages offered as a default speech language when the system uses them.
const KNOWN_SPEECH_LANGUAGES: &[&str] = &[
    "ru", "en", "uk", "de", "es", "fr", "it", "pt", "pl", "tr", "ja", "zh", "ko",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordMode {
    Hold,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranslateStyle {
    Business,
    Casual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    Gemini,
    Groq,
    OpenAi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertMethod {
    Paste,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLanguage {
    Auto,
    Ru,
    En,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    pub dictation: KeyId,
    pub translate: KeyId,
    /// «Задача» (spec 2026-11-09 §3.7). None: not assigned — newcomers pick their own key
    /// (the user 2026-11-10: a MacBook has no right ⌃).
    #[serde(default)]
    pub task: Option<KeyId>,
}

impl Hotkeys {
    /// A task key on dictation's or translation's key could never fire: it is dropped.
    fn fix_task_key(&mut self) {
        if self.task.as_ref().is_some_and(|k| *k == self.dictation || *k == self.translate) {
            self.task = None;
        }
    }
}

/// Which AI agents the Drop watches (spec 2026-11-09).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    pub claude_code: bool,
    pub codex: bool,
    /// «Итог одной строкой»: the provider writes the summary line.
    pub summary: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        AgentSettings { claude_code: false, codex: false, summary: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub record_mode: RecordMode,
    pub insert_method: InsertMethod,
    /// Whisper language code; `None` means auto-detect.
    pub speech_language: Option<String>,
    pub cleanup: bool,
    pub paragraphs: bool,
    pub autostart: bool,
    pub theme: Theme,
    pub ui_language: UiLanguage,
    pub translate_style: TranslateStyle,
    pub provider: ProviderKind,
    pub hotkeys: Hotkeys,
    pub history_enabled: bool,
    pub onboarding_done: bool,
    /// The Drop lives in the camera notch while idle (§4.2a).
    pub notch_drop: bool,
    pub agents: AgentSettings,
    /// Codex`s own `notify` command before Utter chained itself in front of it.
    pub agents_codex_prev_notify: Option<Vec<String>>,
}

impl Settings {
    pub fn defaults_for(system_lang: Option<&str>) -> Settings {
        let speech_language = system_lang
            .and_then(|l| l.split(['-', '_']).next())
            .map(|l| l.to_ascii_lowercase())
            .filter(|l| KNOWN_SPEECH_LANGUAGES.contains(&l.as_str()));
        Settings {
            schema_version: SCHEMA_VERSION,
            record_mode: RecordMode::Hold,
            insert_method: InsertMethod::Paste,
            speech_language,
            cleanup: true,
            paragraphs: false,
            autostart: false,
            theme: Theme::Dark,
            ui_language: UiLanguage::Auto,
            translate_style: TranslateStyle::Business,
            provider: ProviderKind::Gemini,
            hotkeys: default_hotkeys(),
            history_enabled: true,
            onboarding_done: false,
            notch_drop: true,
            agents: AgentSettings::default(),
            agents_codex_prev_notify: None,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::defaults_for(None)
    }
}

#[cfg(not(windows))]
fn default_hotkeys() -> Hotkeys {
    Hotkeys { dictation: KeyId::RightAlt, translate: KeyId::RightMeta, task: None }
}

#[cfg(windows)]
fn default_hotkeys() -> Hotkeys {
    Hotkeys { dictation: KeyId::RightAlt, translate: KeyId::RightCtrl, task: None }
}

/// Loads settings; a missing or unreadable file yields defaults.
/// A corrupt file is kept as `<name>.bak` so the user's data is not silently lost.
pub fn load(path: &Path, system_lang: Option<&str>) -> Settings {
    let defaults = Settings::defaults_for(system_lang);
    let Ok(bytes) = std::fs::read(path) else {
        return defaults;
    };
    // Fields absent from the file fall back to the defaults for this system.
    let mut base = match serde_json::to_value(&defaults) {
        Ok(v) => v,
        Err(_) => return defaults,
    };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(saved)) => {
            if let serde_json::Value::Object(map) = &mut base {
                for (k, v) in saved {
                    map.insert(k, v);
                }
            }
            match serde_json::from_value::<Settings>(base) {
                Ok(mut s) => {
                    s.hotkeys.fix_task_key();
                    s
                }
                Err(e) => {
                    backup_corrupt(path, &e.to_string());
                    defaults
                }
            }
        }
        Ok(_) | Err(_) => {
            backup_corrupt(path, "invalid JSON");
            defaults
        }
    }
}

fn backup_corrupt(path: &Path, _reason: &str) {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    let _ = std::fs::rename(path, bak);
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?;
    write_atomic(path, &bytes)
}

/// `write_atomic`, keeping the file's permission bits (`mode`) when given.
pub fn write_atomic_mode(path: &Path, bytes: &[u8], mode: Option<u32>) -> std::io::Result<()> {
    let Some(mode) = mode else { return write_atomic(path, bytes) };
    use std::os::unix::fs::PermissionsExt;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".mode.tmp");
    let tmp = std::path::PathBuf::from(tmp);
    write_atomic(&tmp, bytes)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&tmp, path)
}

/// Writes to a sibling temp file, syncs it, then renames over `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::KeyId;

    #[test]
    fn defaults_match_spec() {
        let s = Settings::defaults_for(Some("ru"));
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.record_mode, RecordMode::Hold);
        assert_eq!(s.insert_method, InsertMethod::Paste);
        assert_eq!(s.speech_language.as_deref(), Some("ru"));
        assert!(s.cleanup && !s.paragraphs && s.history_enabled);
        assert!(!s.autostart);
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.ui_language, UiLanguage::Auto);
        assert_eq!(s.translate_style, TranslateStyle::Business);
        assert_eq!(s.provider, ProviderKind::Gemini);
        assert_eq!(s.hotkeys.dictation, KeyId::RightAlt);
        #[cfg(target_os = "macos")]
        assert_eq!(s.hotkeys.translate, KeyId::RightMeta);
        #[cfg(windows)]
        assert_eq!(s.hotkeys.translate, KeyId::RightCtrl);
        assert!(!s.onboarding_done);
        assert!(s.notch_drop);
    }

    #[test]
    fn missing_notch_drop_means_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"schema_version":1,"autostart":true}"#).unwrap();
        let s = load(&path, None);
        assert!(s.notch_drop);
        assert!(s.autostart);
    }

    #[test]
    fn old_settings_get_agents_off_and_summary_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"schema_version":1}"#).unwrap();
        let s = load(&path, None);
        assert_eq!(s.agents, AgentSettings { claude_code: false, codex: false, summary: true });
        assert_eq!(s.agents_codex_prev_notify, None);
        // A file that has only some agent fields keeps the rest at their defaults.
        std::fs::write(&path, r#"{"agents":{"codex":true}}"#).unwrap();
        assert_eq!(load(&path, None).agents, AgentSettings { claude_code: false, codex: true, summary: true });
    }

    #[test]
    fn old_settings_get_default_task_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"hotkeys":{"dictation":"CapsLock","translate":"RightMeta"}}"#).unwrap();
        let s = load(&path, None);
        assert_eq!(s.hotkeys.dictation, KeyId::CapsLock);
        // The user 2026-11-10: newcomers get no task key; each one picks their own.
        assert_eq!(s.hotkeys.task, None);
        // The file was not taken for a corrupt one.
        assert!(!dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn task_key_moves_off_a_taken_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"hotkeys":{"dictation":"RightCtrl","translate":"RightMeta","task":"RightCtrl"}}"#).unwrap();
        let s = load(&path, None);
        // A task key on another mode's key is dropped (it could never fire).
        assert_eq!(s.hotkeys.task, None);
        std::fs::write(&path, r#"{"hotkeys":{"dictation":"RightAlt","translate":"RightMeta","task":"LeftCtrl"}}"#).unwrap();
        assert_eq!(load(&path, None).hotkeys.task, Some(KeyId::LeftCtrl));
    }

    #[test]
    fn bad_notch_drop_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"notch_drop":"yes"}"#).unwrap();
        assert_eq!(load(&path, None), Settings::defaults_for(None));
        assert!(dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn unknown_system_lang_means_auto() {
        assert_eq!(Settings::defaults_for(None).speech_language, None);
        assert_eq!(Settings::defaults_for(Some("xx")).speech_language, None);
    }

    #[test]
    fn system_lang_with_region_is_reduced_to_code() {
        assert_eq!(
            Settings::defaults_for(Some("en-US")).speech_language.as_deref(),
            Some("en")
        );
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut s = Settings::defaults_for(Some("ru"));
        s.record_mode = RecordMode::Toggle;
        s.paragraphs = true;
        s.hotkeys.dictation = KeyId::F(13);
        save(&path, &s).unwrap();
        assert_eq!(load(&path, Some("ru")), s);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(load(&path, Some("ru")), Settings::defaults_for(Some("ru")));
    }

    #[test]
    fn corrupt_file_falls_back_and_keeps_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{oops").unwrap();
        assert_eq!(load(&path, Some("ru")), Settings::defaults_for(Some("ru")));
        assert!(dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"schema_version":1,"cleanup":false}"#).unwrap();
        let s = load(&path, Some("ru"));
        assert!(!s.cleanup);
        assert_eq!(s.record_mode, RecordMode::Hold);
        assert_eq!(s.speech_language.as_deref(), Some("ru"));
    }

    #[test]
    fn old_file_with_sounds_field_still_loads() {
        // Sounds were removed (2026-10-08); settings saved before keep loading.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"schema_version":1,"sounds":true,"cleanup":false}"#).unwrap();
        let s = load(&path, Some("ru"));
        assert!(!s.cleanup);
        assert!(!dir.path().join("settings.json.bak").exists());
    }

    #[test]
    fn write_atomic_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
