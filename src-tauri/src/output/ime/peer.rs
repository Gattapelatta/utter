//! Code-signature check of the socket's other end: only Utter's own input method, signed
//! with the same certificate as Utter, may talk to it (spec 2026-11-04 §5.4).

use std::ffi::c_void;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};

pub const IM_IDENTIFIER: &str = "app.utter.desktop.inputmethod";

type OSStatus = i32;
type SecCodeRef = *const c_void;

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecGuestAttributeAudit: CFStringRef;
    static kSecCodeInfoCertificates: CFStringRef;
    fn SecCodeCopySelf(flags: u32, code: *mut SecCodeRef) -> OSStatus;
    fn SecCodeCopyStaticCode(code: SecCodeRef, flags: u32, out: *mut SecCodeRef) -> OSStatus;
    fn SecCodeCopySigningInformation(code: SecCodeRef, flags: u32, info: *mut CFDictionaryRef) -> OSStatus;
    fn SecCertificateCopyData(cert: CFTypeRef) -> CFDataRef;
    fn SecCodeCopyGuestWithAttributes(host: SecCodeRef, attrs: CFDictionaryRef, flags: u32, guest: *mut SecCodeRef) -> OSStatus;
    fn SecRequirementCreateWithString(text: CFStringRef, flags: u32, req: *mut CFTypeRef) -> OSStatus;
    fn SecCodeCheckValidity(code: SecCodeRef, flags: u32, req: CFTypeRef) -> OSStatus;
}

extern "C" {
    fn getsockopt(fd: i32, level: i32, name: i32, value: *mut c_void, len: *mut u32) -> i32;
    fn CC_SHA1(data: *const c_void, len: u32, md: *mut u8) -> *mut u8;
}

const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;
/// <sys/un.h>: SOL_LOCAL, LOCAL_PEERTOKEN.
const SOL_LOCAL: i32 = 0;
const LOCAL_PEERTOKEN: i32 = 6;

pub fn requirement(identifier: &str, leaf_sha1_hex: &str) -> String {
    format!("identifier \"{identifier}\" and certificate leaf = H\"{leaf_sha1_hex}\"")
}

/// SHA-1 of the leaf certificate Utter itself is signed with (`None` when unsigned or ad hoc).
pub fn own_leaf_sha1() -> Option<String> {
    unsafe {
        let mut me: SecCodeRef = std::ptr::null();
        if SecCodeCopySelf(0, &mut me) != 0 || me.is_null() {
            return None;
        }
        let mut stat: SecCodeRef = std::ptr::null();
        let ok = SecCodeCopyStaticCode(me, 0, &mut stat) == 0 && !stat.is_null();
        CFRelease(me);
        if !ok {
            return None;
        }
        let mut info: CFDictionaryRef = std::ptr::null();
        let ok = SecCodeCopySigningInformation(stat, K_SEC_CS_SIGNING_INFORMATION, &mut info) == 0 && !info.is_null();
        CFRelease(stat);
        if !ok {
            return None;
        }
        let info: CFDictionary = CFDictionary::wrap_under_create_rule(info);
        let certs = info.find(kSecCodeInfoCertificates as *const c_void)?;
        let certs = core_foundation::array::CFArray::<*const c_void>::wrap_under_get_rule(*certs as _);
        let leaf = *certs.get(0)?;
        let der = SecCertificateCopyData(leaf);
        if der.is_null() {
            return None;
        }
        let der = CFData::wrap_under_create_rule(der);
        let mut md = [0u8; 20];
        CC_SHA1(der.bytes().as_ptr() as *const c_void, der.len() as u32, md.as_mut_ptr());
        Some(md.iter().map(|b| format!("{b:02X}")).collect())
    }
}

/// Checks the process at the other end of a connected Unix socket against `requirement`.
pub fn check_peer(fd: i32, requirement: &str) -> bool {
    unsafe {
        let mut token = [0u8; 32];
        let mut len = token.len() as u32;
        if getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, token.as_mut_ptr() as *mut c_void, &mut len) != 0 {
            return false;
        }
        let key = CFString::wrap_under_get_rule(kSecGuestAttributeAudit);
        let value = CFData::from_buffer(&token[..len as usize]);
        let attrs = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        let mut guest: SecCodeRef = std::ptr::null();
        if SecCodeCopyGuestWithAttributes(std::ptr::null(), attrs.as_concrete_TypeRef(), 0, &mut guest) != 0 || guest.is_null() {
            return false;
        }
        let text = CFString::new(requirement);
        let mut req: CFTypeRef = std::ptr::null();
        let ok = SecRequirementCreateWithString(text.as_concrete_TypeRef(), 0, &mut req) == 0
            && !req.is_null()
            && SecCodeCheckValidity(guest, 0, req) == 0;
        if !req.is_null() {
            CFRelease(req);
        }
        CFRelease(guest);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_string() {
        assert_eq!(
            requirement(IM_IDENTIFIER, "5439ABCD"),
            r#"identifier "app.utter.desktop.inputmethod" and certificate leaf = H"5439ABCD""#
        );
    }

    #[test]
    fn unsigned_peer_is_rejected() {
        // The test binary's socketpair peer is itself: not signed as the input method.
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        use std::os::fd::AsRawFd;
        assert!(!check_peer(a.as_raw_fd(), &requirement(IM_IDENTIFIER, "00")));
        assert!(!check_peer(-1, &requirement(IM_IDENTIFIER, "00")));
    }
}
