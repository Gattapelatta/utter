//! Smart spacing: a dictated sentence placed right after another one gets a space.

use std::time::{Duration, Instant};

/// How long Utter remembers its last insertion when the app hides the caret context.
pub const RECENT_WINDOW: Duration = Duration::from_secs(120);

/// The last text Utter inserted, for apps that do not expose the text before the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecentInsert {
    pub pid: i32,
    pub at: Instant,
    pub last: char,
}

pub fn needs_leading_space(prev: Option<char>, text: &str) -> bool {
    let (Some(prev), Some(first)) = (prev, text.chars().next()) else {
        return false;
    };
    let opens = matches!(prev, '(' | '[' | '{' | '«' | '“' | '‘' | '"' | '\'' | '/' | '@' | '#');
    let attaches = first.is_whitespace() || matches!(first, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']' | '}' | '»' | '”');
    !prev.is_whitespace() && !opens && !attaches
}

pub fn fallback_prev(recent: Option<&RecentInsert>, pid: Option<i32>, now: Instant) -> Option<char> {
    let r = recent?;
    (Some(r.pid) == pid && now.saturating_duration_since(r.at) <= RECENT_WINDOW).then_some(r.last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_after_sentence_or_word() {
        assert!(needs_leading_space(Some('.'), "Привет"));
        assert!(needs_leading_space(Some('a'), "next"));
        assert!(needs_leading_space(Some('!'), "Hi"));
    }

    #[test]
    fn no_space_where_it_would_be_wrong() {
        assert!(!needs_leading_space(None, "Hi"));
        assert!(!needs_leading_space(Some(' '), "Hi"));
        assert!(!needs_leading_space(Some('\n'), "Hi"));
        assert!(!needs_leading_space(Some('('), "Hi"));
        assert!(!needs_leading_space(Some('«'), "Hi"));
        assert!(!needs_leading_space(Some('a'), ", and more"));
        assert!(!needs_leading_space(Some('a'), " already spaced"));
        assert!(!needs_leading_space(Some('a'), ""));
    }

    #[test]
    fn fallback_uses_recent_insert_in_same_app() {
        let t0 = Instant::now();
        let recent = RecentInsert { pid: 42, at: t0, last: '.' };
        assert_eq!(fallback_prev(Some(&recent), Some(42), t0 + Duration::from_secs(30)), Some('.'));
        assert_eq!(fallback_prev(Some(&recent), Some(7), t0 + Duration::from_secs(30)), None);
        assert_eq!(fallback_prev(Some(&recent), Some(42), t0 + RECENT_WINDOW + Duration::from_secs(1)), None);
        assert_eq!(fallback_prev(Some(&recent), None, t0), None);
        assert_eq!(fallback_prev(None, Some(42), t0), None);
    }
}
