//! The only place provider endpoints and model names live.

use serde::Serialize;

use super::ProviderConfig;
use crate::pipeline::routes::Route;
use crate::settings::ProviderKind;

pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const GEMINI_STT_MODEL: &str = "gemini-3.5-transcribe";
/// Flash-Lite: ~1.3 s audio→text in one request and 15 RPM on the free tier (Flash: 5 RPM).
pub const GEMINI_TEXT_MODEL: &str = "gemini-3.5-flash-lite";
/// Backup when Flash-Lite is slow: 2–3 s audio→text, measured 2026-11-04.
pub const GEMINI_BACKUP_MODEL: &str = "gemini-3.6-flash";

/// Backup models in priority order (spec 2026-11-04 §4.5). On 2026-11-04 Flash-Lite took
/// ~20 s while gemini-3.5-transcribe answered in 1.1 s and gemini-3.6-flash in 2–3 s.
pub const GEMINI_ROUTES: [Route; 3] = [
    Route::Direct { model: GEMINI_TEXT_MODEL },
    Route::TwoStep { stt: GEMINI_STT_MODEL, text: GEMINI_BACKUP_MODEL },
    Route::Direct { model: GEMINI_BACKUP_MODEL },
];

/// Live API (WebSocket): the draft while speaking (spec 2026-11-04 §5.1).
pub const GEMINI_LIVE_URL: &str =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent";
pub const GEMINI_LIVE_MODEL: &str = "gemini-3.5-transcribe-live";

pub const GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_TEXT_MODEL: &str = "openai/gpt-oss-120b";

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const OPENAI_STT_MODEL: &str = "gpt-4o-mini-transcribe";
pub const OPENAI_TEXT_MODEL: &str = "gpt-4o-mini";

/// Daily free-tier limits shown as usage bars; field names match `usage::UsageDay`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FreeLimits {
    pub stt_requests: Option<u32>,
    pub audio_secs: Option<u32>,
    pub llm_requests: Option<u32>,
    pub llm_tokens: Option<u32>,
}

/// Groq: https://console.groq.com/docs/rate-limits (free plan, read 2026-10-07) —
/// whisper-large-v3-turbo 2K requests and 28.8K audio seconds per day,
/// openai/gpt-oss-120b 1K requests and 200K tokens per day.
/// Gemini: Google no longer publishes per-model numbers (only in AI Studio per
/// project, read 2026-10-07), so no bars until a verified value is known.
pub fn free_limits(kind: ProviderKind) -> Option<FreeLimits> {
    match kind {
        ProviderKind::Groq => Some(FreeLimits {
            stt_requests: Some(2_000),
            audio_secs: Some(28_800),
            llm_requests: Some(1_000),
            llm_tokens: Some(200_000),
        }),
        ProviderKind::Gemini | ProviderKind::OpenAi => None,
    }
}

pub fn config_for(kind: ProviderKind, api_key: String) -> ProviderConfig {
    let (base, stt, text) = match kind {
        ProviderKind::Gemini => (GEMINI_BASE_URL, GEMINI_STT_MODEL, GEMINI_TEXT_MODEL),
        ProviderKind::Groq => (GROQ_BASE_URL, GROQ_STT_MODEL, GROQ_TEXT_MODEL),
        ProviderKind::OpenAi => (OPENAI_BASE_URL, OPENAI_STT_MODEL, OPENAI_TEXT_MODEL),
    };
    ProviderConfig {
        kind,
        base_url: base.to_string(),
        api_key,
        stt_model: stt.to_string(),
        text_model: text.to_string(),
    }
}

/// Backup routes of a provider; empty when it has a single route.
pub fn routes_for(kind: ProviderKind) -> &'static [Route] {
    match kind {
        ProviderKind::Gemini => &GEMINI_ROUTES,
        ProviderKind::Groq | ProviderKind::OpenAi => &[],
    }
}

/// The provider's config with the models of one route.
pub fn route_config(kind: ProviderKind, api_key: String, route: Route) -> ProviderConfig {
    let mut cfg = config_for(kind, api_key);
    match route {
        Route::Direct { model } => cfg.text_model = model.to_string(),
        Route::TwoStep { stt, text } => {
            cfg.stt_model = stt.to_string();
            cfg.text_model = text.to_string();
        }
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_limits_known_for_free_tiers() {
        let groq = free_limits(ProviderKind::Groq).expect("Groq has a free tier");
        assert!(groq.stt_requests.is_some() && groq.audio_secs.is_some());
        assert!(groq.llm_requests.is_some() && groq.llm_tokens.is_some());
        assert_eq!(free_limits(ProviderKind::OpenAi), None);
    }

    #[test]
    fn only_gemini_has_backup_routes() {
        assert_eq!(routes_for(ProviderKind::Gemini).len(), 3);
        assert_eq!(routes_for(ProviderKind::Gemini)[0], Route::Direct { model: GEMINI_TEXT_MODEL });
        assert!(routes_for(ProviderKind::Groq).is_empty() && routes_for(ProviderKind::OpenAi).is_empty());
    }

    #[test]
    fn route_config_sets_route_models() {
        let two = route_config(ProviderKind::Gemini, "k".into(), GEMINI_ROUTES[1]);
        assert_eq!((two.stt_model.as_str(), two.text_model.as_str()), (GEMINI_STT_MODEL, GEMINI_BACKUP_MODEL));
        let direct = route_config(ProviderKind::Gemini, "k".into(), GEMINI_ROUTES[2]);
        assert_eq!(direct.text_model, GEMINI_BACKUP_MODEL);
        assert_eq!(direct.api_key, "k");
    }
}
