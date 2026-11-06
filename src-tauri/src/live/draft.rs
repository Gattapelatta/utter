//! What the field shows while speaking and what is committed on release (spec 2026-11-04 §5.2).

use std::time::{Duration, Instant};

use serde::Serialize;

/// The island's field shows about 6 lines (~300 characters); a 10-minute draft is 10–20 KB, so
/// only its tail travels to the page (display only — the final text keeps everything).
pub const DRAFT_TAIL_CHARS: usize = 800;

/// The last `max` characters of `text`, starting at a word.
fn tail(text: String, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text;
    }
    let cut: String = text.chars().skip(n - max).collect();
    match cut.find(' ') {
        Some(i) => cut[i + 1..].to_string(),
        None => cut,
    }
}

/// What the island shows: recognised pieces (white) and the piece still being revised (grey).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DraftParts {
    pub settled: String,
    pub interim: String,
}

/// The draft never swaps a piece's draft for its final text mid-speech (the field would
/// "rewrite itself"); finals are used only for the committed text.
#[derive(Default)]
pub struct DraftComposer {
    /// The last draft of each finished piece.
    frozen: Vec<String>,
    segments: Vec<String>,
    current: String,
    /// Leading words of `current` that the last update did not change (shown white).
    stable: usize,
}

/// Words compared without case and punctuation: "привет" and "Привет," are the same word.
fn norm_words(text: &str) -> Vec<String> {
    text.split_whitespace().map(|w| w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()).collect()
}

fn join<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts.map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

impl DraftComposer {
    /// Google sends the whole current piece as one interim until a pause, so a word turns white
    /// once an update leaves it unchanged; the last word, still being heard, stays grey.
    pub fn on_interim(&mut self, text: &str) {
        let (old, new) = (norm_words(&self.current), norm_words(text));
        let common = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        self.stable = common.min(new.len().saturating_sub(1));
        self.current = text.to_string();
    }

    pub fn on_segment(&mut self, text: &str) {
        let draft = std::mem::take(&mut self.current);
        self.stable = 0;
        // A piece that never had a draft shows its final text.
        self.frozen.push(if draft.trim().is_empty() { text.to_string() } else { draft });
        self.segments.push(text.to_string());
    }

    pub fn draft(&self) -> String {
        join(self.frozen.iter().map(String::as_str).chain(std::iter::once(self.current.as_str())))
    }

    pub fn parts(&self) -> DraftParts {
        let words: Vec<&str> = self.current.split_whitespace().collect();
        let (heard, hearing) = words.split_at(self.stable.min(words.len()));
        DraftParts {
            settled: tail(join(self.frozen.iter().map(String::as_str).chain(heard.iter().copied())), DRAFT_TAIL_CHARS),
            interim: hearing.join(" "),
        }
    }

    /// Final pieces, plus the last draft when its final never came (§4.3).
    pub fn final_text(&self) -> String {
        join(self.segments.iter().map(String::as_str).chain(std::iter::once(self.current.as_str())))
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }
}

pub const DRAFT_EVERY: Duration = Duration::from_millis(100);

/// The field is updated at most every 100 ms and only when the draft changed.
#[derive(Default)]
pub struct DraftThrottle {
    last: Option<Instant>,
    shown: String,
}

