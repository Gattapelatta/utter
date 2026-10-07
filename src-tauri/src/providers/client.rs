use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::{multipart, Response, StatusCode};
use serde::Deserialize;

use super::{Completion, ProviderConfig, ProviderError, SpeechToText, TextModel};

/// Gemini answers short clips in ~1.5 s; 10 s keeps a dead network from looking endless.
const BASE_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// 10 s, plus 5 s for every minute of audio beyond two minutes.
pub fn request_timeout(audio_secs: f32) -> Duration {
    let extra_minutes = ((audio_secs - 120.0) / 60.0).ceil().max(0.0) as u64;
    BASE_TIMEOUT + Duration::from_secs(5 * extra_minutes)
}

/// Parses `"30"`, `"7.66s"`, `"2m59.56s"`, `"1h2m3s"`.
pub fn parse_reset(value: &str) -> Option<Duration> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if let Ok(secs) = v.parse::<f64>() {
        return (secs >= 0.0).then(|| Duration::from_secs_f64(secs));
    }
    let mut total = 0.0f64;
    let mut num = String::new();
    for ch in v.chars() {
        match ch {
            '0'..='9' | '.' => num.push(ch),
            'h' | 'm' | 's' => {
                let n: f64 = num.parse().ok()?;
                num.clear();
                total += n * match ch {
                    'h' => 3600.0,
                    'm' => 60.0,
                    _ => 1.0,
                };
            }
            _ => return None,
        }
    }
    if !num.is_empty() {
        return None;
    }
    Some(Duration::from_millis((total * 1000.0).round() as u64))
}

/// Any OpenAI-compatible API (Groq and OpenAI differ only in base URL and models).
pub struct OpenAiCompatible {
    client: reqwest::Client,
    cfg: ProviderConfig,
    timeout_override: Option<Duration>,
    retry_delay: Duration,
}

#[derive(Deserialize)]
struct TranscriptionBody {
    text: String,
}

#[derive(Deserialize)]
struct ChatBody {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    total_tokens: u32,
}

impl OpenAiCompatible {
    pub fn new(cfg: ProviderConfig) -> Self {
        let client = reqwest::Client::builder()
            .pool_idle_timeout(Duration::from_secs(90))
            // Offline, DNS/TCP would otherwise hang until the full request timeout.
            .connect_timeout(Duration::from_secs(3))
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

    pub fn config(&self) -> &ProviderConfig {
        &self.cfg
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.cfg.base_url.trim_end_matches('/'), path)
    }

    fn timeout(&self, audio_secs: f32) -> Duration {
        self.timeout_override.unwrap_or_else(|| request_timeout(audio_secs))
    }

    /// Opens the TLS connection early so the real request skips the handshake.
    pub async fn prewarm(&self) {
        let _ = self
            .client
            .get(self.url("models"))
            .bearer_auth(&self.cfg.api_key)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
    }

    pub async fn verify_key(&self) -> Result<(), ProviderError> {
        let resp = self
            .client
            .get(self.url("models"))
            .bearer_auth(&self.cfg.api_key)
            .timeout(self.timeout(0.0))
            .send()
            .await
            .map_err(map_reqwest)?;
        check_status(resp).await.map(|_| ())
    }

    /// Sends a request built by `build`, retrying once on a 5xx response.
    async fn send_with_retry<F>(&self, build: F) -> Result<Response, ProviderError>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        match check_status(build().send().await.map_err(map_reqwest)?).await {
            Err(ProviderError::Server(_)) => {
                tokio::time::sleep(self.retry_delay).await;
                check_status(build().send().await.map_err(map_reqwest)?).await
            }
            other => other,
        }
    }
}

async fn check_status(resp: Response) -> Result<Response, ProviderError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => {
            ProviderError::RateLimited { retry_after: retry_after(resp.headers()) }
        }
        s if s.is_server_error() => ProviderError::Server(s.as_u16()),
        s => ProviderError::BadResponse(format!("HTTP {}", s.as_u16())),
    })
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    ["retry-after", "x-ratelimit-reset-requests", "x-ratelimit-reset-tokens"]
        .iter()
        .find_map(|name| headers.get(*name)?.to_str().ok().and_then(parse_reset))
}

