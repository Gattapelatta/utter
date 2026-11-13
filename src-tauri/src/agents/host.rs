//! Which app a session runs in (Claude, Terminal, iTerm, VS Code, ChatGPT…): the first ancestor
//! of `utter-agent` that is a regular app. «Открыть» brings that app forward.

use crate::agents::event::Agent;
use serde::{Deserialize, Serialize};

/// The app Codex lives in (ChatGPT; the standalone Codex app has the same id).
pub const CODEX_APP: &str = "com.openai.codex";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub pid: i32,
    pub bundle_id: Option<String>,
    pub name: Option<String>,
}

/// One ancestor: pid, bundle id (only for regular apps), name.
pub type Ancestor = (i32, Option<String>, Option<String>);

/// The first ancestor that is a regular app (shells, node and helpers have no bundle id here).
pub fn pick_host(chain: &[Ancestor]) -> Option<Host> {
    chain
        .iter()
        .find(|(_, bundle, _)| bundle.is_some())
        .map(|(pid, bundle_id, name)| Host { pid: *pid, bundle_id: bundle_id.clone(), name: name.clone() })
}

/// The app «Открыть» brings forward: the one found among the helper's parents, else (Codex runs
/// hooks where the helper cannot see its parents — live 2026-11-14) the agent's own app.
pub fn host_to_open(agent: Agent, chain: &[Ancestor]) -> Option<Host> {
    pick_host(chain).or_else(|| match agent {
        Agent::Codex => Some(Host { pid: 0, bundle_id: Some(CODEX_APP.to_string()), name: Some("ChatGPT".to_string()) }),
        Agent::ClaudeCode => None,
    })
}

/// Looks the pids up among running apps; only regular (Dock) apps get a bundle id.
#[cfg(target_os = "macos")]
pub fn parent_chain(ancestors: &[i32]) -> Vec<Ancestor> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};
    ancestors
        .iter()
        .map(|&pid| {
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
            match app {
                Some(app) if app.activationPolicy() == NSApplicationActivationPolicy::Regular => {
                    let bundle = app.bundleIdentifier().map(|b| b.to_string());
                    let name = app.localizedName().map(|n| n.to_string());
                    (pid, bundle, name)
                }
                _ => (pid, None, None),
            }
        })
        .collect()
}

/// Brings the app forward with its desktop (Space). `open -b` goes through LaunchServices, which
/// switches to the app's full-screen Space; a background app's own `activate` may be refused
/// on macOS 14+ (cooperative activation).
pub fn activate(host: &Host) {
    let Some(bundle) = host.bundle_id.as_deref() else { return };
    if let Err(e) = std::process::Command::new("/usr/bin/open").args(["-b", bundle]).spawn() {
        tracing::warn!("could not open {bundle}: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn parent_chain(ancestors: &[i32]) -> Vec<Ancestor> {
    ancestors.iter().map(|&pid| (pid, None, None)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_host_skips_shells() {
        let chain = vec![
            (30, None, Some("zsh".to_string())),
            (20, None, Some("node".to_string())),
            (10, Some("com.anthropic.claudefordesktop".to_string()), Some("Claude".to_string())),
            (1, Some("com.apple.launchd".to_string()), None),
        ];
        assert_eq!(
            pick_host(&chain),
            Some(Host { pid: 10, bundle_id: Some("com.anthropic.claudefordesktop".into()), name: Some("Claude".into()) })
        );
        assert_eq!(pick_host(&chain[..2]), None);
    }

    #[test]
    fn codex_falls_back_to_its_app() {
        // Live 2026-11-14: Codex runs hooks where the helper cannot see its parents, so «Открыть»
        // found nothing. Codex lives in the ChatGPT app (bundle id com.openai.codex).
        let codex = host_to_open(Agent::Codex, &[]).unwrap();
        assert_eq!(codex.bundle_id.as_deref(), Some(CODEX_APP));
        assert_eq!(host_to_open(Agent::ClaudeCode, &[]), None);
        // A found app still wins.
        let chain = vec![(10, Some("com.googlecode.iterm2".to_string()), Some("iTerm2".to_string()))];
        assert_eq!(host_to_open(Agent::Codex, &chain).unwrap().bundle_id.as_deref(), Some("com.googlecode.iterm2"));
    }
}
