//! Live latency benchmark against Gemini with the user's key from the Keychain.
//! Run: cargo test --release --lib -- --ignored gemini_latency --nocapture

use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Value};

use crate::pipeline::{system_prompt, user_message, PipelineOptions};
use crate::secrets::{KeychainStore, SecretStore};
use crate::settings::{ProviderKind, TranslateStyle};
use crate::types::Mode;

const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const RUNS: usize = 3;

struct Bench {
    http: reqwest::Client,
    key: String,
}

impl Bench {
    async fn interact(&self, body: Value) -> (Duration, Result<String, String>) {
        let t = Instant::now();
        let resp = self
            .http
            .post(format!("{BASE}/interactions"))
            .header("x-goog-api-key", &self.key)
            .timeout(Duration::from_secs(60))
            .json(&body)
            .send()
            .await;
        let out = match resp {
            Err(e) => Err(e.to_string()),
            Ok(r) => {
                let status = r.status();
                let v: Value = r.json().await.unwrap_or(Value::Null);
                if !status.is_success() {
                    Err(format!("HTTP {status}: {}", v.pointer("/error/message").and_then(Value::as_str).unwrap_or("")))
                } else {
                    Ok(v.get("output_text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
                        v.get("steps").map(|s| s.to_string()).unwrap_or_default()
                    }))
                }
            }
        };
        (t.elapsed(), out)
    }

    async fn run(&self, name: &str, body: Value) -> Option<String> {
        let mut times = Vec::new();
        let mut first = None;
        for _ in 0..RUNS {
            let (d, out) = self.interact(body.clone()).await;
            match out {
                Ok(text) => {
                    times.push(d.as_millis());
                    first.get_or_insert(text);
                }
                Err(e) => {
                    println!("{name:<52} ERROR {e}");
                    return None;
                }
            }
        }
        times.sort();
        let text = first.unwrap_or_default();
        println!("{name:<52} median {:>5} ms  (min {:>5})  → {}", times[RUNS / 2], times[0], text.replace('\n', " ⏎ "));
        Some(text)
    }
}

fn thinking(level: Option<&str>) -> Value {
    match level {
        Some(l) => json!({"thinking_level": l, "temperature": 0.2}),
        None => json!({"temperature": 0.2}),
    }
}

#[tokio::test]
#[ignore = "live network benchmark; reads the Gemini key from the Keychain"]
async fn gemini_latency() {
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let bench = Bench { http: reqwest::Client::new(), key };
    let wav = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data/phrase-ru.wav")).unwrap();
    let audio = json!({"type": "audio", "mime_type": "audio/wav",
        "data": base64::engine::general_purpose::STANDARD.encode(&wav)});

    let models = bench
        .http
        .get(format!("{BASE}/models?pageSize=200"))
        .header("x-goog-api-key", &bench.key)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let names: Vec<&str> = models["models"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["name"].as_str()).collect())
        .unwrap_or_default();
    println!("models: {}", names.iter().filter(|n| n.contains("flash") || n.contains("transcribe")).cloned().collect::<Vec<_>>().join(", "));
    // Warm the connection so the first variant is not penalised by TLS setup.
    let _ = bench.http.get(format!("{BASE}/models")).header("x-goog-api-key", &bench.key).send().await;

    println!("\n== Speech-to-text (9.9 s Russian phrase) ==");
    let raw = bench
        .run("transcribe: gemini-3.5-transcribe", json!({"model": "gemini-3.5-transcribe", "store": false, "input": [audio.clone()],
            "generation_config": {"transcription_config": {"language_codes": ["ru-RU"]}}}))
        .await;
    let stt_prompt = "Transcribe this audio verbatim in its original language. Output only the transcript.";
    for (model, level) in [("gemini-3.5-flash-lite", Some("minimal")), ("gemini-3.6-flash", Some("minimal")), ("gemini-3.5-flash", None)] {
        bench
            .run(&format!("transcribe: {model} thinking={}", level.unwrap_or("default")),
                json!({"model": model, "store": false, "input": [{"type": "text", "text": stt_prompt}, audio.clone()],
                    "generation_config": thinking(level)}))
            .await;
    }

    let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style: TranslateStyle::Business };
    let cleanup_sys = system_prompt(Mode::Dictation, &opts, 10.0).unwrap();
    let translate_sys = system_prompt(Mode::Translate, &opts, 10.0).unwrap();
    let raw = raw.unwrap_or_else(|| "привет слушай я посмотрел отчёт по продажам за сентябрь".into());

    println!("\n== Cleanup of the transcript (text only) ==");
    for (model, level) in [("gemini-3.5-flash", None), ("gemini-3.5-flash", Some("low")), ("gemini-3.5-flash-lite", Some("minimal")), ("gemini-3.6-flash", Some("minimal"))] {
        bench
            .run(&format!("cleanup: {model} thinking={}", level.unwrap_or("default")),
                json!({"model": model, "store": false, "system_instruction": cleanup_sys, "input": user_message(&raw),
                    "generation_config": thinking(level)}))
            .await;
    }

    println!("\n== One request: audio → final text ==");
    let note = "The audio attachment is the transcript source: transcribe it, then apply the instructions to the transcription.";
    for (label, sys) in [("cleanup", &cleanup_sys), ("translate business", &translate_sys)] {
        for (model, level) in [("gemini-3.5-flash-lite", Some("minimal")), ("gemini-3.6-flash", Some("minimal"))] {
            bench
                .run(&format!("combined {label}: {model} thinking={}", level.unwrap_or("default")),
                    json!({"model": model, "store": false, "system_instruction": format!("{sys}\n{note}"),
                        "input": [{"type": "text", "text": "Process the attached audio."}, audio.clone()],
                        "generation_config": thinking(level)}))
                .await;
        }
    }
}

