//! Filler sounds taken out of text that had no cleanup (spec 2026-11-10 §5.4): only whole
//! words from a short list; words that can carry meaning («ну», «типа», «а») stay.

/// Removes standalone filler sounds («э-э», «эм», «мм», «um», …).
pub fn strip_fillers(text: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut cap_next = false;
    for word in text.split_whitespace() {
        let core = word.trim_matches(|c: char| !c.is_alphanumeric());
        let tail = &word[word.trim_end_matches(|c: char| !c.is_alphanumeric()).len()..];
        if !is_filler(core) {
            let mut w = word.to_string();
            if cap_next {
                w = capitalise(&w);
                cap_next = false;
            }
            kept.push(w);
            continue;
        }
        let at_sentence_start = kept.last().is_none_or(|w| ends_sentence(w));
        if at_sentence_start {
            cap_next = true;
        }
        // The end of a sentence stays with the word before the filler.
        if let Some(end) = tail.chars().find(|c| matches!(c, '.' | '!' | '?' | '…')) {
            if let Some(prev) = kept.last_mut().filter(|w| !ends_sentence(w)) {
                let trimmed = prev.trim_end_matches([',', ';', ':']).to_string();
                *prev = format!("{trimmed}{end}");
                cap_next = true;
            }
        }
    }
    kept.join(" ")
}

fn is_filler(word: &str) -> bool {
    let w = word.to_lowercase();
    let only = |w: &str, set: &[char]| !w.is_empty() && w.chars().all(|c| set.contains(&c));
    // «э», «ээ», «э-э», «э-э-э»…
    (w.starts_with('э') && only(&w, &['э', '-']))
        || matches!(w.as_str(), "эм" | "эмм" | "хм" | "um" | "uh" | "erm")
        // «мм», «ммм»
        || (w.chars().count() >= 2 && only(&w, &['м']))
}

fn ends_sentence(word: &str) -> bool {
    word.ends_with(['.', '!', '?', '…'])
}

fn capitalise(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_filler_goes_and_the_next_word_is_capitalised() {
        assert_eq!(strip_fillers("Э-э, давай завтра."), "Давай завтра.");
    }

    #[test]
    fn fillers_inside_go_with_their_comma() {
        assert_eq!(strip_fillers("Я думаю, эм, что надо мм позвонить."), "Я думаю, что надо позвонить.");
    }

    #[test]
    fn meaningful_words_stay() {
        assert_eq!(strip_fillers("Ну, давай."), "Ну, давай.");
        assert_eq!(strip_fillers("А потом типа ушёл."), "А потом типа ушёл.");
    }

    #[test]
    fn parts_of_words_stay() {
        assert_eq!(strip_fillers("Эмоции и мама."), "Эмоции и мама.");
    }

    #[test]
    fn english_fillers() {
        assert_eq!(strip_fillers("Um, let's call, uh, tomorrow."), "Let's call, tomorrow.");
    }

    #[test]
    fn only_fillers_is_empty() {
        assert_eq!(strip_fillers("э-э-э"), "");
        assert_eq!(strip_fillers("Ммм. Хм."), "");
    }

    #[test]
    fn sentence_end_moves_to_the_previous_word() {
        assert_eq!(strip_fillers("Позвоню завтра э-э. Потом решим."), "Позвоню завтра. Потом решим.");
    }
}
