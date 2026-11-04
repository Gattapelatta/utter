//! Gemini Live API (WebSocket `BidiGenerateContent`): messages and the streaming session.

use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};

use super::{gemini_language_code, ProviderError};

/// Connecting plus `setupComplete` must take no longer than this (plan: Global Constraints).
pub const READY_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscribeMode {
    /// Cleans fillers and self-corrections (cleanup on).
    Smart,
    /// Word for word (cleanup off).
    Verbatim,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    /// `setupComplete`: audio may be sent from now on.
    Ready,
    /// The current piece's draft, revised as the user speaks.
    Interim(String),
    /// The final text of one piece.
    Segment(String),
    TurnComplete,
    Error(String),
}

/// Two-letter settings code → BCP-47 (`ru` → `ru-RU`); unknown codes pass as is; `None` is auto.
pub fn language_code(lang: Option<&str>) -> Option<String> {
    let code = lang?.trim();
    if code.is_empty() {
        return None;
    }
    Some(gemini_language_code(code).map_or_else(|| code.to_string(), str::to_string))
}

/// The first message of a session. `language` is the settings code (`None` = auto).
pub fn setup_message(model: &str, mode: TranscribeMode, language: Option<&str>) -> Value {
    let mode = match mode {
        TranscribeMode::Smart => "SMART",
        TranscribeMode::Verbatim => "VERBATIM",
    };
    let codes: Vec<String> = language_code(language).into_iter().collect();
    json!({
        "setup": {
            "model": format!("models/{model}"),
            "generationConfig": {"responseModalities": ["TEXT"]},
            "inputAudioTranscription": {"mode": mode, "languageCodes": codes},
        }
    })
}

/// 16-bit little-endian PCM at 16 kHz, mono.
pub fn audio_message(pcm: &[i16]) -> Value {
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    json!({"realtimeInput": {"audio": {"data": data, "mimeType": "audio/pcm;rate=16000"}}})
}

pub fn end_message() -> Value {
    json!({"realtimeInput": {"audioStreamEnd": true}})
}

/// Events in one server message (it may carry several fields); unknown fields are ignored.
pub fn parse_server(text: &str) -> Vec<LiveEvent> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut events = Vec::new();
    if v.get("setupComplete").is_some() {
        events.push(LiveEvent::Ready);
    }
    let text_at = |path: &str| v.pointer(path).and_then(Value::as_str).map(str::to_string);
    if let Some(t) = text_at("/serverContent/interimInputTranscription/text") {
        events.push(LiveEvent::Interim(t));
    }
    if let Some(t) = text_at("/serverContent/inputTranscription/text") {
        events.push(LiveEvent::Segment(t));
    }
    if v.pointer("/serverContent/turnComplete").and_then(Value::as_bool) == Some(true) {
        events.push(LiveEvent::TurnComplete);
    }
    if let Some(e) = v.get("error") {
        let msg = e.get("message").and_then(Value::as_str).map_or_else(|| e.to_string(), str::to_string);
        events.push(LiveEvent::Error(msg));
    }
    events
}


pub struct LiveConfig {
    pub url: String,
    pub key: String,
    pub model: String,
    pub mode: TranscribeMode,
    /// Settings code of the speech language; `None` is auto.
    pub language: Option<String>,
}

/// A running session: 100 ms chunks of 16 kHz PCM go into `audio`, events come out.
/// Dropping it closes the connection.
pub struct LiveSession {
    pub audio: mpsc::UnboundedSender<Vec<i16>>,
    pub events: mpsc::UnboundedReceiver<LiveEvent>,
}

impl LiveSession {
    /// Ends the audio stream after every chunk already queued (an empty chunk marks it).
    pub fn finish(&self) {
        let _ = self.audio.send(Vec::new());
    }
}

#[async_trait::async_trait]
pub trait LiveConnector: Send + Sync {
    async fn connect(&self, cfg: LiveConfig) -> Result<LiveSession, ProviderError>;
}

pub struct GeminiLive;

#[async_trait::async_trait]
impl LiveConnector for GeminiLive {
    async fn connect(&self, cfg: LiveConfig) -> Result<LiveSession, ProviderError> {
        connect(cfg).await
    }
}

