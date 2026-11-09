//! Which app a session runs in (Claude, Terminal, iTerm, VS Code, ChatGPT…): the first ancestor
//! of `utter-agent` that is a regular app. «Открыть» brings that app forward.

use serde::{Deserialize, Serialize};

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
}
