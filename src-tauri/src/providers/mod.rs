mod client;
pub mod defaults;
mod error;
mod gemini;
pub mod gemini_live;
#[cfg(test)]
mod bench;

pub use client::{parse_reset, request_timeout, OpenAiCompatible};
pub use error::ProviderError;
pub use gemini::{gemini_language_code, http_client as gemini_http_client, GeminiClient};

use crate::settings::ProviderKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: String,
    pub stt_model: String,
    pub text_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub text: String,
    pub total_tokens: u32,
}

#[async_trait::async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(
        &self,
        wav: Vec<u8>,
        language: Option<String>,
        audio_secs: f32,
    ) -> Result<String, ProviderError>;
}

#[async_trait::async_trait]
pub trait TextModel: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<Completion, ProviderError>;
}

/// A model that turns audio straight into final text in one request
/// (transcription plus cleanup or translation).
#[async_trait::async_trait]
pub trait AudioLlm: Send + Sync {
    async fn complete_audio(
        &self,
        system: &str,
        wav: Vec<u8>,
        language: Option<String>,
        audio_secs: f32,
    ) -> Result<Completion, ProviderError>;
}