pub(super) fn map_reqwest(e: reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::Timeout
    } else if e.is_decode() || e.is_body() {
        ProviderError::BadResponse(e.to_string())
    } else {
        ProviderError::Network
    }
}

async fn read_json<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, ProviderError> {
    let bytes = resp.bytes().await.map_err(map_reqwest)?;
    serde_json::from_slice(&bytes).map_err(|e| ProviderError::BadResponse(e.to_string()))
}

#[async_trait::async_trait]
impl SpeechToText for OpenAiCompatible {
    async fn transcribe(
        &self,
        wav: Vec<u8>,
        language: Option<String>,
        audio_secs: f32,
    ) -> Result<String, ProviderError> {
        let timeout = self.timeout(audio_secs);
        let build = || {
            let file = multipart::Part::bytes(wav.clone())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .unwrap_or_else(|_| multipart::Part::bytes(wav.clone()).file_name("audio.wav"));
            let mut form = multipart::Form::new()
                .part("file", file)
                .text("model", self.cfg.stt_model.clone())
                .text("temperature", "0")
                .text("response_format", "json");
            if let Some(lang) = &language {
                form = form.text("language", lang.clone());
            }
            self.client
                .post(self.url("audio/transcriptions"))
                .bearer_auth(&self.cfg.api_key)
                .timeout(timeout)
                .multipart(form)
        };
        let resp = self.send_with_retry(build).await?;
        let body: TranscriptionBody = read_json(resp).await?;
        Ok(body.text.trim().to_string())
    }
}

