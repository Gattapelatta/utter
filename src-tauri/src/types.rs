use serde::{Deserialize, Serialize};

/// What a recording is for: plain dictation or translation to English.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    Dictation,
    Translate,
    /// Speech turned into a task for an AI agent (spec 2026-11-09 §3.7): the dictation path with
    /// its own prompt.
    Task,
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
    LeftAlt,
    LeftMeta,
    LeftCtrl,
    LeftShift,
    CapsLock,
    Fn,
    F(u8),
    Escape,
    Raw(u32),
}

impl KeyId {
    /// Any key can be a hotkey but Escape, which cancels (the user 2026-11-10: «каждую клавишу»).
    pub fn is_assignable(&self) -> bool {
        !matches!(self, KeyId::Escape)
    }

    /// A key that types text (a letter, a digit, space…): as a hotkey it stops typing, so the
    /// router swallows it and the settings warn about it.
    pub fn types_text(&self) -> bool {
        matches!(self, KeyId::Raw(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_but_escape_is_assignable() {
        // The user 2026-11-10: «каждую клавишу можно поставить» — a MacBook has no right ⌃.
        for k in [KeyId::LeftCtrl, KeyId::LeftAlt, KeyId::LeftMeta, KeyId::LeftShift, KeyId::Raw(0), KeyId::Raw(49)] {
            assert!(k.is_assignable(), "{k:?}");
        }
        assert!(!KeyId::Escape.is_assignable());
        // Letters and the like stop typing while they are a hotkey: the UI warns about them.
        assert!(KeyId::Raw(0).types_text());
        assert!(!KeyId::LeftCtrl.types_text());
        assert!(!KeyId::F(5).types_text());
    }

    #[test]
    #[ignore = "replaced by every_key_but_escape_is_assignable (2026-11-10)"]
    fn letters_and_escape_are_not_assignable() {
        for k in [KeyId::Raw(0), KeyId::Escape, KeyId::Raw(55)] {
            assert!(!k.is_assignable(), "{k:?}");
        }
        for k in [KeyId::RightAlt, KeyId::RightMeta, KeyId::RightCtrl, KeyId::RightShift, KeyId::CapsLock, KeyId::Fn, KeyId::F(13)] {
            assert!(k.is_assignable(), "{k:?}");
        }
    }
}