impl DraftThrottle {
    pub fn due(&mut self, now: Instant, draft: &str) -> bool {
        if draft == self.shown || self.last.is_some_and(|t| now.saturating_duration_since(t) < DRAFT_EVERY) {
            return false;
        }
        self.last = Some(now);
        self.shown = draft.to_string();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_split_settled_and_interim() {
        let mut c = DraftComposer::default();
        c.on_interim("а б");
        c.on_segment("А б.");
        c.on_interim("в");
        assert_eq!(c.parts(), DraftParts { settled: "а б".into(), interim: "в".into() });
        assert_eq!(c.draft(), "а б в");
    }

    #[test]
    fn interim_words_settle_once_unchanged() {
        // Google sends the whole current piece as one interim until a pause or release.
        let mut c = DraftComposer::default();
        c.on_interim("привет");
        assert_eq!(c.parts(), DraftParts { settled: "".into(), interim: "привет".into() });
        c.on_interim("привет слушай я");
        assert_eq!(c.parts(), DraftParts { settled: "привет".into(), interim: "слушай я".into() });
        c.on_interim("Привет, слушай, я посмотрела");
        assert_eq!(c.parts(), DraftParts { settled: "Привет, слушай, я".into(), interim: "посмотрела".into() });
    }

    #[test]
    fn revised_word_goes_back_to_grey() {
        let mut c = DraftComposer::default();
        c.on_interim("отчёт по продам");
        c.on_interim("отчёт по продам за");
        assert_eq!(c.parts().settled, "отчёт по продам");
        c.on_interim("отчёт по продажам за сентябрь");
        assert_eq!(c.parts(), DraftParts { settled: "отчёт по".into(), interim: "продажам за сентябрь".into() });
    }

    #[test]
    fn the_last_word_stays_grey() {
        let mut c = DraftComposer::default();
        c.on_interim("привет мир");
        c.on_interim("привет мир");
        assert_eq!(c.parts(), DraftParts { settled: "привет".into(), interim: "мир".into() });
    }

    #[test]
    fn long_draft_sends_only_its_tail() {
        let mut c = DraftComposer::default();
        for i in 0..400 {
            c.on_interim(&format!("слово{i}"));
            c.on_segment(&format!("Слово{i}."));
        }
        c.on_interim("последнее");
        let p = c.parts();
        assert!(p.settled.chars().count() <= DRAFT_TAIL_CHARS, "{}", p.settled.chars().count());
        assert!(p.settled.ends_with("слово399"), "{}", &p.settled[p.settled.len().saturating_sub(40)..]);
        assert!(!p.settled.starts_with(' ') && p.settled.split(' ').next().unwrap().starts_with("слово"), "cut at a word boundary");
        assert_eq!(p.interim, "последнее");
        assert!(c.final_text().starts_with("Слово0."), "the final text keeps everything");
    }

    #[test]
    fn parts_empty_by_default() {
        assert_eq!(DraftComposer::default().parts(), DraftParts::default());
    }

    #[test]
    fn draft_parts_serialize() {
        let v = serde_json::to_value(DraftParts { settled: "x".into(), interim: "y".into() }).unwrap();
        assert_eq!(v, serde_json::json!({"settled": "x", "interim": "y"}));
    }

    #[test]
    fn draft_grows_with_interims() {
        let mut c = DraftComposer::default();
        c.on_interim("Привет");
        assert_eq!(c.draft(), "Привет");
        c.on_interim("Привет, слушай");
        assert_eq!(c.draft(), "Привет, слушай");
        c.on_segment("Привет. Слушай.");
        c.on_interim("я посмотрела");
        assert_eq!(c.draft(), "Привет, слушай я посмотрела");
    }

    #[test]
    fn segment_freezes_its_draft_not_its_final() {
        let mut c = DraftComposer::default();
        c.on_interim("отчёт по продажам");
        c.on_segment("Отчёт по продам.");
        assert_eq!(c.draft(), "отчёт по продажам");
        assert_eq!(c.final_text(), "Отчёт по продам.");
        assert_eq!(c.segments(), ["Отчёт по продам.".to_string()]);
    }

    #[test]
    fn final_includes_tail_without_segment() {
        let mut c = DraftComposer::default();
        c.on_interim("первый");
        c.on_segment("Первый.");
        c.on_interim("и хвост");
        assert_eq!(c.final_text(), "Первый. и хвост");
        let mut empty = DraftComposer::default();
        empty.on_segment("Без черновика.");
        assert_eq!(empty.draft(), "Без черновика.");
        assert_eq!(DraftComposer::default().final_text(), "");
    }

    #[test]
    fn throttle_limits_to_100ms_and_changes() {
        let t0 = Instant::now();
        let mut t = DraftThrottle::default();
        assert!(t.due(t0, "а"));
        assert!(!t.due(t0 + Duration::from_millis(50), "аб"), "too soon");
        assert!(t.due(t0 + Duration::from_millis(100), "аб"));
        assert!(!t.due(t0 + Duration::from_millis(300), "аб"), "unchanged");
        assert!(t.due(t0 + Duration::from_millis(300), "абв"));
        assert!(t.due(t0 + Duration::from_millis(500), ""), "an emptied draft is a change");
    }
}