#[async_trait::async_trait]
impl TextModel for OpenAiCompatible {
    async fn complete(&self, system: &str, user: &str) -> Result<Completion, ProviderError> {
        let body = serde_json::json!({
            "model": self.cfg.text_model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
        });
        let timeout = self.timeout(0.0);
        let build = || {
            self.client
                .post(self.url("chat/completions"))
                .bearer_auth(&self.cfg.api_key)
                .timeout(timeout)
                .json(&body)
        };
        let resp = self.send_with_retry(build).await?;
        let chat: ChatBody = read_json(resp).await?;
        let text = chat
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| ProviderError::BadResponse("no choices".into()))?;
        Ok(Completion { text, total_tokens: chat.usage.map(|u| u.total_tokens).unwrap_or(0) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderConfig, SpeechToText, TextModel};
    use crate::settings::ProviderKind;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(server: &MockServer) -> ProviderConfig {
        ProviderConfig {
            kind: ProviderKind::Groq,
            base_url: server.uri(),
            api_key: "k".into(),
            stt_model: "whisper-x".into(),
            text_model: "llm-x".into(),
        }
    }

    fn client(server: &MockServer) -> OpenAiCompatible {
        OpenAiCompatible::with_timeout_override(cfg(server), Duration::from_millis(500))
            .with_retry_delay(Duration::from_millis(10))
    }

    #[tokio::test]
    async fn transcribe_sends_multipart_and_returns_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .and(body_string_contains("whisper-x"))
            .and(body_string_contains("name=\"temperature\"\r\n\r\n0"))
            .and(body_string_contains("name=\"language\"\r\n\r\nru"))
            .and(body_string_contains("filename=\"audio.wav\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": " Привет "})))
            .expect(1)
            .mount(&server)
            .await;
        let text = client(&server).transcribe(b"RIFF".to_vec(), Some("ru".into()), 1.0).await;
        assert_eq!(text, Ok("Привет".to_string()));
    }

    #[tokio::test]
    async fn transcribe_omits_language_when_auto() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "hi"})))
            .mount(&server)
            .await;
        client(&server).transcribe(b"RIFF".to_vec(), None, 1.0).await.unwrap();
        let req = &server.received_requests().await.unwrap()[0];
        assert!(!String::from_utf8_lossy(&req.body).contains("name=\"language\""));
    }

    #[tokio::test]
    async fn complete_returns_text_and_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("\"model\":\"llm-x\""))
            .and(body_string_contains("\"role\":\"system\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "Hello."}}],
                "usage": {"total_tokens": 42}
            })))
            .mount(&server)
            .await;
        let c = client(&server).complete("sys", "user").await.unwrap();
        assert_eq!(c.text, "Hello.");
        assert_eq!(c.total_tokens, 42);
    }

    #[tokio::test]
    async fn unauthorized_maps_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let r = client(&server).complete("s", "u").await;
        assert_eq!(r, Err(ProviderError::Unauthorized));
    }

    #[tokio::test]
    async fn rate_limited_uses_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "42"))
            .mount(&server)
            .await;
        let r = client(&server).complete("s", "u").await;
        assert_eq!(r, Err(ProviderError::RateLimited { retry_after: Some(Duration::from_secs(42)) }));
    }

    #[tokio::test]
    async fn rate_limited_falls_back_to_groq_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429).insert_header("x-ratelimit-reset-requests", "2m59.56s"),
            )
            .mount(&server)
            .await;
        let r = client(&server).transcribe(vec![], None, 1.0).await;
        assert_eq!(
            r,
            Err(ProviderError::RateLimited { retry_after: Some(Duration::from_millis(179_560)) })
        );
    }

    #[tokio::test]
    async fn server_error_retries_once_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "ok"})))
            .mount(&server)
            .await;
        let r = client(&server).transcribe(vec![], None, 1.0).await;
        assert_eq!(r, Ok("ok".to_string()));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn server_error_twice_returns_server() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let r = client(&server).transcribe(vec![], None, 1.0).await;
        assert_eq!(r, Err(ProviderError::Server(503)));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn bad_json_is_bad_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let r = client(&server).complete("s", "u").await;
        assert!(matches!(r, Err(ProviderError::BadResponse(_))));
    }

    #[tokio::test]
    async fn timeout_maps_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "late"}))
                    .set_delay(Duration::from_secs(2)),
            )
            .mount(&server)
            .await;
        let r = client(&server).transcribe(vec![], None, 1.0).await;
        assert_eq!(r, Err(ProviderError::Timeout));
    }

    #[tokio::test]
    async fn network_error_maps_network() {
        let cfg = ProviderConfig {
            kind: ProviderKind::Groq,
            base_url: "http://127.0.0.1:9".into(),
            api_key: "k".into(),
            stt_model: "w".into(),
            text_model: "t".into(),
        };
        let c = OpenAiCompatible::with_timeout_override(cfg, Duration::from_millis(500))
            .with_retry_delay(Duration::from_millis(10));
        assert_eq!(c.complete("s", "u").await, Err(ProviderError::Network));
    }

    #[tokio::test]
    async fn verify_key_checks_models_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .mount(&server)
            .await;
        assert_eq!(client(&server).verify_key().await, Ok(()));
    }

    #[test]
    fn timeout_formula() {
        assert_eq!(request_timeout(30.0), Duration::from_secs(10));
        assert_eq!(request_timeout(120.0), Duration::from_secs(10));
        assert_eq!(request_timeout(180.0), Duration::from_secs(15));
        assert_eq!(request_timeout(600.0), Duration::from_secs(50));
    }

    #[test]
    fn parse_reset_formats() {
        assert_eq!(parse_reset("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_reset("7.66s"), Some(Duration::from_millis(7_660)));
        assert_eq!(parse_reset("2m59.56s"), Some(Duration::from_millis(179_560)));
        assert_eq!(parse_reset("1h2m3s"), Some(Duration::from_secs(3_723)));
        assert_eq!(parse_reset("soon"), None);
        assert_eq!(parse_reset(""), None);
    }
}
