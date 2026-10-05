//! The only place provider endpoints and model names live.

use super::ProviderConfig;
use crate::settings::ProviderKind;

pub const GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_TEXT_MODEL: &str = "openai/gpt-oss-120b";

pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const OPENAI_STT_MODEL: &str = "gpt-4o-mini-transcribe";
pub const OPENAI_TEXT_MODEL: &str = "gpt-4o-mini";

pub fn config_for(kind: ProviderKind, api_key: String) -> ProviderConfig {
    let (base, stt, text) = match kind {
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
