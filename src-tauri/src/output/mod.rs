//! Puts text into the focused app: clipboard first, then a paste or typed keystrokes.

pub mod clipboard;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod spacing;

use std::time::Duration;

use crate::settings::InsertMethod;
use clipboard::{set_and_verify, Clipboard};

const CLIPBOARD_CONFIRM: Duration = Duration::from_millis(200);
const MODIFIER_RELEASE: Duration = Duration::from_millis(500);
const TYPE_CHUNK_UNITS: usize = 20;
const TYPE_PAUSE: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CopyReason {
    OwnWindow,
    Elevated,
    ClipboardFailed,
    InputFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum DeliveryOutcome {
    Pasted,
    Typed,
    CopiedOnly(CopyReason),
}

/// What precedes the caret in the focused text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaretContext {
    Char(char),
    StartOfField,
    /// The app does not expose its text (some Electron apps, terminals).
    Unknown,
}

pub trait InputPlatform {
    fn caret_context(&self) -> CaretContext;
    fn frontmost_pid(&self) -> Option<i32>;
    fn frontmost_is_self(&self) -> bool;
    fn frontmost_is_elevated(&self) -> bool;
    fn wait_modifiers_released(&self, timeout: Duration);
    fn send_paste(&self) -> Result<(), String>;
    fn type_text(&self, chunk: &str) -> Result<(), String>;
    fn send_shift_enter(&self) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypingChunk {
    Text(String),
    NewLine,
}

pub fn split_for_typing(text: &str) -> Vec<TypingChunk> {
    let normalized = text.replace("\r\n", "\n");
    let mut out = Vec::new();
    for (i, line) in normalized.split('\n').enumerate() {
        if i > 0 {
            out.push(TypingChunk::NewLine);
        }
        // macOS accepts at most 20 UTF-16 units per synthetic key event.
        let mut chunk = String::new();
        let mut units = 0;
        for ch in line.chars() {
            if units + ch.len_utf16() > TYPE_CHUNK_UNITS {
                out.push(TypingChunk::Text(std::mem::take(&mut chunk)));
                units = 0;
            }
            chunk.push(ch);
            units += ch.len_utf16();
        }
        if !chunk.is_empty() {
            out.push(TypingChunk::Text(chunk));
        }
    }
    out
}

/// `fallback_prev` is the last character Utter itself inserted into this app,
/// used only when the app does not expose the text before the caret.
pub fn deliver(
    text: &str,
    method: InsertMethod,
    cb: &mut dyn Clipboard,
    input: &dyn InputPlatform,
    fallback_prev: Option<char>,
) -> DeliveryOutcome {
    let prev = match input.caret_context() {
        CaretContext::Char(c) => Some(c),
        CaretContext::StartOfField => None,
        CaretContext::Unknown => fallback_prev,
    };
    let spaced;
    let text = if spacing::needs_leading_space(prev, text) {
        spaced = format!(" {text}");
        spaced.as_str()
    } else {
        text
    };
    // The text always ends up in the clipboard, whatever happens next.
    if set_and_verify(cb, text, CLIPBOARD_CONFIRM).is_err() {
        return DeliveryOutcome::CopiedOnly(CopyReason::ClipboardFailed);
    }
    if input.frontmost_is_self() {
        return DeliveryOutcome::CopiedOnly(CopyReason::OwnWindow);
    }
    if input.frontmost_is_elevated() {
        return DeliveryOutcome::CopiedOnly(CopyReason::Elevated);
    }
    input.wait_modifiers_released(MODIFIER_RELEASE);
    let result = match method {
        InsertMethod::Paste => input.send_paste().map(|_| DeliveryOutcome::Pasted),
        InsertMethod::Type => type_all(text, input).map(|_| DeliveryOutcome::Typed),
    };
    result.unwrap_or(DeliveryOutcome::CopiedOnly(CopyReason::InputFailed))
}

fn type_all(text: &str, input: &dyn InputPlatform) -> Result<(), String> {
    for (i, chunk) in split_for_typing(text).into_iter().enumerate() {
        if i > 0 {
            std::thread::sleep(TYPE_PAUSE);
        }
        match chunk {
            TypingChunk::Text(t) => input.type_text(&t)?,
            TypingChunk::NewLine => input.send_shift_enter()?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::clipboard::tests::FakeClipboard;
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeInput {
        own: bool,
        elevated: bool,
        fail: bool,
        caret: Option<CaretContext>,
        calls: RefCell<Vec<String>>,
    }

    impl InputPlatform for FakeInput {
        fn caret_context(&self) -> CaretContext {
            self.caret.unwrap_or(CaretContext::Unknown)
        }
        fn frontmost_pid(&self) -> Option<i32> {
            Some(1)
        }
        fn frontmost_is_self(&self) -> bool {
            self.own
        }
        fn frontmost_is_elevated(&self) -> bool {
            self.elevated
        }
        fn wait_modifiers_released(&self, _: Duration) {
            self.calls.borrow_mut().push("wait".into());
        }
        fn send_paste(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("paste".into());
            if self.fail { Err("x".into()) } else { Ok(()) }
        }
        fn type_text(&self, chunk: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("type:{chunk}"));
            if self.fail { Err("x".into()) } else { Ok(()) }
        }
        fn send_shift_enter(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("shift_enter".into());
            Ok(())
        }
    }

    #[test]
    fn split_handles_newlines_and_chunks() {
        assert_eq!(
            split_for_typing("a\nb"),
            vec![TypingChunk::Text("a".into()), TypingChunk::NewLine, TypingChunk::Text("b".into())]
        );
        assert_eq!(split_for_typing("a\r\n\nb").iter().filter(|c| **c == TypingChunk::NewLine).count(), 2);
        let long = "x".repeat(45);
        let lens: Vec<usize> = split_for_typing(&long)
            .iter()
            .map(|c| match c { TypingChunk::Text(t) => t.chars().count(), TypingChunk::NewLine => 0 })
            .collect();
        assert_eq!(lens, vec![20, 20, 5]);
    }

    #[test]
    fn split_chunks_fit_20_utf16_units() {
        let s = "😀".repeat(15) + "абв";
        for c in split_for_typing(&s) {
            if let TypingChunk::Text(t) = c {
                assert!(t.encode_utf16().count() <= 20, "chunk {t:?}");
            }
        }
    }

    #[test]
    fn split_is_char_safe() {
        let s = "привет😀мир".repeat(5);
        let joined: String = split_for_typing(&s)
            .into_iter()
            .map(|c| match c { TypingChunk::Text(t) => t, TypingChunk::NewLine => "\n".into() })
            .collect();
        assert_eq!(joined, s);
    }

    #[test]
    fn paste_flow_order() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput::default();
        assert_eq!(deliver("Привет", InsertMethod::Paste, &mut cb, &input, None), DeliveryOutcome::Pasted);
        assert_eq!(*input.calls.borrow(), vec!["wait".to_string(), "paste".to_string()]);
        assert_eq!(cb.value.as_deref(), Some("Привет"));
    }

    #[test]
    fn own_window_copies_only() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput { own: true, ..Default::default() };
        assert_eq!(deliver("x", InsertMethod::Paste, &mut cb, &input, None), DeliveryOutcome::CopiedOnly(CopyReason::OwnWindow));
        assert!(input.calls.borrow().is_empty());
        assert_eq!(cb.value.as_deref(), Some("x"));
    }

