use std::collections::HashMap;
use std::sync::Mutex;

use crate::settings::ProviderKind;

pub const SERVICE: &str = "app.utter.desktop";

#[derive(thiserror::Error, Debug)]
pub enum SecretError {
    #[error("keychain: {0}")]
    Backend(String),
}

/// Where API keys live. Keys are trimmed on write and never logged.
pub trait SecretStore: Send + Sync {
    fn get(&self, provider: ProviderKind) -> Option<String>;
    fn set(&self, provider: ProviderKind, key: &str) -> Result<(), SecretError>;
    fn delete(&self, provider: ProviderKind) -> Result<(), SecretError>;
}

fn account(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::Gemini => "gemini",
        ProviderKind::Groq => "groq",
        ProviderKind::OpenAi => "openai",
    }
}

/// Keys in the macOS Keychain / Windows Credential Manager.
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new() -> Self {
        Self::with_service(SERVICE)
    }

    pub fn with_service(service: &str) -> Self {
        Self { service: service.to_string() }
    }

    fn entry(&self, provider: ProviderKind) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, account(provider))
            .map_err(|e| SecretError::Backend(e.to_string()))
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, provider: ProviderKind) -> Option<String> {
        self.entry(provider).ok()?.get_password().ok().filter(|k| !k.is_empty())
    }

    fn set(&self, provider: ProviderKind, key: &str) -> Result<(), SecretError> {
        self.entry(provider)?
            .set_password(key.trim())
            .map_err(|e| SecretError::Backend(e.to_string()))
    }

    fn delete(&self, provider: ProviderKind) -> Result<(), SecretError> {
        match self.entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Backend(e.to_string())),
        }
    }
}

/// In-memory store for tests and for running without a keychain.
#[derive(Default)]
pub struct MemoryStore {
    keys: Mutex<HashMap<ProviderKind, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, provider: ProviderKind) -> Option<String> {
        self.keys.lock().ok()?.get(&provider).cloned()
    }

    fn set(&self, provider: ProviderKind, key: &str) -> Result<(), SecretError> {
        let mut keys = self.keys.lock().map_err(|e| SecretError::Backend(e.to_string()))?;
        keys.insert(provider, key.trim().to_string());
        Ok(())
    }

    fn delete(&self, provider: ProviderKind) -> Result<(), SecretError> {
        let mut keys = self.keys.lock().map_err(|e| SecretError::Backend(e.to_string()))?;
        keys.remove(&provider);
        Ok(())
    }
}

/// Shows enough of a key to recognise it: `gsk_••••1234`.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "••••".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}••••{tail}")
}

/// Whether `text` looks like an API key of `provider` (used to pick a key up from the
/// clipboard during onboarding; anything else is never shown or stored).
pub fn looks_like_key(provider: crate::settings::ProviderKind, text: &str) -> bool {
    use crate::settings::ProviderKind;
    let t = text.trim();
    let body_ok = |rest: &str, len: std::ops::RangeInclusive<usize>| {
        len.contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    match provider {
        ProviderKind::Gemini => t.strip_prefix("AIza").is_some_and(|r| body_ok(r, 35..=35)),
        ProviderKind::Groq => t.strip_prefix("gsk_").is_some_and(|r| body_ok(r, 20..=200)),
        ProviderKind::OpenAi => t.strip_prefix("sk-").is_some_and(|r| body_ok(r, 20..=300)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_long_key() {
        assert_eq!(mask("gsk_abcdefghijkl1234"), "gsk_••••1234");
    }

    #[test]
    fn mask_short_key() {
        assert_eq!(mask("abc"), "••••");
        assert_eq!(mask("abcdefgh"), "••••");
    }

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryStore::default();
        assert_eq!(store.get(ProviderKind::Groq), None);
        store.set(ProviderKind::Groq, "gsk_x").unwrap();
        assert_eq!(store.get(ProviderKind::Groq).as_deref(), Some("gsk_x"));
        assert_eq!(store.get(ProviderKind::OpenAi), None);
        store.delete(ProviderKind::Groq).unwrap();
        assert_eq!(store.get(ProviderKind::Groq), None);
    }

    #[test]
    fn set_trims_whitespace() {
        let store = MemoryStore::default();
        store.set(ProviderKind::Groq, " gsk_x \n").unwrap();
        assert_eq!(store.get(ProviderKind::Groq).as_deref(), Some("gsk_x"));
    }

    #[test]
    #[ignore = "touches the real macOS Keychain"]
    fn keychain_roundtrip() {
        let store = KeychainStore::with_service("app.utter.desktop.test");
        store.set(ProviderKind::Groq, " gsk_test_value ").unwrap();
        assert_eq!(store.get(ProviderKind::Groq).as_deref(), Some("gsk_test_value"));
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("security")
                .args(["find-generic-password", "-s", "app.utter.desktop.test", "-a", "groq"])
                .output()
                .unwrap();
            assert!(out.status.success(), "item must be in the real Keychain");
        }
        store.delete(ProviderKind::Groq).unwrap();
        assert_eq!(store.get(ProviderKind::Groq), None);
    }

    #[test]
    fn gemini_key_pattern() {
        use crate::settings::ProviderKind;
        let key = format!("AIza{}", &"Sy1_-abcdefghijABCDEFGHIJ0123456789"[..35]);
        assert_eq!(key.len(), 39);
        assert!(looks_like_key(ProviderKind::Gemini, &key));
        assert!(looks_like_key(ProviderKind::Gemini, &format!("  {key}\n")), "surrounding whitespace is fine");
        assert!(!looks_like_key(ProviderKind::Gemini, &key[..38]));
        assert!(looks_like_key(ProviderKind::Groq, "gsk_abcdefghijklmnopqrstuvwxyz0123"));
        assert!(looks_like_key(ProviderKind::OpenAi, "sk-proj-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn other_text_is_not_a_key() {
        use crate::settings::ProviderKind;
        for t in ["", "hello", "AIza with spaces inside the copied text here", "Привет, можешь скинуть отчёт?", "gsk_short"] {
            for p in [ProviderKind::Gemini, ProviderKind::Groq, ProviderKind::OpenAi] {
                assert!(!looks_like_key(p, t), "{p:?} {t}");
            }
        }
        let gemini = format!("AIza{}", "a".repeat(35));
        assert!(!looks_like_key(ProviderKind::Groq, &gemini), "a Gemini key is not a Groq key");
    }
}
