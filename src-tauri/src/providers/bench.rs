//! Live latency benchmark against Gemini with the user's key from the Keychain.
//! Run: cargo test --release --lib -- --ignored gemini_latency --nocapture

use std::sync::Arc;
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
        let mut last_interim = String::new();
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
                    last_interim = t.clone();
                    if first_interim.is_none() {
                        first_interim = Some(t0.elapsed().as_millis());
                        println!("  first interim {} ms: {t}", t0.elapsed().as_millis());
                    }
                }
                Ok(Some(LiveEvent::Segment(t))) => {
                    let after = released.map(|r| r.elapsed().as_millis() as i64).unwrap_or(-1);
                    println!("  last interim: {last_interim}");
                    println!("  segment (release→ {after} ms): {t}");
                    let (a, b) = (crate::pipeline::normalize(&last_interim), crate::pipeline::normalize(&t));
                    println!("  same words: {} | interim «{a}» | final «{b}»", a == b);
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

#[tokio::test]
#[ignore = "live: translation latency of candidate text models with the user's key"]
async fn translate_models_latency() {
    use crate::providers::{defaults, GeminiClient, TextModel};
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style: TranslateStyle::Business };
    let system = system_prompt(Mode::Translate, &opts, 5.0).unwrap();
    let phrases = [
        "Привет, слушай, я посмотрела отчёт по продажам за сентябрь.",
        "В целом всё хорошо, но давай завтра созвонимся.",
        "Обсудим бюджет на рекламу и сроки запуска.",
        "Пришли, пожалуйста, презентацию до пятницы.",
        "Я думаю, что нам нужно нанять ещё одного дизайнера.",
    ];
    for model in ["gemini-3.5-flash-lite", "gemini-3.6-flash", "gemini-3.7-flash", "gemini-3.8-flash"] {
        let mut cfg = defaults::config_for(ProviderKind::Gemini, key.clone());
        cfg.text_model = model.into();
        let client = GeminiClient::new(cfg);
        client.prewarm().await;
        let mut times = Vec::new();
        for p in phrases {
            let t = Instant::now();
            match client.complete(&system, &user_message(p)).await {
                Ok(c) => {
                    times.push(t.elapsed().as_millis());
                    println!("  {model} {} ms: {}", t.elapsed().as_millis(), c.text.trim());
                }
                Err(e) => println!("  {model} error after {} ms: {e:?}", t.elapsed().as_millis()),
            }
        }
        times.sort();
        println!("{model}: median {:?} ms of {} ok", times.get(times.len() / 2), times.len());
    }
}

#[tokio::test]
#[ignore = "live: raw 429 details and the model list with the user's key"]
async fn quota_details() {
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let http = reqwest::Client::new();
    let models: Value = http.get(format!("{BASE}/models?pageSize=200")).header("x-goog-api-key", &key).send().await.unwrap().json().await.unwrap();
    for m in models["models"].as_array().into_iter().flatten() {
        let name = m["name"].as_str().unwrap_or("");
        if name.contains("flash") || name.contains("pro") || name.contains("transcribe") {
            println!("model {name} methods={}", m["supportedGenerationMethods"]);
        }
    }
    for model in ["gemini-3.6-flash", "gemini-3.8-flash"] {
        let body = json!({"model": model, "input": "Say hi", "store": false, "generation_config": {"thinking_level": "low"}});
        let r = http.post(format!("{BASE}/interactions")).header("x-goog-api-key", &key).json(&body).send().await.unwrap();
        let status = r.status();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        println!("{model}: HTTP {status}\n{}", serde_json::to_string_pretty(&v.get("error").cloned().unwrap_or(v.clone())).unwrap());
    }
}