    #[test]
    fn elevated_copies_only() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput { elevated: true, ..Default::default() };
        assert_eq!(deliver("x", InsertMethod::Paste, &mut cb, &input, None), DeliveryOutcome::CopiedOnly(CopyReason::Elevated));
    }

    #[test]
    fn clipboard_failure_reports_it() {
        let mut cb = FakeClipboard::new();
        cb.fail_writes = usize::MAX;
        let input = FakeInput::default();
        assert_eq!(deliver("x", InsertMethod::Paste, &mut cb, &input, None), DeliveryOutcome::CopiedOnly(CopyReason::ClipboardFailed));
        assert!(input.calls.borrow().is_empty());
    }

    #[test]
    fn type_flow_uses_shift_enter() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput::default();
        assert_eq!(deliver("a\nb", InsertMethod::Type, &mut cb, &input, None), DeliveryOutcome::Typed);
        assert_eq!(*input.calls.borrow(), vec!["wait", "type:a", "shift_enter", "type:b"]);
        assert_eq!(cb.value.as_deref(), Some("a\nb"));
    }

    #[test]
    fn input_failure_reports_copied() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput { fail: true, ..Default::default() };
        assert_eq!(deliver("x", InsertMethod::Paste, &mut cb, &input, None), DeliveryOutcome::CopiedOnly(CopyReason::InputFailed));
    }

    #[test]
    fn paste_adds_space_after_previous_sentence() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput { caret: Some(CaretContext::Char('.')), ..Default::default() };
        deliver("Привет", InsertMethod::Paste, &mut cb, &input, None);
        assert_eq!(cb.value.as_deref(), Some(" Привет"));
    }

    #[test]
    fn no_space_at_start_of_field() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput { caret: Some(CaretContext::StartOfField), ..Default::default() };
        deliver("Привет", InsertMethod::Paste, &mut cb, &input, Some('.'));
        assert_eq!(cb.value.as_deref(), Some("Привет"));
    }

    #[test]
    fn unknown_caret_uses_fallback() {
        let mut cb = FakeClipboard::new();
        let input = FakeInput::default();
        deliver("Hi", InsertMethod::Paste, &mut cb, &input, Some('.'));
        assert_eq!(cb.value.as_deref(), Some(" Hi"));
        let mut cb = FakeClipboard::new();
        deliver("Hi", InsertMethod::Paste, &mut cb, &input, None);
        assert_eq!(cb.value.as_deref(), Some("Hi"));
    }
}
