//! Text Input Sources (Carbon): the current keyboard layout, switching, registering.

use std::ffi::c_void;
use std::path::Path;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURLRef, CFURL};

type OSStatus = i32;
type TISInputSourceRef = *const c_void;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyInputSourceID: CFStringRef;
    static kTISPropertyInputSourceIsEnabled: CFStringRef;
    fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
    fn TISGetInputSourceProperty(src: TISInputSourceRef, key: CFStringRef) -> CFTypeRef;
    fn TISCreateInputSourceList(props: core_foundation::dictionary::CFDictionaryRef, include_all: u8) -> CFArrayRef;
    fn TISSelectInputSource(src: TISInputSourceRef) -> OSStatus;
    fn TISEnableInputSource(src: TISInputSourceRef) -> OSStatus;
    fn TISRegisterInputSource(location: CFURLRef) -> OSStatus;
}

use super::Tis;

pub struct SystemTis;

impl Tis for SystemTis {
    fn current_id(&self) -> Option<String> {
        current_id()
    }
    fn select(&self, id: &str) -> bool {
        select(id)
    }
}

fn source_id(src: TISInputSourceRef) -> Option<String> {
    unsafe {
        let v = TISGetInputSourceProperty(src, kTISPropertyInputSourceID);
        (!v.is_null()).then(|| CFString::wrap_under_get_rule(v as CFStringRef).to_string())
    }
}

/// Runs `f` with the input source `id` (also not enabled ones when `include_all`).
fn with_source<R>(id: &str, include_all: bool, f: impl FnOnce(TISInputSourceRef) -> R) -> Option<R> {
    unsafe {
        let key = CFString::wrap_under_get_rule(kTISPropertyInputSourceID);
        let props = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFString::new(id).as_CFType())]);
        let list = TISCreateInputSourceList(props.as_concrete_TypeRef(), include_all as u8);
        if list.is_null() {
            return None;
        }
        let list = CFArray::<*const c_void>::wrap_under_create_rule(list);
        let src = *list.get(0)?;
        Some(f(src))
    }
}

pub fn current_id() -> Option<String> {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        if src.is_null() {
            return None;
        }
        let id = source_id(src);
        CFRelease(src);
        id
    }
}

pub fn select(id: &str) -> bool {
    with_source(id, false, |src| unsafe { TISSelectInputSource(src) == 0 }).unwrap_or(false)
}

pub fn register(bundle: &Path) -> bool {
    let Some(url) = CFURL::from_path(bundle, true) else { return false };
    unsafe { TISRegisterInputSource(url.as_concrete_TypeRef()) == 0 }
}

pub fn enable(id: &str) -> bool {
    with_source(id, true, |src| unsafe { TISEnableInputSource(src) == 0 }).unwrap_or(false)
}

pub fn is_enabled(id: &str) -> bool {
    with_source(id, true, |src| unsafe {
        let v = TISGetInputSourceProperty(src, kTISPropertyInputSourceIsEnabled);
        !v.is_null() && CFBoolean::wrap_under_get_rule(v as _) == CFBoolean::true_value()
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_layout_is_readable() {
        // Any Mac has a current keyboard input source.
        let id = current_id().expect("a current input source");
        assert!(id.contains('.'), "{id}");
        assert!(is_enabled(&id));
        assert!(!is_enabled("app.utter.desktop.no-such-source"));
    }
}
