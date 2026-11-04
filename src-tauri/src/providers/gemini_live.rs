//! Gemini Live API (WebSocket `BidiGenerateContent`) messages: setup, audio, events.
//! Pure functions; the connection itself lives in `live`.

use base64::Engine;
use serde_json::{json, Value};

use super::gemini_language_code;

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

#[cfg(test)]
mod tests {
    use super::*;

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