/// Opens the socket and sends `setup`; returns as soon as audio may be queued. Audio sent
/// before `setupComplete` is held and goes right after it. No `setupComplete` within
/// `READY_TIMEOUT` of the call → `LiveEvent::Error("timeout")`; a server close → `Error(reason)`.
/// The key travels in a header; the address is never logged.
pub async fn connect(cfg: LiveConfig) -> Result<LiveSession, ProviderError> {
    connect_within(cfg, READY_TIMEOUT).await
}

fn map_ws(e: tungstenite::Error) -> ProviderError {
    use tungstenite::Error as E;
    match e {
        E::Http(resp) => match resp.status().as_u16() {
            401 | 403 => ProviderError::Unauthorized,
            429 => ProviderError::RateLimited { retry_after: None },
            code @ 500..=599 => ProviderError::Server(code),
            code => ProviderError::BadResponse(format!("live handshake HTTP {code}")),
        },
        E::Io(_) | E::Tls(_) | E::ConnectionClosed | E::AlreadyClosed => ProviderError::Network,
        // Other variants may carry the address; keep the message generic.
        _ => ProviderError::BadResponse("live connection failed".into()),
    }
}

async fn connect_within(cfg: LiveConfig, ready_timeout: Duration) -> Result<LiveSession, ProviderError> {
    let deadline = tokio::time::Instant::now() + ready_timeout;
    let mut req = cfg
        .url
        .as_str()
        .into_client_request()
        .map_err(|_| ProviderError::BadResponse("bad live address".into()))?;
    let key = HeaderValue::from_str(&cfg.key).map_err(|_| ProviderError::Unauthorized)?;
    req.headers_mut().insert("x-goog-api-key", key);
    let (ws, _) = tokio::time::timeout_at(deadline, tokio_tungstenite::connect_async(req))
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(map_ws)?;
    let (mut sink, mut stream) = ws.split();
    let setup = setup_message(&cfg.model, cfg.mode, cfg.language.as_deref());
    sink.send(Message::Text(setup.to_string())).await.map_err(map_ws)?;

    let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<Vec<i16>>();
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut ready = false;
        let mut held: Vec<Vec<i16>> = Vec::new();
        let fail = |reason: String| {
            let _ = events_tx.send(LiveEvent::Error(reason));
        };
        loop {
            tokio::select! {
                msg = stream.next() => {
                    let text = match msg {
                        Some(Ok(Message::Text(t))) => t,
                        // The server sends its JSON in binary frames too.
                        Some(Ok(Message::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
                        Some(Ok(Message::Close(frame))) => {
                            let reason = frame.map(|f| f.reason.into_owned()).filter(|r| !r.is_empty());
                            return fail(reason.unwrap_or_else(|| "closed".into()));
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(_)) | None => return fail("connection lost".into()),
                    };
                    for ev in parse_server(&text) {
                        if ev == LiveEvent::Ready && !ready {
                            ready = true;
                            for chunk in held.drain(..) {
                                let msg = if chunk.is_empty() { end_message() } else { audio_message(&chunk) };
                                if sink.send(Message::Text(msg.to_string())).await.is_err() {
                                    return fail("connection lost".into());
                                }
                            }
                        }
                        if events_tx.send(ev).is_err() {
                            let _ = sink.close().await;
                            return;
                        }
                    }
                }
                chunk = audio_rx.recv() => match chunk {
                    Some(chunk) if ready => {
                        let msg = if chunk.is_empty() { end_message() } else { audio_message(&chunk) };
                        if sink.send(Message::Text(msg.to_string())).await.is_err() {
                            return fail("connection lost".into());
                        }
                    }
                    Some(chunk) => held.push(chunk),
                    // The session was dropped.
                    None => {
                        let _ = sink.close().await;
                        return;
                    }
                },
                _ = tokio::time::sleep_until(deadline), if !ready => {
                    let _ = sink.close().await;
                    return fail("timeout".into());
                }
            }
        }
    });
    Ok(LiveSession { audio: audio_tx, events: events_rx })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::WebSocketStream;

    type Server = WebSocketStream<tokio::net::TcpStream>;

    /// A local server accepting one connection; returns the URL and the accepted socket.
    async fn server() -> (String, tokio::task::JoinHandle<Server>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws/live", listener.local_addr().unwrap());
        let accept = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(tcp).await.unwrap()
        });
        (url, accept)
    }

    fn cfg(url: &str) -> LiveConfig {
        LiveConfig { url: url.into(), key: "secret-key".into(), model: "m".into(), mode: TranscribeMode::Smart, language: Some("ru".into()) }
    }

    async fn read_json(ws: &mut Server) -> Value {
        loop {
            match ws.next().await.unwrap().unwrap() {
                Message::Text(t) => return serde_json::from_str(&t).unwrap(),
                Message::Binary(b) => return serde_json::from_slice(&b).unwrap(),
                _ => continue,
            }
        }
    }

    async fn say(ws: &mut Server, v: Value) {
        ws.send(Message::Text(v.to_string())).await.unwrap();
    }

    fn pcm_of(v: &Value) -> Vec<i16> {
        let data = v["realtimeInput"]["audio"]["data"].as_str().expect("an audio message");
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
        bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect()
    }

    async fn next_event(s: &mut LiveSession) -> LiveEvent {
        tokio::time::timeout(Duration::from_secs(2), s.events.recv()).await.expect("event in time").expect("open")
    }

    #[tokio::test]
    async fn sends_setup_first_then_audio() {
        let (url, accept) = server().await;
        let session = connect(cfg(&url)).await.unwrap();
        let mut ws = accept.await.unwrap();
        let first = read_json(&mut ws).await;
        assert_eq!(first["setup"]["model"], "models/m");
        say(&mut ws, json!({"setupComplete": {}})).await;
        session.audio.send(vec![1, 2]).unwrap();
        assert_eq!(pcm_of(&read_json(&mut ws).await), vec![1, 2]);
    }

    #[tokio::test]
    async fn audio_before_ready_is_held_and_flushed() {
        let (url, accept) = server().await;
        let session = connect(cfg(&url)).await.unwrap();
        for i in 1..=3 {
            session.audio.send(vec![i]).unwrap();
        }
        let mut ws = accept.await.unwrap();
        assert!(read_json(&mut ws).await.get("setup").is_some());
        tokio::time::sleep(Duration::from_millis(200)).await;
        say(&mut ws, json!({"setupComplete": {}})).await;
        session.audio.send(vec![4]).unwrap();
        let mut got = Vec::new();
        for _ in 0..4 {
            got.extend(pcm_of(&read_json(&mut ws).await));
        }
        assert_eq!(got, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn events_arrive_in_order() {
        let (url, accept) = server().await;
        let mut session = connect(cfg(&url)).await.unwrap();
        let mut ws = accept.await.unwrap();
        read_json(&mut ws).await;
        say(&mut ws, json!({"setupComplete": {}})).await;
        say(&mut ws, json!({"serverContent": {"interimInputTranscription": {"text": "при"}}})).await;
        // Binary frames carry JSON too.
        let binary = json!({"serverContent": {"interimInputTranscription": {"text": "привет"}}}).to_string();
        ws.send(Message::Binary(binary.into_bytes())).await.unwrap();
        say(&mut ws, json!({"serverContent": {"inputTranscription": {"text": "Привет."}}})).await;
        assert_eq!(next_event(&mut session).await, LiveEvent::Ready);
        assert_eq!(next_event(&mut session).await, LiveEvent::Interim("при".into()));
        assert_eq!(next_event(&mut session).await, LiveEvent::Interim("привет".into()));
        assert_eq!(next_event(&mut session).await, LiveEvent::Segment("Привет.".into()));
    }

    #[tokio::test]
    async fn finish_sends_stream_end_after_audio() {
        let (url, accept) = server().await;
        let session = connect(cfg(&url)).await.unwrap();
        let mut ws = accept.await.unwrap();
        read_json(&mut ws).await;
        session.audio.send(vec![7]).unwrap();
        session.audio.send(vec![8]).unwrap();
        session.finish();
        say(&mut ws, json!({"setupComplete": {}})).await;
        assert_eq!(pcm_of(&read_json(&mut ws).await), vec![7]);
        assert_eq!(pcm_of(&read_json(&mut ws).await), vec![8]);
        assert_eq!(read_json(&mut ws).await, end_message());
    }

    #[tokio::test]
    async fn no_setup_complete_times_out() {
        let (url, accept) = server().await;
        let t = std::time::Instant::now();
        let mut session = connect_within(cfg(&url), Duration::from_millis(300)).await.unwrap();
        let mut ws = accept.await.unwrap();
        read_json(&mut ws).await;
        assert_eq!(next_event(&mut session).await, LiveEvent::Error("timeout".into()));
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    }

    #[tokio::test]
    async fn handshake_that_never_finishes_is_a_timeout() {
        // A server that accepts TCP but never answers the WebSocket upgrade.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let _hold = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(10)).await;
            drop(tcp);
        });
        let r = connect_within(cfg(&url), Duration::from_millis(300)).await;
        assert_eq!(r.err(), Some(ProviderError::Timeout));
    }

    #[tokio::test]
    async fn server_close_is_error_event() {
        let (url, accept) = server().await;
        let mut session = connect(cfg(&url)).await.unwrap();
        let mut ws = accept.await.unwrap();
        read_json(&mut ws).await;
        let reason = "User location is not supported for the API use.";
        ws.close(Some(CloseFrame { code: CloseCode::Policy, reason: reason.into() })).await.unwrap();
        assert_eq!(next_event(&mut session).await, LiveEvent::Error(reason.into()));
    }

    #[tokio::test]
    async fn key_is_sent_in_header_not_url() {
        use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws/live", listener.local_addr().unwrap());
        let seen = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut seen = None;
            #[allow(clippy::result_large_err)] // the callback's type is set by tungstenite
            let cb = |req: &Request, resp: Response| {
                let key = req.headers().get("x-goog-api-key").map(|v| v.to_str().unwrap().to_string());
                seen = Some((key, req.uri().to_string()));
                Ok(resp)
            };
            let _ws = tokio_tungstenite::accept_hdr_async(tcp, cb).await.unwrap();
            seen.unwrap()
        });
        let _session = connect(cfg(&url)).await.unwrap();
        let (key, uri) = seen.await.unwrap();
        assert_eq!(key.as_deref(), Some("secret-key"));
        assert!(!uri.contains("key="), "{uri}");
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests-data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn setup_smart_ru() {
        let expected: Value = serde_json::from_str(
            r#"{"setup":{"model":"models/gemini-3.5-transcribe-live","generationConfig":{"responseModalities":["TEXT"]},"inputAudioTranscription":{"mode":"SMART","languageCodes":["ru-RU"]}}}"#,
        )
        .unwrap();
        assert_eq!(setup_message("gemini-3.5-transcribe-live", TranscribeMode::Smart, Some("ru")), expected);
    }

    #[test]
    fn setup_auto_language_is_empty_list() {
        let v = setup_message("m", TranscribeMode::Verbatim, None);
        assert_eq!(v["setup"]["inputAudioTranscription"], json!({"mode": "VERBATIM", "languageCodes": []}));
    }

    #[test]
    fn audio_message_is_base64_le_pcm() {
        let v = audio_message(&[1, -2]);
        assert_eq!(v["realtimeInput"]["audio"]["data"], "AQD+/w==");
        assert_eq!(v["realtimeInput"]["audio"]["mimeType"], "audio/pcm;rate=16000");
        assert_eq!(end_message(), json!({"realtimeInput": {"audioStreamEnd": true}}));
    }

    #[test]
    fn parse_interim_and_final_fixtures() {
        assert_eq!(parse_server(&fixture("live-setup-complete.json")), vec![LiveEvent::Ready]);
        assert_eq!(
            parse_server(&fixture("live-interim.json")),
            vec![LiveEvent::Interim("Привет, слушай, я посмотрела".into())]
        );
        assert_eq!(
            parse_server(&fixture("live-final.json")),
            vec![
                LiveEvent::Segment("Привет. Слушай, я посмотрела отчёт по продажам за сентябрь.".into()),
                LiveEvent::TurnComplete
            ]
        );
    }

    #[test]
    fn parse_ignores_unknown_fields() {
        assert!(parse_server(r#"{"usageMetadata":{"totalTokenCount":5},"serverContent":{"modelTurn":{}}}"#).is_empty());
        assert!(parse_server("not json").is_empty());
    }

    #[test]
    fn parse_error_message() {
        assert_eq!(
            parse_server(&fixture("live-error.json")),
            vec![LiveEvent::Error("User location is not supported for the API use.".into())]
        );
    }

    #[test]
    fn language_codes() {
        assert_eq!(language_code(Some("ru")).as_deref(), Some("ru-RU"));
        assert_eq!(language_code(Some("en")).as_deref(), Some("en-US"));
        assert_eq!(language_code(Some("de")).as_deref(), Some("de-DE"));
        assert_eq!(language_code(Some("xx")).as_deref(), Some("xx"));
        assert_eq!(language_code(None), None);
    }
}