#[tokio::test]
#[ignore = "live probe of free-tier request-per-minute limits"]
async fn gemini_rpm_probe() {
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let bench = Bench { http: reqwest::Client::new(), key };
    for model in ["gemini-3.5-flash-lite", "gemini-3.1-flash-lite"] {
        let mut ok = 0;
        for i in 0..25 {
            let (_, out) = bench
                .interact(json!({"model": model, "store": false, "input": "Reply with: ok",
                    "generation_config": {"thinking_level": "minimal"}}))
                .await;
            match out {
                Ok(_) => ok += 1,
                Err(e) => {
                    println!("{model}: 429 after {ok} ok requests (request #{}) — {e}", i + 1);
                    break;
                }
            }
        }
        if ok == 25 {
            println!("{model}: 25/25 requests within a minute succeeded");
        }
    }
}

#[tokio::test]
#[ignore = "live end-to-end through the app's own pipeline with the user's key"]
async fn pipeline_direct_live() {
    use crate::providers::{defaults, GeminiClient};
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let client = GeminiClient::new(defaults::config_for(ProviderKind::Gemini, key));
    client.prewarm().await;
    let wav = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data/phrase-ru.wav")).unwrap();
    for (mode, style) in [(Mode::Dictation, TranslateStyle::Business), (Mode::Translate, TranslateStyle::Business), (Mode::Translate, TranslateStyle::Casual)] {
        let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style };
        let t = Instant::now();
        let out = crate::pipeline::run_direct(mode, wav.clone(), 9.9, &opts, &client, &client).await;
        println!("{mode:?}/{style:?}: {} ms → {:?}", t.elapsed().as_millis(), out.map(|o| (o.text, o.degraded)));
    }
    let opts = PipelineOptions { language: Some("ru".into()), cleanup: false, paragraphs: false, style: TranslateStyle::Business };
    let t = Instant::now();
    let out = crate::pipeline::run(Mode::Dictation, wav, 9.9, &opts, &client, &client).await;
    println!("Dictation without cleanup (transcribe model): {} ms → {:?}", t.elapsed().as_millis(), out.map(|o| o.text));
}

