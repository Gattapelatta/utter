//! The one line under «<проект> · Claude Code закончил»: what the agent did.
//! The provider's text model writes it from the agent's last message (3 s at most); otherwise,
//! or when it fails, the message's first sentence.

use crate::app::tray::Lang;
use crate::providers::TextModel;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

pub const SUMMARY_MAX: usize = 70;
/// Only the head of a long message goes to the provider.
pub const SEND_MAX: usize = 4_000;

const PROMPT: &str = include_str!("../pipeline/prompts/agent_summary.md");

static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static LIST_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#+|>+|[-*+]|\d+[.)])\s+").unwrap());
static SENTENCE_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?…](\s|$)").unwrap());

/// Cuts to `max` characters on a word boundary, with «…».
fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    let on_word = match head.rfind(' ') {
        Some(i) if i > 0 => head[..i].trim_end().to_string(),
        _ => head,
    };
    format!("{on_word}…")
}

/// Markdown off: headings, list marks, quotes, emphasis, code ticks, links (their text stays).
fn plain_line(line: &str) -> String {
    let line = LIST_MARK.replace(line.trim(), "");
    let line = LINK.replace_all(&line, "$1");
    line.replace(['*', '`'], "").trim().to_string()
}

/// The first sentence of the first paragraph, as plain text; `None` if there is no text.
pub fn first_sentence(text: &str, max: usize) -> Option<String> {
    let mut in_fence = false;
    let line = text
        .lines()
        .filter(|l| {
            if l.trim_start().starts_with("```") {
                in_fence = !in_fence;
                return false;
            }
            !in_fence
        })
        .map(plain_line)
        .find(|l| !l.is_empty())?;
    let sentence = match SENTENCE_END.find(&line) {
        Some(m) => line[..m.start() + m.as_str().trim_end().len()].to_string(),
        None => line,
    };
    Some(cut(&sentence, max))
}

fn language(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "русский",
        Lang::En => "English",
    }
}

/// The model's answer as one clean line, or `None` if there is nothing usable in it.
fn clean_answer(answer: &str) -> Option<String> {
    let line = answer.lines().map(str::trim).find(|l| !l.is_empty())?;
    let quotes: &[char] = &['«', '»', '"', '“', '”', '\''];
    let line = line.trim_end_matches('.').trim_matches(quotes).trim_end_matches('.').trim();
    (!line.is_empty()).then(|| cut(line, SUMMARY_MAX))
}

