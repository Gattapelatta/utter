//! Google Gemini via the Interactions API (`POST {base}/interactions`).

use std::time::Duration;

use base64::Engine;
use reqwest::{Response, StatusCode};
use serde_json::{json, Value};

use super::client::{map_reqwest, parse_reset, request_timeout};
use super::{AudioLlm, Completion, ProviderConfig, ProviderError, SpeechToText, TextModel};

const RETRY_DELAY: Duration = Duration::from_secs(1);

/// Gemini expects BCP-47 locales; settings store two-letter codes.
pub fn gemini_language_code(code: &str) -> Option<&'static str> {
    Some(match code {
        "ru" => "ru-RU",
        "en" => "en-US",
        "uk" => "uk-UA",
        "de" => "de-DE",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "it" => "it-IT",
        "pt" => "pt-BR",
        "pl" => "pl-PL",
        "tr" => "tr-TR",
        "ja" => "ja-JP",
        "zh" => "cmn-Hans-CN",
        "ko" => "ko-KR",
        _ => return None,
    })
}

pub struct GeminiClient {
    client: reqwest::Client,
    cfg: ProviderConfig,
    timeout_override: Option<Duration>,
    retry_delay: Duration,
}

impl GeminiClient {
    pub fn new(cfg: ProviderConfig) -> Self {
        let client = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        Self { client, cfg, timeout_override: None, retry_delay: RETRY_DELAY }
    }

    pub fn with_timeout_override(cfg: ProviderConfig, timeout: Duration) -> Self {
        Self { timeout_override: Some(timeout), ..Self::new(cfg) }
    }

    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.cfg.base_url.trim_end_matches('/'), path)
    }

    fn timeout(&self, audio_secs: f32) -> Duration {
        self.timeout_override.unwrap_or_else(|| request_timeout(audio_secs))
    }

    pub async fn prewarm(&self) {
        let _ = self
            .client
            .get(self.url("models"))
            .header("x-goog-api-key", &self.cfg.api_key)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
    }

    pub async fn verify_key(&self) -> Result<(), ProviderError> {
        let resp = self
            .client
            .get(self.url("models"))
            .header("x-goog-api-key", &self.cfg.api_key)
            .timeout(self.timeout(0.0))
            .send()
            .await
            .map_err(map_reqwest)?;
        check(resp).await.map(|_| ())
    }

    async fn interact(&self, body: &Value, timeout: Duration) -> Result<Value, ProviderError> {
        let send = || async {
            let resp = self
                .client
                .post(self.url("interactions"))
                .header("x-goog-api-key", &self.cfg.api_key)
                .timeout(timeout)
                .json(body)
                .send()
                .await
                .map_err(map_reqwest)?;
            check(resp).await
        };
        let resp = match send().await {
            Err(ProviderError::Server(_)) => {
                tokio::time::sleep(self.retry_delay).await;
                send().await?
            }
            other => other?,
        };
        let bytes = resp.bytes().await.map_err(map_reqwest)?;
        serde_json::from_slice(&bytes).map_err(|e| ProviderError::BadResponse(e.to_string()))
    }
}

