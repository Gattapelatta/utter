//! Per-day request counters, reset when the local date changes.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::settings::write_atomic;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageDay {
    pub date: NaiveDate,
    pub stt_requests: u32,
    pub audio_secs: f32,
    pub llm_requests: u32,
    pub llm_tokens: u32,
}

impl UsageDay {
    fn empty(date: NaiveDate) -> Self {
        Self { date, stt_requests: 0, audio_secs: 0.0, llm_requests: 0, llm_tokens: 0 }
    }
}

pub struct Usage {
    path: PathBuf,
    day: Option<UsageDay>,
}

impl Usage {
    pub fn load(path: &Path) -> Self {
        let day = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok());
        Self { path: path.to_path_buf(), day }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&self.day).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes)
    }

    fn current(&mut self, today: NaiveDate) -> &mut UsageDay {
        if self.day.as_ref().map(|d| d.date) != Some(today) {
            self.day = Some(UsageDay::empty(today));
        }
        self.day.get_or_insert_with(|| UsageDay::empty(today))
    }

    pub fn record_stt(&mut self, today: NaiveDate, audio_secs: f32) {
        let d = self.current(today);
        d.stt_requests += 1;
        d.audio_secs += audio_secs;
    }

    pub fn record_llm(&mut self, today: NaiveDate, tokens: u32) {
        let d = self.current(today);
        d.llm_requests += 1;
        d.llm_tokens += tokens;
    }

    /// One more speech request for audio already counted (a recording sent in pieces).
    pub fn count_stt_request(&mut self, today: NaiveDate) {
        self.current(today).stt_requests += 1;
    }

    /// Tokens from a request already counted elsewhere (one-request audio→text).
    pub fn record_tokens(&mut self, today: NaiveDate, tokens: u32) {
        self.current(today).llm_tokens += tokens;
    }

    pub fn today(&self, today: NaiveDate) -> UsageDay {
        self.day.clone().filter(|d| d.date == today).unwrap_or_else(|| UsageDay::empty(today))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn usage_accumulates_same_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut u = Usage::load(&dir.path().join("u.json"));
        u.record_stt(d(6), 10.5);
        u.record_stt(d(6), 2.0);
        u.record_llm(d(6), 300);
        let t = u.today(d(6));
        assert_eq!((t.stt_requests, t.llm_requests, t.llm_tokens), (2, 1, 300));
        assert!((t.audio_secs - 12.5).abs() < 1e-4);
    }

    #[test]
    fn record_tokens_does_not_count_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut u = Usage::load(&dir.path().join("u.json"));
        u.record_tokens(d(6), 40);
        let t = u.today(d(6));
        assert_eq!((t.llm_requests, t.llm_tokens), (0, 40));
    }

    #[test]
    fn usage_resets_on_new_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut u = Usage::load(&dir.path().join("u.json"));
        u.record_stt(d(6), 10.0);
        assert_eq!(u.today(d(7)).stt_requests, 0);
        u.record_llm(d(7), 5);
        assert_eq!(u.today(d(7)).llm_tokens, 5);
        assert_eq!(u.today(d(7)).stt_requests, 0);
    }

    #[test]
    fn usage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("u.json");
        let mut u = Usage::load(&path);
        u.record_stt(d(6), 3.0);
        u.save().unwrap();
        assert_eq!(Usage::load(&path).today(d(6)).stt_requests, 1);
    }
}
