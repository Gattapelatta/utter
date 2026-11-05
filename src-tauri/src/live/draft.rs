//! What the field shows while speaking and what is committed on release (spec 2026-11-04 §5.2).

use std::time::{Duration, Instant};

use serde::Serialize;

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
}

fn join<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts.map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

impl DraftComposer {
    pub fn on_interim(&mut self, text: &str) {
        self.current = text.to_string();
    }

    pub fn on_segment(&mut self, text: &str) {
        let draft = std::mem::take(&mut self.current);
        // A piece that never had a draft shows its final text.
        self.frozen.push(if draft.trim().is_empty() { text.to_string() } else { draft });
        self.segments.push(text.to_string());
    }

    pub fn draft(&self) -> String {
        join(self.frozen.iter().map(String::as_str).chain(std::iter::once(self.current.as_str())))
    }

    pub fn parts(&self) -> DraftParts {
        DraftParts { settled: join(self.frozen.iter().map(String::as_str)), interim: self.current.trim().to_string() }
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