#[tokio::test]
#[ignore = "live: each backup route once with the user's key"]
async fn routes_live() {
    use crate::pipeline::routes::Route;
    use crate::providers::{defaults, GeminiClient};
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let wav = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data/phrase-ru.wav")).unwrap();
    for (mode, style) in [(Mode::Dictation, TranslateStyle::Business), (Mode::Translate, TranslateStyle::Business)] {
        let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style };
        for (i, route) in defaults::GEMINI_ROUTES.iter().enumerate() {
            let client = GeminiClient::new(defaults::route_config(ProviderKind::Gemini, key.clone(), *route));
            client.prewarm().await;
            let t = Instant::now();
            let out = match route {
                Route::Direct { .. } => crate::pipeline::run_audio_only(mode, wav.clone(), 9.9, &opts, &client).await,
                Route::TwoStep { .. } => crate::pipeline::run(mode, wav.clone(), 9.9, &opts, &client, &client).await,
            };
            println!("{mode:?} route {i} {route:?}: {} ms → {:?}", t.elapsed().as_millis(), out.map(|o| (o.text, o.degraded)));
        }
    }
}

/// Run: UTTER_LIVE_MODEL=<model> cargo test --lib -- --ignored live_phrase_latency --nocapture
#[tokio::test]
#[ignore = "live: Live API streaming with the user's key"]
async fn live_phrase_latency() {
    use crate::live::pcm::Chunker;
    use crate::providers::defaults;
    use crate::providers::gemini_live::{connect, LiveConfig, LiveEvent, TranscribeMode};
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let model = std::env::var("UTTER_LIVE_MODEL").unwrap_or_else(|_| defaults::GEMINI_LIVE_MODEL.into());
    let mut reader = hound::WavReader::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data/phrase-ru.wav")).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = reader.samples::<i16>().map(|s| s.unwrap() as f32 / i16::MAX as f32).collect();
    let mut chunker = Chunker::new(spec.sample_rate, spec.channels);
    let step = spec.sample_rate as usize * spec.channels as usize / 10;
    for mode in [TranscribeMode::Verbatim, TranscribeMode::Smart] {
        let t0 = Instant::now();
        let cfg = LiveConfig { url: defaults::GEMINI_LIVE_URL.into(), key: key.clone(), model: model.clone(), mode, language: Some("ru".into()) };
        let mut s = match connect(cfg).await {
            Ok(s) => s,
            Err(e) => return println!("{model}: connect failed: {e}"),
        };
        println!("{model} {mode:?}: socket open {} ms", t0.elapsed().as_millis());
        let audio = s.audio.clone();
        let feeder = tokio::spawn({
            let samples = samples.clone();
            let mut chunker = std::mem::replace(&mut chunker, Chunker::new(spec.sample_rate, spec.channels));
            async move {
                for block in samples.chunks(step) {
                    for c in chunker.push(block) {
                        let _ = audio.send(c);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if let Some(c) = chunker.flush() {
                    let _ = audio.send(c);
                }
                Instant::now()
            }
        });
        let mut released: Option<Instant> = None;
        let mut first_interim = None;
        let mut segments = Vec::new();
        let mut feeder = Some(feeder);
        loop {
            if let Some(f) = feeder.as_ref() {
                if f.is_finished() {
                    released = Some(feeder.take().unwrap().await.unwrap());
                    s.finish();
                }
            }
            let wait = if released.is_some() { Duration::from_secs(3) } else { Duration::from_millis(50) };
            match tokio::time::timeout(wait, s.events.recv()).await {
                Ok(Some(LiveEvent::Ready)) => println!("  ready {} ms", t0.elapsed().as_millis()),
                Ok(Some(LiveEvent::Interim(t))) => {
                    if first_interim.is_none() {
                        first_interim = Some(t0.elapsed().as_millis());
                        println!("  first interim {} ms: {t}", t0.elapsed().as_millis());
                    }
                }
                Ok(Some(LiveEvent::Segment(t))) => {
                    let after = released.map(|r| r.elapsed().as_millis() as i64).unwrap_or(-1);
                    println!("  segment (release→ {after} ms): {t}");
                    segments.push(t);
                }
                Ok(Some(LiveEvent::TurnComplete)) => println!("  turnComplete"),
                Ok(Some(LiveEvent::Error(e))) => {
                    println!("  error: {e}");
                    break;
                }
                Ok(None) => break,
                Err(_) if released.is_some() => break,
                Err(_) => {}
            }
        }
        println!("  final: {}", segments.join(" "));
    }
}
