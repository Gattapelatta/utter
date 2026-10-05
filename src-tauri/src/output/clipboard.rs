use std::time::{Duration, Instant};

const WRITE_ATTEMPTS: usize = 10;
const POLL: Duration = Duration::from_millis(10);

pub trait Clipboard {
    fn set_text(&mut self, t: &str) -> Result<(), String>;
    fn get_text(&mut self) -> Option<String>;
}

/// System clipboard via `arboard`.
pub struct ArboardClipboard(arboard::Clipboard);

impl ArboardClipboard {
    pub fn new() -> Result<Self, String> {
        arboard::Clipboard::new().map(Self).map_err(|e| e.to_string())
    }
}

impl Clipboard for ArboardClipboard {
    fn set_text(&mut self, t: &str) -> Result<(), String> {
        self.0.set_text(t.to_string()).map_err(|e| e.to_string())
    }
    fn get_text(&mut self) -> Option<String> {
        self.0.get_text().ok()
    }
}

/// Writes `text` and waits until the system reports it back, so a paste
/// never fires before the clipboard actually holds the new text.
pub fn set_and_verify(cb: &mut dyn Clipboard, text: &str, timeout: Duration) -> Result<(), String> {
    let mut last_err = String::new();
    let mut written = false;
    for _ in 0..WRITE_ATTEMPTS {
        match cb.set_text(text) {
            Ok(()) => {
                written = true;
                break;
            }
            Err(e) => {
                last_err = e;
                std::thread::sleep(POLL);
            }
        }
    }
    if !written {
        return Err(format!("clipboard write failed: {last_err}"));
    }
    let deadline = Instant::now() + timeout;
    loop {
        if cb.get_text().as_deref() == Some(text) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("clipboard did not confirm the new text".into());
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Returns the previous value for the first `stale_reads` reads after a write.
    pub(crate) struct FakeClipboard {
        pub value: Option<String>,
        pending: Option<String>,
        pub stale_reads: usize,
        pub fail_writes: usize,
        pub writes: usize,
    }

    impl FakeClipboard {
        pub(crate) fn new() -> Self {
            Self { value: Some("old".into()), pending: None, stale_reads: 0, fail_writes: 0, writes: 0 }
        }
    }

    impl Clipboard for FakeClipboard {
        fn set_text(&mut self, t: &str) -> Result<(), String> {
            self.writes += 1;
            if self.fail_writes > 0 {
                self.fail_writes -= 1;
                return Err("locked".into());
            }
            self.pending = Some(t.to_string());
            Ok(())
        }
        fn get_text(&mut self) -> Option<String> {
            if self.stale_reads > 0 {
                self.stale_reads -= 1;
                return self.value.clone();
            }
            if let Some(p) = self.pending.take() {
                self.value = Some(p);
            }
            self.value.clone()
        }
    }

    #[test]
    fn verify_retries_until_readback_matches() {
        let mut cb = FakeClipboard::new();
        cb.stale_reads = 3;
        assert_eq!(set_and_verify(&mut cb, "new", Duration::from_millis(200)), Ok(()));
        assert_eq!(cb.value.as_deref(), Some("new"));
    }

    #[test]
    fn verify_times_out() {
        let mut cb = FakeClipboard::new();
        cb.stale_reads = usize::MAX;
        assert!(set_and_verify(&mut cb, "new", Duration::from_millis(50)).is_err());
    }

    #[test]
    fn write_is_retried_when_clipboard_is_busy() {
        let mut cb = FakeClipboard::new();
        cb.fail_writes = 3;
        assert_eq!(set_and_verify(&mut cb, "new", Duration::from_millis(200)), Ok(()));
        assert_eq!(cb.writes, 4);
    }

    #[test]
    fn write_gives_up_after_ten_attempts() {
        let mut cb = FakeClipboard::new();
        cb.fail_writes = usize::MAX;
        assert!(set_and_verify(&mut cb, "new", Duration::from_millis(200)).is_err());
        assert_eq!(cb.writes, 10);
    }
}