#[tokio::test]
#[ignore = "live: candidate backup models — audio→text and text latency, limits from 429"]
async fn backup_candidates() {
    use crate::pipeline::audio_system_prompt;
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let bench = Bench { http: reqwest::Client::new(), key };
    let _ = bench.http.get(format!("{BASE}/models")).header("x-goog-api-key", &bench.key).send().await;
    let wav = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests-data/phrase-ru.wav")).unwrap();
    let audio = json!({"type": "audio", "mime_type": "audio/wav", "data": base64::engine::general_purpose::STANDARD.encode(&wav)});
    let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style: TranslateStyle::Business };
    let audio_system = audio_system_prompt(Mode::Dictation, &opts, 9.9).unwrap();
    let text_system = system_prompt(Mode::Translate, &opts, 5.0).unwrap();
    let models = ["gemini-3.5-flash", "gemini-3.1-flash-lite", "gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-flash-latest", "gemini-3-flash-preview"];
    for model in models {
        for level in [Some("minimal"), None] {
            let audio_body = json!({"model": model, "system_instruction": audio_system, "store": false,
                "input": [{"type": "text", "text": "Process the attached audio. The speaker most likely uses ru-RU."}, audio.clone()],
                "generation_config": thinking(level)});
            let (d, out) = bench.interact(audio_body).await;
            let label = format!("{model} thinking={}", level.unwrap_or("default"));
            match out {
                Err(e) if e.contains("thinking") || e.contains("Thinking") => {
                    println!("{label}: audio → {e}");
                    continue;
                }
                Err(e) => {
                    println!("{label}: audio ERROR after {} ms: {e}", d.as_millis());
                    break;
                }
                Ok(t) => println!("{label}: audio {} ms → {}", d.as_millis(), t.trim()),
            }
            for _ in 0..2 {
                let body = json!({"model": model, "system_instruction": text_system, "store": false,
                    "input": user_message("Пришли, пожалуйста, презентацию до пятницы."), "generation_config": thinking(level)});
                let (d, out) = bench.interact(body).await;
                match out {
                    Ok(t) => println!("{label}: text {} ms → {}", d.as_millis(), t.trim()),
                    Err(e) => println!("{label}: text ERROR after {} ms: {e}", d.as_millis()),
                }
            }
            break;
        }
    }
}

/// Spread of the cleanup request at the end of a dictation: both Lite models get the same text at
/// once (as the app's hedge would), rounds spaced to stay under 15 requests/minute per model.
/// Run: cargo test --release --lib -- --ignored cleanup_latency_spread --nocapture
#[tokio::test]
#[ignore = "live: ~34 cleanup requests with the user's key"]
async fn cleanup_latency_spread() {
    use crate::providers::{defaults, gemini_http_client, GeminiClient, TextModel};
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style: TranslateStyle::Business };
    let system = system_prompt(Mode::Dictation, &opts, 0.0).unwrap();
    let texts = [
        "ну и в общем давай завтра созвонимся часа в три",
        "слушай я тут посмотрел отчёт по продажам за сентябрь и ну в целом всё неплохо но вот по рекламе мы как-то э-э перерасходовали бюджет процентов на двадцать наверное и я думаю что надо бы это обсудить с маркетингом до конца недели потому что иначе в октябре будет та же история",
    ];
    let http = gemini_http_client();
    let client = |model: &str| {
        let mut cfg = defaults::config_for(ProviderKind::Gemini, key.clone());
        cfg.text_model = model.into();
        Arc::new(GeminiClient::with_client(cfg, http.clone()))
    };
    let (a, b, c) = (client(defaults::GEMINI_TEXT_MODEL), client(defaults::GEMINI_BACKUP_LITE_MODEL), client(defaults::GEMINI_BACKUP_MODEL));
    a.prewarm().await;
    let time = |m: Arc<GeminiClient>, text: &'static str, system: String| async move {
        let t = Instant::now();
        let r = m.complete(&system, &user_message(text)).await;
        (t.elapsed().as_millis() as u64, r.map(|c| c.text).map_err(|e| format!("{e:?}")))
    };
    let mut rows: Vec<(u64, u64, Option<u64>)> = Vec::new();
    for round in 0..16 {
        let started = Instant::now();
        let text = texts[round % 2];
        let with_c = round % 4 == 3;
        let fc = async { if with_c { Some(time(c.clone(), text, system.clone()).await) } else { None } };
        let ((ta, ra), (tb, rb), rc) = tokio::join!(time(a.clone(), text, system.clone()), time(b.clone(), text, system.clone()), fc);
        let short = |r: &Result<String, String>| match r { Ok(t) => t.chars().take(50).collect::<String>(), Err(e) => format!("ERR {e}") };
        println!("round {round:>2} {:<5}  3.5-lite {ta:>5} ms  3.1-lite {tb:>5} ms  {}  | {}", if round % 2 == 0 { "short" } else { "long" },
            rc.as_ref().map(|(t, _)| format!("3.6-flash {t:>5} ms")).unwrap_or_default(), short(&ra));
        if rb.is_err() { println!("           3.1-lite: {}", short(&rb)); }
        rows.push((if ra.is_ok() { ta } else { u64::MAX }, if rb.is_ok() { tb } else { u64::MAX }, rc.map(|(t, r)| if r.is_ok() { t } else { u64::MAX })));
        let spent = started.elapsed();
        if spent < Duration::from_secs(5) { tokio::time::sleep(Duration::from_secs(5) - spent).await; }
    }
    let stat = |mut v: Vec<u64>, name: &str| {
        v.sort();
        let n = v.len();
        let under = |ms| v.iter().filter(|&&t| t <= ms).count();
        println!("{name:<22} n={n} median {} ms, max {} ms, ≤1 s {}/{n}, ≤1.5 s {}/{n}", v[n / 2], v[n - 1], under(1000), under(1500));
    };
    stat(rows.iter().map(|r| r.0).collect(), "3.5-flash-lite");
    stat(rows.iter().map(|r| r.1).collect(), "3.1-flash-lite");
    stat(rows.iter().map(|r| r.0.min(r.1)).collect(), "first of both Lite");
    stat(rows.iter().filter_map(|r| r.2).collect(), "3.6-flash");
}

