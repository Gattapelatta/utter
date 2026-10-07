use serde::{Deserialize, Serialize};

/// What a recording is for: plain dictation or translation to English.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    Dictation,
    Translate,
}

/// A physical key, named platform-neutrally.
/// `RightAlt` is Right Option on macOS; `RightMeta` is Right Command / Right Win.
/// `Raw` carries the platform keycode of any other key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyId {
    RightAlt,
    RightMeta,
    RightCtrl,
    RightShift,
    CapsLock,
    Fn,
    F(u8),
    Escape,
    Raw(u32),
}

impl KeyId {
    /// Keys that can be a hotkey without getting in the way of typing: right-hand
    /// modifiers, Fn, Caps Lock and function keys. Letters, digits, left modifiers
    /// and Escape (which cancels) cannot.
    pub fn is_assignable(&self) -> bool {
        !matches!(self, KeyId::Escape | KeyId::Raw(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_escape_are_not_assignable() {
        for k in [KeyId::Raw(0), KeyId::Escape, KeyId::Raw(55)] {
            assert!(!k.is_assignable(), "{k:?}");
        }
        for k in [KeyId::RightAlt, KeyId::RightMeta, KeyId::RightCtrl, KeyId::RightShift, KeyId::CapsLock, KeyId::Fn, KeyId::F(13)] {
            assert!(k.is_assignable(), "{k:?}");
        }
    }
}
