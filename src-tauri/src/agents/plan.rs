//! An agent's task list, kept by Utter (spec 2026-11-13 §3.3): Claude Code sends the whole list
//! only now and then; between those it sends marks — «next step», «step 2 done», «all done».
//! Pure: no time, no system.

use serde::Serialize;

/// Longest list kept; longer lists are cut (no real plan is that long).
pub const MAX_STEPS: usize = 50;
/// Longest step title kept, in characters.
pub const MAX_TITLE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanStep {
    pub title: String,
    pub status: PlanStatus,
}

/// What one plan event does. Step numbers count from 1, as the agent sends them.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanUpdate {
    /// The whole list (`TodoWrite`, the desktop app's plan tool with `items`).
    Replace(Vec<PlanStep>),
    /// The current step is done, the next one starts.
    Advance,
    /// Steps `done` are done; step `start` becomes the current one.
    Mark { done: Vec<u32>, start: Option<u32> },
    /// The work is wrapped up: every step is done.
    Finish,
}

pub fn apply(plan: &mut Vec<PlanStep>, update: PlanUpdate) {
    let index = |n: u32, len: usize| (n as usize).checked_sub(1).filter(|&i| i < len);
    match update {
        PlanUpdate::Replace(steps) => *plan = steps,
        PlanUpdate::Advance => {
            let active = plan.iter().position(|s| s.status == PlanStatus::Active);
            if let Some(a) = active {
                plan[a].status = PlanStatus::Done;
            }
            let from = active.map_or(0, |a| a + 1);
            if let Some(next) = plan.iter_mut().skip(from).find(|s| s.status == PlanStatus::Pending) {
                next.status = PlanStatus::Active;
            }
        }
        PlanUpdate::Mark { done, start } => {
            let len = plan.len();
            for i in done.into_iter().filter_map(|n| index(n, len)) {
                plan[i].status = PlanStatus::Done;
            }
            if let Some(k) = start.and_then(|n| index(n, len)) {
                // The app keeps one current step: an earlier one not marked done goes back to waiting.
                for s in plan.iter_mut().filter(|s| s.status == PlanStatus::Active) {
                    s.status = PlanStatus::Pending;
                }
                plan[k].status = PlanStatus::Active;
            }
        }
        PlanUpdate::Finish => plan.iter_mut().for_each(|s| s.status = PlanStatus::Done),
    }
}

/// A plan whose steps are all done (an empty plan is not finished: there is nothing).
pub fn finished(plan: &[PlanStep]) -> bool {
    !plan.is_empty() && plan.iter().all(|s| s.status == PlanStatus::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// «dap» → done, active, pending; titles «1», «2», «3».
    fn p(s: &str) -> Vec<PlanStep> {
        s.chars()
            .enumerate()
            .map(|(i, c)| PlanStep {
                title: (i + 1).to_string(),
                status: match c {
                    'd' => PlanStatus::Done,
                    'a' => PlanStatus::Active,
                    _ => PlanStatus::Pending,
                },
            })
            .collect()
    }

    fn st(plan: &[PlanStep]) -> String {
        plan.iter()
            .map(|s| match s.status {
                PlanStatus::Done => 'd',
                PlanStatus::Active => 'a',
                PlanStatus::Pending => 'p',
            })
            .collect()
    }

    fn after(s: &str, u: PlanUpdate) -> String {
        let mut v = p(s);
        apply(&mut v, u);
        st(&v)
    }

    #[test]
    fn replace_takes_the_new_list() {
        assert_eq!(after("dd", PlanUpdate::Replace(p("ap"))), "ap");
    }

    #[test]
    fn advance_completes_active_and_starts_next_pending() {
        assert_eq!(after("dap", PlanUpdate::Advance), "dda");
    }

    #[test]
    fn advance_without_active_starts_first_pending() {
        assert_eq!(after("dpp", PlanUpdate::Advance), "dap");
    }

    #[test]
    fn advance_on_last_step_completes_it() {
        assert_eq!(after("da", PlanUpdate::Advance), "dd");
    }

    #[test]
    fn mark_out_of_order() {
        assert_eq!(after("papp", PlanUpdate::Mark { done: vec![2], start: Some(4) }), "pdpa");
    }

    #[test]
    fn start_leaves_one_active() {
        assert_eq!(after("ap", PlanUpdate::Mark { done: vec![], start: Some(2) }), "pa");
    }

    #[test]
    fn mark_ignores_numbers_outside_the_list() {
        assert_eq!(after("ap", PlanUpdate::Mark { done: vec![0, 3], start: Some(9) }), "ap");
    }

    #[test]
    fn finish_completes_everything() {
        let mut v = p("dap");
        apply(&mut v, PlanUpdate::Finish);
        assert_eq!(st(&v), "ddd");
        assert!(finished(&v));
        assert!(!finished(&p("dap")));
    }

    #[test]
    fn marks_without_a_plan_do_nothing() {
        for u in [PlanUpdate::Advance, PlanUpdate::Finish, PlanUpdate::Mark { done: vec![1], start: Some(1) }] {
            let mut v = vec![];
            apply(&mut v, u);
            assert!(v.is_empty());
        }
        assert!(!finished(&[]));
    }

    #[test]
    fn status_serializes_for_the_page() {
        assert_eq!(serde_json::to_value(PlanStatus::Active).unwrap(), "active");
        let step = PlanStep { title: "x".into(), status: PlanStatus::Done };
        assert_eq!(serde_json::to_value(step).unwrap(), serde_json::json!({"title":"x","status":"done"}));
    }
}
