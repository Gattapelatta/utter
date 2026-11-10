//! What the Drop knows about agent sessions: their state, the cards waiting to be shown, the pause.
//! Pure: time comes in as `now`, nothing here talks to the system.

use crate::agents::event::{Agent, AgentEvent, EventKind, SessionKey};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// How long «готово» / «ждёт» stays open once it is on screen.
pub const DONE_CARD: Duration = Duration::from_secs(6);
/// «Пока была пауза: …».
pub const AFTER_PAUSE_CARD: Duration = Duration::from_secs(4);
/// A working session with no events this long has crashed or been closed.
pub const STALE: Duration = Duration::from_secs(30 * 60);
/// Last actions kept per session.
const ACTIONS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    // Declaration order is the order of rows: the most urgent first.
    Permission,
    Waiting,
    Working,
    Done,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentRow {
    pub key: SessionKey,
    pub agent: Agent,
    pub project: String,
    pub state: SessionState,
    pub steps: Option<(u32, u32)>,
    /// At most two, the newest last.
    pub actions: Vec<String>,
    pub summary: Option<String>,
    /// The user has looked at this finished session (opened the agent or pressed OK).
    pub seen: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Card {
    /// `rule`: what «Всегда» allows from now on, in words (shown beside the button).
    Permission { id: u64, key: SessionKey, tool: String, detail: String, can_always: bool, rule: Option<String> },
    Done { key: SessionKey },
    Waiting { key: SessionKey },
    AfterPause { done: u32, waiting: u32 },
}

/// Claude Code's suggested rule, readable: `Bash(npm run:*)`, `mode: acceptEdits`, directories,
/// or the suggestion itself.
fn rule_text(s: &serde_json::Value) -> String {
    let str_of = |v: &serde_json::Value, k: &str| v.get(k).and_then(serde_json::Value::as_str).map(str::to_string);
    match str_of(s, "type").as_deref() {
        Some("addRules" | "replaceRules") => {
            let rules: Vec<String> = s
                .get("rules")
                .and_then(serde_json::Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| match (str_of(r, "toolName"), str_of(r, "ruleContent")) {
                            (Some(t), Some(c)) => format!("{t}({c})"),
                            (Some(t), None) => t,
                            _ => r.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            if rules.is_empty() { s.to_string() } else { rules.join(", ") }
        }
        Some("setMode") => format!("mode: {}", str_of(s, "mode").unwrap_or_default()),
        Some("addDirectories") => s
            .get("directories")
            .and_then(serde_json::Value::as_array)
            .map(|d| d.iter().filter_map(serde_json::Value::as_str).collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| s.to_string()),
        _ => s.to_string(),
    }
}

impl Card {
    fn key(&self) -> Option<&SessionKey> {
        match self {
            Card::Permission { key, .. } | Card::Done { key } | Card::Waiting { key } => Some(key),
            Card::AfterPause { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentView {
    pub rows: Vec<AgentRow>,
    pub card: Option<Card>,
    pub paused: bool,
    /// An agent is switched on in Utter (the right ear keeps its colours; set by the service).
    pub connected: bool,
}

struct Session {
    row: AgentRow,
    last_event: Instant,
    /// Larger = more recent; ties in urgency show the most recent first.
    seq: u64,
}

#[derive(Default)]
pub struct Registry {
    sessions: HashMap<SessionKey, Session>,
    /// Permission requests come before everything else, in arrival order.
    permissions: VecDeque<Card>,
    /// «готово», «ждёт», «пока была пауза», in arrival order.
    notices: VecDeque<Card>,
    /// The card on screen and since when (the show time counts from here).
    shown: Option<(Card, Instant)>,
    paused: bool,
    /// Sessions that finished or started waiting during the pause.
    missed: HashSet<SessionKey>,
    next_id: u64,
    seq: u64,
}

impl Registry {
    fn top(&self) -> Option<&Card> {
        self.permissions.front().or(self.notices.front())
    }

    fn push_notice(&mut self, card: Card) {
        let key = card.key().cloned();
        self.notices.retain(|c| c.key() != key.as_ref());
        self.notices.push_back(card);
    }

    fn drop_notices_of(&mut self, key: &SessionKey) {
        self.notices.retain(|c| c.key() != Some(key));
    }

    /// Takes one event in. Returns the id of a new permission request (its card is queued).
    pub fn apply(&mut self, ev: AgentEvent, now: Instant) -> Option<u64> {
        let key = ev.key.clone();
        if ev.kind == EventKind::Ended {
            self.sessions.remove(&key);
            self.drop_notices_of(&key);
            self.permissions.retain(|c| c.key() != Some(&key));
            return None;
        }
        self.seq += 1;
        let session = self.sessions.entry(key.clone()).or_insert_with(|| Session {
            row: AgentRow {
                key: key.clone(),
                agent: key.agent,
                project: String::new(),
                state: SessionState::Working,
                steps: None,
                actions: Vec::new(),
                summary: None,
                seen: true,
            },
            last_event: now,
            seq: 0,
        });
        session.last_event = now;
        session.seq = self.seq;
        if !ev.project.is_empty() {
            session.row.project = ev.project;
        }
        let row = &mut session.row;
        let working = |row: &mut AgentRow| {
            if row.state != SessionState::Permission {
                row.state = SessionState::Working;
            }
        };
        let mut notice = None;
        let mut permission = None;
        match ev.kind {
            EventKind::Started => {
                row.state = SessionState::Working;
                row.actions.clear();
                row.steps = None;
                row.summary = None;
                row.seen = true;
            }
            EventKind::Action(a) => {
                working(row);
                row.actions.push(a);
                let extra = row.actions.len().saturating_sub(ACTIONS);
                row.actions.drain(..extra);
            }
            EventKind::Steps { done, total } => {
                working(row);
                row.steps = Some((done, total));
            }
            EventKind::Waiting(_) => {
                row.state = SessionState::Waiting;
                notice = Some(Card::Waiting { key: key.clone() });
            }
            EventKind::Permission { tool, detail, suggestion } => {
                if self.paused {
                    // Claude Code asks in its own window: the session waits for the user there.
                    row.state = SessionState::Waiting;
                } else {
                    row.state = SessionState::Permission;
                    self.next_id += 1;
                    permission = Some(Card::Permission {
                        id: self.next_id,
                        key: key.clone(),
                        tool,
                        detail,
                        can_always: suggestion.is_some(),
                        rule: suggestion.as_ref().map(rule_text),
                    });
                }
            }
            EventKind::Finished { .. } => {
                row.state = SessionState::Done;
                row.summary = None;
                row.seen = false;
                notice = Some(Card::Done { key: key.clone() });
            }
            EventKind::Ended => unreachable!("handled above"),
        }
        let state = row.state;
        if state == SessionState::Working {
            self.drop_notices_of(&key);
        }
        if self.paused {
            if matches!(state, SessionState::Done | SessionState::Waiting) {
                self.missed.insert(key);
            }
            return None;
        }
        if let Some(card) = notice {
            self.push_notice(card);
        }
        let id = match &permission {
            Some(Card::Permission { id, .. }) => Some(*id),
            _ => None,
        };
        if let Some(card) = permission {
            self.permissions.push_back(card);
        }
        id
    }

    /// Ids of the session's open permission requests.
    pub fn permission_ids(&self, key: &SessionKey) -> Vec<u64> {
        self.permissions
            .iter()
            .filter_map(|c| match c {
                Card::Permission { id, key: k, .. } if k == key => Some(*id),
                _ => None,
            })
            .collect()
    }

    pub fn set_summary(&mut self, key: &SessionKey, s: Option<String>) {
        if let Some(session) = self.sessions.get_mut(key) {
            session.row.summary = s;
        }
    }

    /// The permission was answered (or timed out): its card goes, the session works again.
    pub fn resolve_permission(&mut self, id: u64) {
        let Some(at) = self.permissions.iter().position(|c| matches!(c, Card::Permission { id: i, .. } if *i == id)) else {
            return;
        };
        let card = self.permissions.remove(at);
        let Some(key) = card.as_ref().and_then(Card::key) else { return };
        let still_asking = self.permissions.iter().any(|c| c.key() == Some(key));
        if let Some(session) = self.sessions.get_mut(key) {
            if session.row.state == SessionState::Permission && !still_asking {
                session.row.state = SessionState::Working;
            }
        }
    }

    /// OK on the card. A permission card is not dismissed this way: only an answer resolves it.
    pub fn dismiss_card(&mut self, _now: Instant) {
        if !self.permissions.is_empty() {
            return;
        }
        if let Some(Card::Done { key }) = self.notices.pop_front() {
            self.mark_seen(&key);
        }
        self.shown = None;
    }

    /// The user opened the agent: its green dot and cards go.
    pub fn mark_seen(&mut self, key: &SessionKey) {
        if let Some(session) = self.sessions.get_mut(key) {
            session.row.seen = true;
        }
        self.drop_notices_of(key);
    }

    /// The card on top went on screen now (a dictation may have held it back until now).
    pub fn card_shown(&mut self, now: Instant) {
        let Some(top) = self.top().cloned() else { return };
        if self.shown.as_ref().map(|(c, _)| c) != Some(&top) {
            self.shown = Some((top, now));
        }
    }

    /// Pause on: no cards, open permission requests are dropped (their ids are returned so the
    /// caller can answer them «no decision»). Pause off: «Пока была пауза…» if anything happened.
    pub fn set_paused(&mut self, on: bool, _now: Instant) -> Vec<u64> {
        if on == self.paused {
            return Vec::new();
        }
        self.paused = on;
        self.shown = None;
        if on {
            self.missed.clear();
            self.notices.clear();
            let dropped: Vec<u64> = self
                .permissions
                .drain(..)
                .filter_map(|c| match c {
                    Card::Permission { id, key, .. } => {
                        if let Some(s) = self.sessions.get_mut(&key) {
                            s.row.state = SessionState::Waiting;
                        }
                        Some(id)
                    }
                    _ => None,
                })
                .collect();
            return dropped;
        }
        let state_of = |k: &SessionKey| self.sessions.get(k).map(|s| s.row.state);
        let done = self.missed.iter().filter(|k| state_of(k) == Some(SessionState::Done)).count() as u32;
        let waiting = self.missed.iter().filter(|k| state_of(k) == Some(SessionState::Waiting)).count() as u32;
        self.missed.clear();
        if done + waiting > 0 {
            self.notices.push_back(Card::AfterPause { done, waiting });
        }
        Vec::new()
    }

    /// Time passes: a shown notice closes after its time, silent working sessions go.
    pub fn tick(&mut self, now: Instant) {
        self.sessions.retain(|_, s| !(s.row.state == SessionState::Working && now.duration_since(s.last_event) >= STALE));
        let Some((card, at)) = self.shown.clone() else { return };
        if self.permissions.is_empty() && self.notices.front() == Some(&card) {
            let lasts = if matches!(card, Card::AfterPause { .. }) { AFTER_PAUSE_CARD } else { DONE_CARD };
            if now.duration_since(at) >= lasts {
                self.notices.pop_front();
                self.shown = None;
            }
        }
    }

    pub fn view(&self) -> AgentView {
        if self.paused {
            return AgentView { rows: Vec::new(), card: None, paused: true, connected: false };
        }
        let mut sessions: Vec<&Session> =
            self.sessions.values().filter(|s| !(s.row.state == SessionState::Done && s.row.seen)).collect();
        sessions.sort_by_key(|s| (s.row.state, std::cmp::Reverse(s.seq)));
        AgentView {
            rows: sessions.into_iter().map(|s| s.row.clone()).collect(),
            card: self.top().cloned(),
            paused: false,
            connected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::event::{Agent, AgentEvent, EventKind, SessionKey};
    use serde_json::json;

    fn key(id: &str) -> SessionKey {
        SessionKey { agent: Agent::ClaudeCode, id: id.into() }
    }

    fn ev(id: &str, kind: EventKind) -> AgentEvent {
        AgentEvent { key: key(id), project: format!("proj-{id}"), kind }
    }

    fn finished(id: &str) -> AgentEvent {
        ev(id, EventKind::Finished { last_message: "done".into() })
    }

    fn permission(id: &str, suggestion: bool) -> AgentEvent {
        ev(
            id,
            EventKind::Permission {
                tool: "Bash".into(),
                detail: "git push".into(),
                suggestion: suggestion.then(|| json!({"type": "addRules"})),
            },
        )
    }

    fn row<'a>(view: &'a AgentView, id: &str) -> &'a AgentRow {
        view.rows.iter().find(|r| r.key.id == id).expect(id)
    }

    #[test]
    fn started_then_finished_shows_done_card() {
        let t = Instant::now();
        let mut r = Registry::default();
        assert_eq!(r.apply(ev("a", EventKind::Started), t), None);
        assert_eq!(row(&r.view(), "a").state, SessionState::Working);
        assert_eq!(r.view().card, None);
        r.apply(finished("a"), t);
        let v = r.view();
        assert_eq!(row(&v, "a").state, SessionState::Done);
        assert_eq!(row(&v, "a").project, "proj-a");
        assert_eq!(v.card, Some(Card::Done { key: key("a") }));
        r.set_summary(&key("a"), Some("Fixed it".into()));
        assert_eq!(row(&r.view(), "a").summary.as_deref(), Some("Fixed it"));
    }

    #[test]
    fn done_card_hides_after_6s_but_row_stays_unseen() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("a"), t);
        // The 6 s count from when the card is on screen (a dictation may hold it back).
        r.tick(t + Duration::from_secs(60));
        assert!(r.view().card.is_some());
        r.card_shown(t + Duration::from_secs(60));
        r.tick(t + Duration::from_secs(65));
        assert!(r.view().card.is_some());
        r.tick(t + Duration::from_secs(66));
        assert_eq!(r.view().card, None);
        let v = r.view();
        assert_eq!(row(&v, "a").state, SessionState::Done);
        assert!(!row(&v, "a").seen);
    }

    #[test]
    fn mark_seen_clears_green() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("a"), t);
        r.mark_seen(&key("a"));
        assert!(r.view().rows.is_empty());
        assert_eq!(r.view().card, None);
    }

    #[test]
    fn ok_on_done_card_marks_seen() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("a"), t);
        r.dismiss_card(t);
        assert_eq!(r.view().card, None);
        assert!(r.view().rows.is_empty());
    }

    #[test]
    fn steps_and_last_two_actions() {
        let t = Instant::now();
        let mut r = Registry::default();
        for a in ["Читает a.rs", "Правит b.rs", "Запускает npm test"] {
            r.apply(ev("a", EventKind::Action(a.into())), t);
        }
        r.apply(ev("a", EventKind::Steps { done: 2, total: 4 }), t);
        let v = r.view();
        assert_eq!(row(&v, "a").actions, vec!["Правит b.rs".to_string(), "Запускает npm test".to_string()]);
        assert_eq!(row(&v, "a").steps, Some((2, 4)));
        assert_eq!(row(&v, "a").state, SessionState::Working);
    }

    #[test]
    fn new_prompt_clears_old_work() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(ev("a", EventKind::Action("Правит b.rs".into())), t);
        r.apply(ev("a", EventKind::Steps { done: 4, total: 4 }), t);
        r.apply(finished("a"), t);
        r.apply(ev("a", EventKind::Started), t);
        let v = r.view();
        assert_eq!(row(&v, "a").state, SessionState::Working);
        assert!(row(&v, "a").actions.is_empty());
        assert_eq!(row(&v, "a").steps, None);
        assert_eq!(v.card, None);
    }

    #[test]
    fn permission_card_has_priority_and_queue() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("a"), t);
        let first = r.apply(permission("b", true), t).unwrap();
        let second = r.apply(permission("c", true), t).unwrap();
        assert_ne!(first, second);
        assert!(matches!(r.view().card, Some(Card::Permission { id, .. }) if id == first));
        assert_eq!(row(&r.view(), "b").state, SessionState::Permission);
        r.resolve_permission(first);
        assert_eq!(row(&r.view(), "b").state, SessionState::Working);
        assert!(matches!(r.view().card, Some(Card::Permission { id, .. }) if id == second));
        // OK does not skip a permission card: only an answer resolves it.
        r.dismiss_card(t);
        assert!(matches!(r.view().card, Some(Card::Permission { id, .. }) if id == second));
        r.resolve_permission(second);
        assert_eq!(r.view().card, Some(Card::Done { key: key("a") }));
    }

    #[test]
    fn can_always_only_with_suggestion() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(permission("a", false), t);
        let Some(Card::Permission { can_always, tool, detail, .. }) = r.view().card else { panic!() };
        assert!(!can_always);
        assert_eq!((tool.as_str(), detail.as_str()), ("Bash", "git push"));
        let mut r = Registry::default();
        r.apply(permission("a", true), t);
        assert!(matches!(r.view().card, Some(Card::Permission { can_always: true, .. })));
    }

    #[test]
    fn always_shows_the_rule_it_saves() {
        // Review 2026-11-10: «Всегда» must say what it allows from now on.
        let t = Instant::now();
        let rule = |s: serde_json::Value| {
            let mut r = Registry::default();
            let ev = ev("a", EventKind::Permission { tool: "Bash".into(), detail: "x".into(), suggestion: Some(s) });
            r.apply(ev, t);
            match r.view().card {
                Some(Card::Permission { rule, .. }) => rule,
                _ => panic!(),
            }
        };
        let add = |content: serde_json::Value| json!({"type":"addRules","rules":[{"toolName":"Bash","ruleContent":content}],"behavior":"allow"});
        assert_eq!(rule(add(json!("npm run:*"))).as_deref(), Some("Bash(npm run:*)"));
        assert_eq!(rule(json!({"type":"addRules","rules":[{"toolName":"WebFetch"}]})).as_deref(), Some("WebFetch"));
        assert_eq!(rule(json!({"type":"setMode","mode":"acceptEdits","destination":"session"})).as_deref(), Some("mode: acceptEdits"));
        assert_eq!(rule(json!({"type":"addDirectories","directories":["/a","/b"]})).as_deref(), Some("/a, /b"));
    }

    #[test]
    fn waiting_notification() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(ev("a", EventKind::Started), t);
        r.apply(ev("a", EventKind::Waiting("Claude is waiting".into())), t);
        let v = r.view();
        assert_eq!(row(&v, "a").state, SessionState::Waiting);
        assert_eq!(v.card, Some(Card::Waiting { key: key("a") }));
        // A second notice for the same session replaces the first, it does not queue.
        r.apply(finished("a"), t);
        assert_eq!(r.view().card, Some(Card::Done { key: key("a") }));
        r.dismiss_card(t);
        assert_eq!(r.view().card, None);
    }

    #[test]
    fn ended_removes_session() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("a"), t);
        r.apply(ev("a", EventKind::Ended), t);
        assert!(r.view().rows.is_empty());
        assert_eq!(r.view().card, None);
    }

    #[test]
    fn stale_working_removed_after_30min() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(ev("a", EventKind::Started), t);
        r.apply(ev("b", EventKind::Waiting("w".into())), t);
        r.tick(t + STALE - Duration::from_secs(1));
        assert_eq!(r.view().rows.len(), 2);
        r.tick(t + STALE);
        let v = r.view();
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].key, key("b"));
    }

    #[test]
    fn paused_hides_cards_and_counts_missed() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(ev("w", EventKind::Started), t);
        r.set_paused(true, t);
        r.apply(finished("a"), t);
        r.apply(finished("b"), t);
        assert_eq!(r.apply(permission("c", true), t), None);
        let v = r.view();
        assert!(v.paused);
        assert!(v.rows.is_empty());
        assert_eq!(v.card, None);
    }

    #[test]
    fn pause_drops_open_permissions() {
        let t = Instant::now();
        let mut r = Registry::default();
        let id = r.apply(permission("a", true), t).unwrap();
        assert_eq!(r.set_paused(true, t), vec![id]);
        assert_eq!(r.set_paused(true, t), Vec::<u64>::new());
        r.set_paused(false, t);
        assert_eq!(row(&r.view(), "a").state, SessionState::Waiting);
        assert_eq!(r.view().card, None);
    }

    #[test]
    fn unpause_shows_after_pause_card_for_4s() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.set_paused(true, t);
        r.apply(finished("a"), t);
        r.apply(finished("b"), t);
        r.apply(permission("c", true), t);
        r.set_paused(false, t);
        let v = r.view();
        assert_eq!(v.card, Some(Card::AfterPause { done: 2, waiting: 1 }));
        assert_eq!(row(&v, "c").state, SessionState::Waiting);
        r.card_shown(t);
        r.tick(t + AFTER_PAUSE_CARD - Duration::from_millis(1));
        assert!(r.view().card.is_some());
        r.tick(t + AFTER_PAUSE_CARD);
        assert_eq!(r.view().card, None);
        // Nothing missed → no card.
        r.set_paused(true, t);
        r.set_paused(false, t);
        assert_eq!(r.view().card, None);
    }

    #[test]
    fn codex_finished_without_started() {
        let t = Instant::now();
        let mut r = Registry::default();
        let k = SessionKey { agent: Agent::Codex, id: "th".into() };
        r.apply(AgentEvent { key: k.clone(), project: "shop".into(), kind: EventKind::Finished { last_message: "x".into() } }, t);
        let v = r.view();
        assert_eq!(v.rows[0].agent, Agent::Codex);
        assert_eq!(v.rows[0].state, SessionState::Done);
        assert_eq!(v.card, Some(Card::Done { key: k }));
    }

    #[test]
    fn rows_sorted_by_urgency() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(finished("done"), t);
        r.apply(ev("work", EventKind::Started), t);
        r.apply(ev("wait", EventKind::Waiting("w".into())), t);
        r.apply(permission("perm", false), t);
        let ids: Vec<String> = r.view().rows.iter().map(|r| r.key.id.clone()).collect();
        assert_eq!(ids, ["perm", "wait", "work", "done"]);
    }

    #[test]
    fn view_serializes_for_the_page() {
        let t = Instant::now();
        let mut r = Registry::default();
        r.apply(permission("a", true), t);
        let v = serde_json::to_value(r.view()).unwrap();
        assert_eq!(v["rows"][0]["state"], "permission");
        assert_eq!(v["rows"][0]["agent"], "claude_code");
        assert_eq!(v["card"]["kind"], "permission");
        assert_eq!(v["card"]["can_always"], true);
        assert_eq!(v["paused"], false);
    }
}
