//! The last 50 results, newest first. Keeps audio only for failed entries (max 5).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::write_atomic;
use crate::types::Mode;

pub const MAX_ENTRIES: usize = 50;
pub const MAX_FAILED_AUDIO: usize = 5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryStatus {
    Done,
    Degraded,
    NothingHeard,
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub created_at_ms: i64,
    pub mode: Mode,
    pub text: String,
    pub raw: String,
    pub status: EntryStatus,
    pub audio_file: Option<PathBuf>,
}

pub struct History {
    path: PathBuf,
    entries: VecDeque<HistoryEntry>,
}

impl History {
    pub fn load(path: &Path) -> Self {
        let entries = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path: path.to_path_buf(), entries }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&self.entries).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes)
    }

    /// Adds an entry; returns audio files the caller must delete.
    pub fn push(&mut self, e: HistoryEntry) -> Vec<PathBuf> {
        self.entries.push_front(e);
        let mut removed = Vec::new();
        while self.entries.len() > MAX_ENTRIES {
            if let Some(old) = self.entries.pop_back() {
                removed.extend(old.audio_file);
            }
        }
        removed.extend(self.trim_failed_audio());
        removed
    }

    /// Edits an entry; audio of entries that are no longer failed is released.
    pub fn update(&mut self, id: u64, f: impl FnOnce(&mut HistoryEntry)) -> Vec<PathBuf> {
        let Some(e) = self.entries.iter_mut().find(|e| e.id == id) else {
            return vec![];
        };
        f(e);
        let mut removed = Vec::new();
        if !matches!(e.status, EntryStatus::Failed { .. }) {
            removed.extend(e.audio_file.take());
        }
        removed.extend(self.trim_failed_audio());
        removed
    }

    fn trim_failed_audio(&mut self) -> Vec<PathBuf> {
        let mut kept = 0;
        let mut removed = Vec::new();
        for e in self.entries.iter_mut() {
            if e.audio_file.is_some() {
                kept += 1;
                if kept > MAX_FAILED_AUDIO {
                    removed.extend(e.audio_file.take());
                }
            }
        }
        removed
    }

    pub fn entries(&self) -> &VecDeque<HistoryEntry> {
        &self.entries
    }

    pub fn get(&self, id: u64) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn clear(&mut self) -> Vec<PathBuf> {
        self.entries.drain(..).filter_map(|e| e.audio_file).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(id: u64, status: EntryStatus, audio: bool) -> HistoryEntry {
        HistoryEntry {
            id,
            created_at_ms: id as i64,
            mode: Mode::Dictation,
            text: format!("t{id}"),
            raw: format!("r{id}"),
            status,
            audio_file: audio.then(|| PathBuf::from(format!("/a/{id}.wav"))),
        }
    }

    fn failed() -> EntryStatus {
        EntryStatus::Failed { message: "network".into() }
    }

    #[test]
    fn history_caps_at_50_and_returns_evicted_audio() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(&dir.path().join("h.json"));
        assert!(h.push(entry(1, failed(), true)).is_empty());
        for id in 2..=50 {
            assert!(h.push(entry(id, EntryStatus::Done, false)).is_empty());
        }
        let removed = h.push(entry(51, EntryStatus::Done, false));
        assert_eq!(removed, vec![PathBuf::from("/a/1.wav")]);
        assert_eq!(h.entries().len(), MAX_ENTRIES);
        assert_eq!(h.entries().front().map(|e| e.id), Some(51));
    }

    #[test]
    fn history_keeps_audio_for_last_5_failed_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(&dir.path().join("h.json"));
        for id in 1..=5 {
            assert!(h.push(entry(id, failed(), true)).is_empty());
        }
        assert_eq!(h.push(entry(6, failed(), true)), vec![PathBuf::from("/a/1.wav")]);
        assert_eq!(h.get(1).and_then(|e| e.audio_file.clone()), None);
        assert!(h.get(6).unwrap().audio_file.is_some());
    }

    #[test]
    fn history_update_success_returns_audio_to_delete() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(&dir.path().join("h.json"));
        h.push(entry(1, failed(), true));
        let removed = h.update(1, |e| {
            e.status = EntryStatus::Done;
            e.text = "fixed".into();
        });
        assert_eq!(removed, vec![PathBuf::from("/a/1.wav")]);
        assert_eq!(h.get(1).unwrap().text, "fixed");
        assert_eq!(h.get(1).unwrap().audio_file, None);
    }

    #[test]
    fn history_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.json");
        let mut h = History::load(&path);
        h.push(entry(1, EntryStatus::Degraded, false));
        h.push(entry(2, failed(), true));
        h.save().unwrap();
        let h2 = History::load(&path);
        assert_eq!(h2.entries(), h.entries());
    }

    #[test]
    fn clear_returns_all_audio() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(&dir.path().join("h.json"));
        h.push(entry(1, failed(), true));
        h.push(entry(2, EntryStatus::Done, false));
        assert_eq!(h.clear(), vec![PathBuf::from("/a/1.wav")]);
        assert!(h.entries().is_empty());
    }
}
