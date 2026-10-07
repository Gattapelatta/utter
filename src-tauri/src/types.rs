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