/// Text of an Interaction: `output_text`, else the text parts of its steps.
fn output_text(v: &Value) -> Option<String> {
    if let Some(t) = v.get("output_text").and_then(Value::as_str) {
        return Some(t.to_string());
    }
    let mut out = String::new();
    for step in v.get("steps")?.as_array()? {
        for part in step.get("content").and_then(Value::as_array).into_iter().flatten() {
            if let Some(t) = part.get("text").and_then(Value::as_str) {
                out.push_str(t);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

async fn check(resp: Response) -> Result<Response, ProviderError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let header_delay = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_reset);
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    let message = body
        .pointer("/error/message")
        .or_else(|| body.pointer("/errors/0/message"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Unauthorized,
        StatusCode::BAD_REQUEST if message.contains("API key") => ProviderError::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => {
            let body_delay = body
                .pointer("/error/details")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|d| d.get("retryDelay").and_then(Value::as_str).and_then(parse_reset));
            ProviderError::RateLimited { retry_after: header_delay.or(body_delay) }
        }
        s if s.is_server_error() => ProviderError::Server(s.as_u16()),
        s => ProviderError::BadResponse(if message.is_empty() { format!("HTTP {}", s.as_u16()) } else { message }),
    })
}

#[async_trait::async_trait]
impl SpeechToText for GeminiClient {
    async fn transcribe(
        &self,
        wav: Vec<u8>,
        language: Option<String>,
        audio_secs: f32,
    ) -> Result<String, ProviderError> {
        let data = base64::engine::general_purpose::STANDARD.encode(&wav);
        let mut body = json!({
            "model": self.cfg.stt_model,
            "store": false,
            "input": [{"type": "audio", "mime_type": "audio/wav", "data": data}],
        });
        if let Some(locale) = language.as_deref().and_then(gemini_language_code) {
            body["generation_config"] = json!({"transcription_config": {"language_codes": [locale]}});
        }
        let v = self.interact(&body, self.timeout(audio_secs)).await?;
        let text = output_text(&v).ok_or_else(|| ProviderError::BadResponse("no transcript".into()))?;
        Ok(text.trim().to_string())
    }
}

#[async_trait::async_trait]
impl TextModel for GeminiClient {
    async fn complete(&self, system: &str, user: &str) -> Result<Completion, ProviderError> {
        let body = json!({
            "model": self.cfg.text_model,
            "system_instruction": system,
            "input": user,
            "generation_config": {"temperature": 0.2, "thinking_level": "minimal"},
            "store": false,
        });
        let v = self.interact(&body, self.timeout(0.0)).await?;
        completion(&v)
    }
}

fn completion(v: &Value) -> Result<Completion, ProviderError> {
    let text = output_text(v).ok_or_else(|| ProviderError::BadResponse("no output".into()))?;
    let total_tokens = v.pointer("/usage/total_tokens").and_then(Value::as_u64).unwrap_or(0) as u32;
    Ok(Completion { text, total_tokens })
}

#[async_trait::async_trait]
impl AudioLlm for GeminiClient {
    async fn complete_audio(
        &self,
        system: &str,
        wav: Vec<u8>,
        language: Option<String>,
        audio_secs: f32,
    ) -> Result<Completion, ProviderError> {
        let data = base64::engine::general_purpose::STANDARD.encode(&wav);
        let hint = match language.as_deref().and_then(gemini_language_code) {
            Some(locale) => format!("Process the attached audio. The speaker most likely uses {locale}."),
            None => "Process the attached audio.".to_string(),
        };
        let body = json!({
            "model": self.cfg.text_model,
            "system_instruction": system,
            "input": [{"type": "text", "text": hint}, {"type": "audio", "mime_type": "audio/wav", "data": data}],
            // Minimal thinking: measured ~1.3 s vs ~2.8 s with the default level.
            "generation_config": {"temperature": 0.2, "thinking_level": "minimal"},
            "store": false,
        });
        let v = self.interact(&body, self.timeout(audio_secs)).await?;
        completion(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderConfig, SpeechToText, TextModel};
    use crate::settings::ProviderKind;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> GeminiClient {
        let cfg = ProviderConfig {
            kind: ProviderKind::Gemini,
            base_url: server.uri(),
            api_key: "AIza-test".into(),
            stt_model: "stt-x".into(),
            text_model: "llm-x".into(),
        };
        GeminiClient::with_timeout_override(cfg, Duration::from_millis(500)).with_retry_delay(Duration::from_millis(10))
    }

    #[tokio::test]
    async fn transcribe_sends_inline_audio_and_language() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/interactions"))
            .and(header("x-goog-api-key", "AIza-test"))
            .and(body_partial_json(serde_json::json!({
                "model": "stt-x",
                "store": false,
                "input": [{"type": "audio", "mime_type": "audio/wav", "data": "UklGRg=="}],
                "generation_config": {"transcription_config": {"language_codes": ["ru-RU"]}}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "completed", "output_text": " Привет, мир. "
            })))
            .expect(1)
            .mount(&server)
            .await;
        let t = client(&server).transcribe(b"RIFF".to_vec(), Some("ru".into()), 1.0).await;
        assert_eq!(t, Ok("Привет, мир.".to_string()));
    }

    #[tokio::test]
    async fn transcribe_omits_language_when_auto() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"output_text": "hi"})))
            .mount(&server)
            .await;
        client(&server).transcribe(vec![1], None, 1.0).await.unwrap();
        let body = String::from_utf8_lossy(&server.received_requests().await.unwrap()[0].body).to_string();
        assert!(!body.contains("language_codes"), "{body}");
    }

    #[tokio::test]
    async fn transcribe_reads_steps_when_no_output_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "steps": [{"content": [{"type": "text", "text": "Hello "}, {"type": "text", "text": "world"}]}]
            })))
            .mount(&server)
            .await;
        assert_eq!(client(&server).transcribe(vec![1], None, 1.0).await, Ok("Hello world".to_string()));
    }

    #[tokio::test]
    async fn complete_sends_system_instruction_and_reads_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/interactions"))
            .and(body_partial_json(serde_json::json!({
                "model": "llm-x", "system_instruction": "sys", "input": "user", "store": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "output_text": "Hello.", "usage": {"total_tokens": 21}
            })))
            .mount(&server)
            .await;
        let c = client(&server).complete("sys", "user").await.unwrap();
        assert_eq!((c.text.as_str(), c.total_tokens), ("Hello.", 21));
    }

    #[tokio::test]
    async fn invalid_key_400_maps_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT"}
            })))
            .mount(&server)
            .await;
        assert_eq!(client(&server).complete("s", "u").await, Err(ProviderError::Unauthorized));
    }

    #[tokio::test]
    async fn other_400_is_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {"code": 400, "message": "Unsupported audio format", "status": "INVALID_ARGUMENT"}
            })))
            .mount(&server)
            .await;
        assert!(matches!(client(&server).transcribe(vec![1], None, 1.0).await, Err(ProviderError::BadResponse(m)) if m.contains("Unsupported")));
    }

    #[tokio::test]
    async fn rate_limit_reads_retry_delay_from_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {"code": 429, "status": "RESOURCE_EXHAUSTED", "message": "Quota exceeded",
                  "details": [{"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "37s"}]}
            })))
            .mount(&server)
            .await;
        assert_eq!(
            client(&server).complete("s", "u").await,
            Err(ProviderError::RateLimited { retry_after: Some(Duration::from_secs(37)) })
        );
    }

    #[tokio::test]
    async fn server_error_retries_once() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(503)).up_to_n_times(1).mount(&server).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"output_text": "ok"})))
            .mount(&server)
            .await;
        assert_eq!(client(&server).transcribe(vec![1], None, 1.0).await, Ok("ok".to_string()));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn verify_key_uses_models_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("x-goog-api-key", "AIza-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models": []})))
            .mount(&server)
            .await;
        assert_eq!(client(&server).verify_key().await, Ok(()));
    }

    #[test]
    fn language_code_mapping() {
        assert_eq!(gemini_language_code("ru"), Some("ru-RU"));
        assert_eq!(gemini_language_code("en"), Some("en-US"));
        assert_eq!(gemini_language_code("uk"), Some("uk-UA"));
        assert_eq!(gemini_language_code("xx"), None);
    }

    #[tokio::test]
    async fn audio_complete_sends_system_audio_and_minimal_thinking() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/interactions"))
            .and(body_partial_json(serde_json::json!({
                "model": "llm-x", "system_instruction": "sys", "store": false,
                "generation_config": {"thinking_level": "minimal"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "steps": [{"type": "thought", "signature": "abc"},
                          {"type": "model_output", "content": [{"type": "text", "text": "Готово."}]}],
                "usage": {"total_tokens": 40}
            })))
            .mount(&server)
            .await;
        let c = crate::providers::AudioLlm::complete_audio(&client(&server), "sys", b"RIFF".to_vec(), None, 2.0).await.unwrap();
        assert_eq!((c.text.as_str(), c.total_tokens), ("Готово.", 40));
        let body = String::from_utf8_lossy(&server.received_requests().await.unwrap()[0].body).to_string();
        assert!(body.contains("\"type\":\"audio\"") && body.contains("UklGRg=="), "{body}");
    }

    #[tokio::test]
    async fn complete_uses_minimal_thinking() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(serde_json::json!({"generation_config": {"thinking_level": "minimal"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"output_text": "ok"})))
            .expect(1)
            .mount(&server)
            .await;
        client(&server).complete("s", "u").await.unwrap();
    }
}
