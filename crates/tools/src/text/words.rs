//! Word Counter. Words follow Unicode rules, so languages without spaces are
//! counted too, and emoji count as one character however many code points
//! they take.

use saqviro_jobs::Params;
use unicode_segmentation::UnicodeSegmentation;

/// A typical silent reading speed.
const WORDS_PER_MINUTE: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub words: usize,
    pub characters: usize,
    pub without_spaces: usize,
    pub lines: usize,
    pub sentences: usize,
    pub paragraphs: usize,
}

pub fn count(text: &str) -> Counts {
    let graphemes = || text.graphemes(true);
    Counts {
        words: text.unicode_words().count(),
        characters: graphemes().count(),
        without_spaces: graphemes().filter(|g| !g.chars().all(char::is_whitespace)).count(),
        lines: text.lines().count(),
        sentences: text
            .unicode_sentences()
            .filter(|s| s.chars().any(char::is_alphanumeric))
            .count(),
        paragraphs: text
            .replace("\r\n", "\n")
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count(),
    }
}

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    let c = count(text);
    let minutes = c.words.div_ceil(WORDS_PER_MINUTE).max(1);
    Ok(format!(
        "Words: {}\nCharacters: {}\nCharacters without spaces: {}\nLines: {}\nSentences: {}\nParagraphs: {}\nReading time: about {minutes} min",
        c.words, c.characters, c.without_spaces, c.lines, c.sentences, c.paragraphs
    ))
}
