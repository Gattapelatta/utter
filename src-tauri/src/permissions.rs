//! Microphone and Accessibility permission checks (macOS).

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PermStatus {
    Granted,
    Denied,
    NotDetermined,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::PermStatus;
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }

    pub fn microphone() -> PermStatus {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
            return PermStatus::Denied;
        };
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) };
        match status {
            AVAuthorizationStatus::Authorized => PermStatus::Granted,
            AVAuthorizationStatus::NotDetermined => PermStatus::NotDetermined,
            _ => PermStatus::Denied,
        }
    }

    pub async fn request_microphone() -> bool {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
            return false;
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        {
            // The block is dropped before awaiting so the future stays `Send`.
            let tx = std::sync::Mutex::new(Some(tx));
            let block = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
                if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                    let _ = tx.send(granted.as_bool());
                }
            });
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block) };
        }
        rx.await.unwrap_or(false)
    }

    pub fn accessibility(prompt: bool) -> bool {
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
    }

    pub fn open_accessibility_settings() {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "queries real macOS permission state"]
    fn permission_queries_do_not_crash() {
        let mic = microphone();
        let ax = accessibility(false);
        println!("microphone: {mic:?}, accessibility: {ax}");
    }
}