/// The summary line. Only `text` (the agent's last message, its first `SEND_MAX` characters)
/// goes to the provider.
pub async fn summarize(text: &str, lang: Lang, model: Option<&dyn TextModel>, timeout: Duration) -> Option<String> {
    let fallback = first_sentence(text, SUMMARY_MAX)?;
    let Some(model) = model else { return Some(fallback) };
    let head: String = text.chars().take(SEND_MAX).collect();
    let system = PROMPT.trim().replace("{lang}", language(lang));
    match tokio::time::timeout(timeout, model.complete(&system, &head)).await {
        Ok(Ok(answer)) => Some(clean_answer(&answer.text).unwrap_or(fallback)),
        _ => Some(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Completion, ProviderError, TextModel};
    use std::sync::Mutex;

    struct Fake {
        answer: Result<String, ProviderError>,
        delay: Duration,
        seen: Mutex<Option<(String, String)>>,
    }

    impl Fake {
        fn new(answer: Result<&str, ProviderError>, delay: Duration) -> Fake {
            Fake { answer: answer.map(str::to_string), delay, seen: Mutex::new(None) }
        }
    }

    #[async_trait::async_trait]
    impl TextModel for Fake {
        async fn complete(&self, system: &str, user: &str) -> Result<Completion, ProviderError> {
            *self.seen.lock().unwrap() = Some((system.to_string(), user.to_string()));
            tokio::time::sleep(self.delay).await;
            self.answer.clone().map(|text| Completion { text, total_tokens: 1 })
        }
    }

    const MSG: &str = "I've fixed the rounding in billing.ts. All 42 tests pass now.";

    #[test]
    fn empty_is_none() {
        assert_eq!(first_sentence("", SUMMARY_MAX), None);
        assert_eq!(first_sentence("  \n ``` \n```\n", SUMMARY_MAX), None);
    }

    #[test]
    fn first_sentence_of_plain_text() {
        assert_eq!(first_sentence(MSG, SUMMARY_MAX).unwrap(), "I've fixed the rounding in billing.ts.");
        assert_eq!(first_sentence("Готово\n\nДальше детали", SUMMARY_MAX).unwrap(), "Готово");
    }

    #[test]
    fn markdown_is_stripped() {
        let md = "## Summary\n\n**Fixed** the `round()` call in [billing.ts](src/billing.ts).\n\n- one\n- two";
        assert_eq!(first_sentence(md, SUMMARY_MAX).unwrap(), "Summary");
        let md = "```rust\nfn x() {}\n```\n- **Fixed** the `round()` call in [billing.ts](src/billing.ts). More.";
        assert_eq!(first_sentence(md, SUMMARY_MAX).unwrap(), "Fixed the round() call in billing.ts.");
    }

    #[test]
    fn long_text_cut_on_word() {
        let long = "Переписал весь модуль оплаты так, чтобы округление шло по банковским правилам и не теряло копейки";
        let got = first_sentence(long, SUMMARY_MAX).unwrap();
        assert!(got.chars().count() <= SUMMARY_MAX, "{got}");
        assert!(got.ends_with('…'));
        assert!(long.starts_with(got.trim_end_matches('…')));
        assert!(!got.trim_end_matches('…').ends_with(' '));
        // One giant word: cut in the middle rather than return nothing.
        let word = "a".repeat(200);
        assert_eq!(first_sentence(&word, SUMMARY_MAX).unwrap().chars().count(), SUMMARY_MAX);
    }

    #[tokio::test]
    async fn model_answer_used_and_trimmed_to_70() {
        let fake = Fake::new(Ok("  «Исправил округление в billing.ts, тесты проходят».\n"), Duration::ZERO);
        let got = summarize(MSG, Lang::Ru, Some(&fake), Duration::from_secs(3)).await.unwrap();
        assert_eq!(got, "Исправил округление в billing.ts, тесты проходят");
        let (system, user) = fake.seen.lock().unwrap().clone().unwrap();
        assert!(system.contains("Язык: русский"));
        assert_eq!(user, MSG);
        let long = "x ".repeat(100);
        let fake = Fake::new(Ok(&long), Duration::ZERO);
        let got = summarize(MSG, Lang::En, Some(&fake), Duration::from_secs(3)).await.unwrap();
        assert!(got.chars().count() <= SUMMARY_MAX);
    }

    #[tokio::test]
    async fn huge_message_is_cut_before_sending() {
        let fake = Fake::new(Ok("ok"), Duration::ZERO);
        let huge = "word ".repeat(10_000);
        summarize(&huge, Lang::En, Some(&fake), Duration::from_secs(3)).await;
        let (_, user) = fake.seen.lock().unwrap().clone().unwrap();
        assert!(user.chars().count() <= SEND_MAX);
    }

    #[tokio::test(start_paused = true)]
    async fn model_timeout_falls_back_to_first_sentence() {
        let fake = Fake::new(Ok("late"), Duration::from_secs(5));
        let got = summarize(MSG, Lang::En, Some(&fake), Duration::from_secs(3)).await;
        assert_eq!(got.unwrap(), "I've fixed the rounding in billing.ts.");
    }

    #[tokio::test]
    async fn model_error_falls_back() {
        let fake = Fake::new(Err(ProviderError::Network), Duration::ZERO);
        let got = summarize(MSG, Lang::En, Some(&fake), Duration::from_secs(3)).await;
        assert_eq!(got.unwrap(), "I've fixed the rounding in billing.ts.");
        let fake = Fake::new(Ok("  "), Duration::ZERO);
        let got = summarize(MSG, Lang::En, Some(&fake), Duration::from_secs(3)).await;
        assert_eq!(got.unwrap(), "I've fixed the rounding in billing.ts.");
    }

    #[tokio::test]
    async fn no_model_falls_back() {
        assert_eq!(summarize(MSG, Lang::En, None, Duration::from_secs(3)).await.unwrap(), "I've fixed the rounding in billing.ts.");
        // Nothing to summarize → no line, and the model is not asked.
        let fake = Fake::new(Ok("invented"), Duration::ZERO);
        assert_eq!(summarize("   ", Lang::En, Some(&fake), Duration::from_secs(3)).await, None);
        assert!(fake.seen.lock().unwrap().is_none());
    }
}