/// Round-trip floor (a request with no model work) and the speed of candidate fast text models
/// on the last phrase of a dictation: cleanup and business translation.
/// Run: cargo test --release --lib -- --ignored fast_text_candidates --nocapture
#[tokio::test]
#[ignore = "live: ~50 short requests with the user's key"]
async fn fast_text_candidates() {
    let key = KeychainStore::new().get(ProviderKind::Gemini).expect("Gemini key in Keychain");
    let bench = Bench { http: reqwest::Client::new(), key };
    let models: Value = bench.http.get(format!("{BASE}/models?pageSize=200")).header("x-goog-api-key", &bench.key).send().await.unwrap().json().await.unwrap();
    let gemma: Vec<String> = models["models"].as_array().into_iter().flatten()
        .filter_map(|m| m["name"].as_str()).filter(|n| n.contains("gemma")).map(|n| n.trim_start_matches("models/").to_string()).collect();
    println!("gemma models: {}", gemma.join(", "));
    let mut rtt = Vec::new();
    for _ in 0..6 {
        let t = Instant::now();
        let _ = bench.http.get(format!("{BASE}/models/gemini-3.5-flash-lite")).header("x-goog-api-key", &bench.key).send().await.map(|r| r.status());
        rtt.push(t.elapsed().as_millis());
    }
    println!("round trip (GET model info, warm): {rtt:?} ms");
    let opts = PipelineOptions { language: Some("ru".into()), cleanup: true, paragraphs: false, style: TranslateStyle::Business };
    let cleanup = system_prompt(Mode::Dictation, &opts, 0.0).unwrap();
    let translate = system_prompt(Mode::Translate, &opts, 0.0).unwrap();
    let phrase = "ну и в общем давай завтра созвонимся часа в три";
    let mut candidates: Vec<(String, Option<&str>)> = vec![("gemini-3.5-flash-lite".into(), Some("minimal"))];
    for g in gemma.iter().filter(|g| g.starts_with("gemma-4")) {
        candidates.push((g.clone(), None));
    }
    let mut times: Vec<Vec<u128>> = vec![Vec::new(); candidates.len()];
    for round in 0..8 {
        let started = Instant::now();
        let (label, sys) = if round % 2 == 0 { ("cleanup", &cleanup) } else { ("translate", &translate) };
        for (i, (model, level)) in candidates.iter().enumerate() {
            let body = json!({"model": model, "store": false, "system_instruction": sys, "input": user_message(phrase), "generation_config": thinking(*level)});
            let (d, out) = bench.interact(body).await;
            match out {
                Ok(t) => { times[i].push(d.as_millis()); println!("round {round} {label:<9} {model:<28} {:>5} ms → {}", d.as_millis(), t.trim().replace('\n', " ")); }
                Err(e) => println!("round {round} {label:<9} {model:<28} ERROR after {} ms: {}", d.as_millis(), e.chars().take(160).collect::<String>()),
            }
        }
        let spent = started.elapsed();
        if spent < Duration::from_millis(4500) { tokio::time::sleep(Duration::from_millis(4500) - spent).await; }
    }
    for ((model, _), mut t) in candidates.iter().zip(times) {
        t.sort();
        if t.is_empty() { println!("{model}: no answers"); continue; }
        println!("{model:<28} n={} median {} ms, max {} ms, ≤1 s {}", t.len(), t[t.len() / 2], t[t.len() - 1], t.iter().filter(|&&x| x <= 1000).count());
    }
}
