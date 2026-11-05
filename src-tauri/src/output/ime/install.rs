//! Installs the bundled input method into ~/Library/Input Methods, registers and enables it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{tis, Tis, IM_ID};

/// `CFBundleVersion` from a bundle's XML Info.plist.
pub fn bundle_version(app: &Path) -> Option<u32> {
    let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    let after = plist.split("<key>CFBundleVersion</key>").nth(1)?;
    let value = after.split("<string>").nth(1)?.split("</string>").next()?;
    value.trim().parse().ok()
}

fn executable(app: &Path) -> PathBuf {
    app.join("Contents/MacOS/UtterIM")
}

/// Not installed, an older `CFBundleVersion`, or (same version) a different build.
pub fn needs_install(bundled: &Path, installed: &Path) -> bool {
    match (bundle_version(bundled), bundle_version(installed)) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(b), Some(i)) if i != b => i < b,
        _ => std::fs::read(executable(bundled)).ok() != std::fs::read(executable(installed)).ok(),
    }
}

pub fn installed_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Input Methods/UtterLive.app"))
}

/// Copies (when needed), registers and asks macOS to enable the input method. Returns whether
/// it was copied; `tis::is_enabled` (in a fresh list) tells whether the user has added it.
pub fn install(bundled: &Path) -> Result<bool, String> {
    let installed = installed_path().ok_or("no home folder")?;
    let copied = needs_install(bundled, &installed);
    if copied {
        if let Some(dir) = installed.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_dir_all(&installed);
        let ok = std::process::Command::new("/usr/bin/ditto").arg(bundled).arg(&installed).status().is_ok_and(|s| s.success());
        if !ok {
            return Err("copy failed".into());
        }
        // A running old copy would keep serving fields.
        let _ = std::process::Command::new("/usr/bin/killall").arg("UtterIM").status();
    }
    if !tis::register(&installed) {
        return Err("register failed".into());
    }
    // macOS 15 answers "ok" but enables a third-party input method only when the user adds
    // it in System Settings (measured 2026-11-05); ask, never fail on it.
    let _ = tis::enable(IM_ID);
    Ok(copied)
}

/// Starts the input method process: select it, wake the field, switch back.
pub fn warm_up(tis: &dyn Tis, wake: &dyn Fn()) {
    let Some(user) = tis.current_id().filter(|id| id != IM_ID) else { return };
    if tis.select(IM_ID) {
        std::thread::sleep(Duration::from_millis(150));
        wake();
        std::thread::sleep(Duration::from_millis(150));
    }
    tis.select(&user);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(dir: &Path, name: &str, version: Option<u32>, exe: &[u8]) -> PathBuf {
        let app = dir.join(name);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        if let Some(v) = version {
            let plist = format!("<plist><dict>\n\t<key>CFBundleVersion</key>\n\t<string>{v}</string>\n</dict></plist>");
            std::fs::write(app.join("Contents/Info.plist"), plist).unwrap();
        }
        std::fs::write(executable(&app), exe).unwrap();
        app
    }

    #[test]
    fn needs_install_by_version() {
        let d = tempfile::tempdir().unwrap();
        let bundled = bundle(d.path(), "b.app", Some(2), b"new");
        assert!(needs_install(&bundled, &d.path().join("missing.app")), "not installed");
        assert!(needs_install(&bundled, &bundle(d.path(), "old.app", Some(1), b"old")), "older");
        assert!(!needs_install(&bundled, &bundle(d.path(), "same.app", Some(2), b"new")), "same build");
        assert!(needs_install(&bundled, &bundle(d.path(), "rebuilt.app", Some(2), b"other")), "same version, new build");
        assert!(!needs_install(&bundled, &bundle(d.path(), "newer.app", Some(3), b"x")), "newer installed");
        assert!(!needs_install(&d.path().join("none.app"), &bundled), "nothing bundled");
    }

    /// The input method's plist names the same source, connection and bundle as the code.
    #[test]
    fn plist_ids_match_the_code() {
        let plist = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../macos/UtterIM/Info.plist")).unwrap();
        let value = |key: &str| {
            let after = plist.split(&format!("<key>{key}</key>")).nth(1).unwrap();
            after.split("<string>").nth(1).unwrap().split("</string>").next().unwrap().to_string()
        };
        assert_eq!(value("CFBundleIdentifier"), IM_ID);
        assert_eq!(value("TISInputSourceID"), IM_ID);
        assert_eq!(value("InputMethodConnectionName"), format!("{IM_ID}_Connection"));
        assert_eq!(super::super::peer::IM_IDENTIFIER, IM_ID);
        // A fresh id: macOS still maps the 2026-11-04 prototype's path and id to old entries.
        assert_eq!(IM_ID, "app.utter.inputmethod.live");
        assert!(installed_path().unwrap().ends_with("Input Methods/UtterLive.app"));
    }

    #[test]
    fn reads_the_real_plist_version() {
        let im = Path::new(env!("CARGO_MANIFEST_DIR")).join("../macos/UtterIM");
        std::fs::create_dir_all(std::env::temp_dir().join("utter-im-plist/Contents")).unwrap();
        let tmp = std::env::temp_dir().join("utter-im-plist");
        std::fs::copy(im.join("Info.plist"), tmp.join("Contents/Info.plist")).unwrap();
        assert_eq!(bundle_version(&tmp), Some(1));
    }
}
