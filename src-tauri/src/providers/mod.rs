mod client;
pub mod defaults;
mod error;

pub use client::{parse_reset, request_timeout, OpenAiCompatible};
pub use error::ProviderError;

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
