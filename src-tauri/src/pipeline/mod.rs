//! Speech → text → (cleanup | translation). Pure orchestration over provider traits.

use crate::providers::{ProviderError, SpeechToText, TextModel};
use crate::settings::TranslateStyle;
use crate::types::Mode;

pub const PARAGRAPH_MIN_SECS: f32 = 30.0;

const RULES: &str = include_str!("prompts/rules.md");
const CLEANUP: &str = include_str!("prompts/cleanup.md");
const TRANSLATE_BUSINESS: &str = include_str!("prompts/translate_business.md");
const TRANSLATE_CASUAL: &str = include_str!("prompts/translate_casual.md");
const HALLUCINATIONS: &str = include_str!("hallucinations.txt");
const PARAGRAPHS: &str =
    "Split the text into paragraphs by meaning, separated by a blank line.";

#[derive(Clone, Debug, PartialEq)]
pub struct PipelineOptions {
    /// Whisper language hint; `None` means auto-detect.
    pub language: Option<String>,
    pub cleanup: bool,
    pub paragraphs: bool,
    pub style: TranslateStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobOutput {
    pub text: String,
    pub raw: String,
    /// The LLM step failed and the raw transcript is returned instead.
    pub degraded: bool,
    pub tokens: u32,
    pub llm_used: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobError {
    NothingHeard,
    Provider(ProviderError),
}

/// `None` when no LLM step is needed (dictation with cleanup off).
pub fn system_prompt(mode: Mode, opts: &PipelineOptions, audio_secs: f32) -> Option<String> {
    let task = match mode {
        Mode::Dictation if !opts.cleanup => return None,
        Mode::Dictation => CLEANUP,
        Mode::Translate => match opts.style {
            TranslateStyle::Business => TRANSLATE_BUSINESS,
            TranslateStyle::Casual => TRANSLATE_CASUAL,
        },
    };
    let mut prompt = format!("{}\n{}", task.trim(), RULES.trim());
    if opts.paragraphs && audio_secs > PARAGRAPH_MIN_SECS {
        prompt.push_str("\n- ");
        prompt.push_str(PARAGRAPHS);
    }
    Some(prompt)
}

pub fn user_message(transcript: &str) -> String {
    format!("<transcript>\n{transcript}\n</transcript>")
}

fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Phrases Whisper invents on silence or noise.
pub fn is_hallucination(text: &str) -> bool {
    let t = normalize(text);
    if t.starts_with("субтитры") || t.starts_with("редактор субтитров") {
        return true;
    }
    HALLUCINATIONS.lines().map(str::trim).filter(|l| !l.is_empty()).any(|l| t == l)
}

/// Removes framing an LLM sometimes adds despite instructions.
pub fn postprocess(llm_text: &str) -> String {
    let mut t = llm_text.trim().to_string();
    if let Some(inner) = t.strip_prefix("<transcript>").and_then(|r| r.strip_suffix("</transcript>")) {
        t = inner.trim().to_string();
    }
    if let Some((first, rest)) = t.split_once('\n') {
        let f = first.trim();
        let preface = ["Here is", "Here's", "Sure", "Вот"].iter().any(|p| f.starts_with(p));
        if preface && f.ends_with(':') {
            t = rest.trim().to_string();
        }
    }
    for (open, close) in [('"', '"'), ('«', '»'), ('“', '”')] {
        let inner = t.strip_prefix(open).and_then(|r| r.strip_suffix(close));
        if let Some(inner) = inner {
            if !inner.contains(open) && !inner.contains(close) {
                t = inner.trim().to_string();
                break;
            }
        }
    }
    t
}

pub async fn run(
    mode: Mode,
    wav: Vec<u8>,
    audio_secs: f32,
    opts: &PipelineOptions,
    stt: &dyn SpeechToText,
    llm: &dyn TextModel,
) -> Result<JobOutput, JobError> {
    let raw = stt
        .transcribe(wav, opts.language.clone(), audio_secs)
        .await
        .map_err(JobError::Provider)?
        .trim()
        .to_string();
    if raw.is_empty() || is_hallucination(&raw) {
        return Err(JobError::NothingHeard);
    }
    let Some(system) = system_prompt(mode, opts, audio_secs) else {
        return Ok(JobOutput { text: raw.clone(), raw, degraded: false, tokens: 0, llm_used: false });
    };
    let degraded = |raw: String| JobOutput { text: raw.clone(), raw, degraded: true, tokens: 0, llm_used: true };
    match llm.complete(&system, &user_message(&raw)).await {
        Ok(c) => {
            let text = postprocess(&c.text);
            if text.is_empty() {
                return Ok(degraded(raw));
            }
            Ok(JobOutput { text, raw, degraded: false, tokens: c.total_tokens, llm_used: true })
        }
        Err(_) => Ok(degraded(raw)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Completion, ProviderError, SpeechToText, TextModel};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeStt(Result<String, ProviderError>);
    #[async_trait::async_trait]
    impl SpeechToText for FakeStt {
        async fn transcribe(&self, _: Vec<u8>, _: Option<String>, _: f32) -> Result<String, ProviderError> {
            self.0.clone()
        }
    }

    struct FakeLlm {
        reply: Result<Completion, ProviderError>,
        calls: AtomicUsize,
    }
    impl FakeLlm {
        fn ok(text: &str) -> Self {
            Self { reply: Ok(Completion { text: text.into(), total_tokens: 7 }), calls: AtomicUsize::new(0) }
        }
        fn err(e: ProviderError) -> Self {
            Self { reply: Err(e), calls: AtomicUsize::new(0) }
        }
    }
    #[async_trait::async_trait]
    impl TextModel for FakeLlm {
        async fn complete(&self, _: &str, _: &str) -> Result<Completion, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reply.clone()
        }
    }

    fn opts(cleanup: bool, paragraphs: bool, style: TranslateStyle) -> PipelineOptions {
        PipelineOptions { language: Some("ru".into()), cleanup, paragraphs, style }
    }

    #[test]
    fn dictation_without_cleanup_has_no_prompt() {
        assert!(system_prompt(Mode::Dictation, &opts(false, true, TranslateStyle::Business), 60.0).is_none());
        assert!(system_prompt(Mode::Dictation, &opts(true, false, TranslateStyle::Business), 5.0).is_some());
        // Translation always needs the LLM, even with cleanup off.
        assert!(system_prompt(Mode::Translate, &opts(false, false, TranslateStyle::Casual), 5.0).is_some());
    }

    #[test]
    fn paragraphs_only_when_enabled_and_long() {
        let has = |p: bool, secs: f32| {
            system_prompt(Mode::Dictation, &opts(true, p, TranslateStyle::Business), secs)
                .unwrap()
                .contains("paragraphs")
        };
        assert!(has(true, 31.0));
        assert!(!has(true, 30.0));
        assert!(!has(false, 120.0));
    }

    #[test]
    fn translate_prompt_depends_on_style() {
        let b = system_prompt(Mode::Translate, &opts(true, false, TranslateStyle::Business), 5.0).unwrap();
        let c = system_prompt(Mode::Translate, &opts(true, false, TranslateStyle::Casual), 5.0).unwrap();
        assert_ne!(b, c);
        assert!(b.contains("English") && c.contains("English"));
    }

    #[test]
    fn user_message_wraps_transcript() {
        assert_eq!(user_message("hi"), "<transcript>\nhi\n</transcript>");
    }

    #[test]
    fn hallucination_matches_normalized() {
        assert!(is_hallucination("Продолжение следует..."));
        assert!(is_hallucination("  Thanks for watching!  "));
        assert!(is_hallucination("Субтитры сделал DimaTorzok"));
        assert!(is_hallucination("Редактор субтитров А.Синецкая"));
        assert!(is_hallucination("You"));
        assert!(!is_hallucination("Спасибо за просмотр, коллеги, сейчас отправлю отчёт"));
        assert!(!is_hallucination("you are right"));
    }

    #[test]
    fn postprocess_strips_wrappers() {
        assert_eq!(postprocess("Here is the translation:\n\"Hello\""), "Hello");
        assert_eq!(postprocess("<transcript>Hi</transcript>"), "Hi");
        assert_eq!(postprocess("<transcript>\nHi there\n</transcript>"), "Hi there");
        assert_eq!(postprocess("«Привет»"), "Привет");
        assert_eq!(postprocess("Вот исправленный текст:\nПривет, мир."), "Привет, мир.");
        assert_eq!(postprocess("Hello \"world\""), "Hello \"world\"");
        assert_eq!(postprocess("\"Hi\" she said, \"bye\""), "\"Hi\" she said, \"bye\"");
        assert_eq!(postprocess("  Line one\n\nLine two  "), "Line one\n\nLine two");
    }

    #[tokio::test]
    async fn run_dictation_with_cleanup_uses_llm() {
        let llm = FakeLlm::ok("Привет, мир.");
        let out = run(Mode::Dictation, vec![], 3.0, &opts(true, false, TranslateStyle::Business),
            &FakeStt(Ok("ну привет мир".into())), &llm).await.unwrap();
        assert_eq!(out, JobOutput { text: "Привет, мир.".into(), raw: "ну привет мир".into(),
            degraded: false, tokens: 7, llm_used: true });
    }

    #[tokio::test]
    async fn run_dictation_without_cleanup_skips_llm() {
        let llm = FakeLlm::ok("unused");
        let out = run(Mode::Dictation, vec![], 3.0, &opts(false, false, TranslateStyle::Business),
            &FakeStt(Ok("сырой текст".into())), &llm).await.unwrap();
        assert_eq!(out.text, "сырой текст");
        assert!(!out.llm_used);
        assert_eq!(llm.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn run_llm_failure_degrades_to_raw() {
        let out = run(Mode::Translate, vec![], 3.0, &opts(true, false, TranslateStyle::Casual),
            &FakeStt(Ok("привет".into())), &FakeLlm::err(ProviderError::Timeout)).await.unwrap();
        assert_eq!(out.text, "привет");
        assert!(out.degraded);
    }

    #[tokio::test]
    async fn run_empty_llm_output_degrades_to_raw() {
        let out = run(Mode::Dictation, vec![], 3.0, &opts(true, false, TranslateStyle::Business),
            &FakeStt(Ok("привет".into())), &FakeLlm::ok("  \"\"  ")).await.unwrap();
        assert_eq!(out.text, "привет");
        assert!(out.degraded);
    }

    #[tokio::test]
    async fn run_hallucination_is_nothing_heard() {
        let llm = FakeLlm::ok("x");
        let r = run(Mode::Dictation, vec![], 3.0, &opts(true, false, TranslateStyle::Business),
            &FakeStt(Ok("Продолжение следует...".into())), &llm).await;
        assert_eq!(r, Err(JobError::NothingHeard));
        assert_eq!(llm.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn run_empty_transcript_is_nothing_heard() {
        let r = run(Mode::Dictation, vec![], 3.0, &opts(true, false, TranslateStyle::Business),
            &FakeStt(Ok("   ".into())), &FakeLlm::ok("x")).await;
        assert_eq!(r, Err(JobError::NothingHeard));
    }

    #[tokio::test]
    async fn run_stt_error_propagates() {
        let r = run(Mode::Dictation, vec![], 3.0, &opts(true, false, TranslateStyle::Business),
            &FakeStt(Err(ProviderError::Unauthorized)), &FakeLlm::ok("x")).await;
        assert_eq!(r, Err(JobError::Provider(ProviderError::Unauthorized)));
    }
}
