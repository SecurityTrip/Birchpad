//! Word completion, as Notepad++'s: the words of the document that start with what is typed.

use std::collections::BTreeMap;

use ropey::Rope;

/// Characters of a word for completion: letters, digits and `_`.
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The start of the word that ends at `pos` (the word being typed).
pub fn word_start(text: &Rope, pos: usize) -> usize {
    let mut start = pos.min(text.len());
    let head = text.slice(..start).to_string();
    for ch in head.chars().rev() {
        if !is_word_char(ch) {
            break;
        }
        start -= ch.len_utf8();
    }
    start
}

/// The words of `text` that start with `prefix` (ignoring case), longer than it, at most
/// `limit` of them in alphabetical order (ignoring case). The word being typed, `exclude`, is
/// left out unless it appears elsewhere too.
pub fn word_candidates(
    text: &Rope,
    prefix: &str,
    exclude: std::ops::Range<usize>,
    limit: usize,
) -> Vec<String> {
    if prefix.is_empty() || limit == 0 {
        return Vec::new();
    }
    let lower_prefix = prefix.to_lowercase();
    let mut found: BTreeMap<(String, String), ()> = BTreeMap::new();
    let mut word = String::new();
    let mut start = 0;
    let mut pos = 0;
    let mut finish = |word: &mut String, start: usize, end: usize| {
        if word.len() > prefix.len()
            && !(start == exclude.start && end == exclude.end)
            && word.to_lowercase().starts_with(&lower_prefix)
            && !word.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        {
            found.insert((word.to_lowercase(), word.clone()), ());
        }
        word.clear();
    };
    for chunk in text.chunks() {
        for ch in chunk.chars() {
            if is_word_char(ch) {
                if word.is_empty() {
                    start = pos;
                }
                word.push(ch);
            } else if !word.is_empty() {
                finish(&mut word, start, pos);
            }
            pos += ch.len_utf8();
        }
    }
    if !word.is_empty() {
        finish(&mut word, start, pos);
    }
    found
        .into_keys()
        .map(|(_, word)| word)
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_that_start_with_the_prefix() {
        let text = Rope::from_str("print printf Printer prin pr_x 2print прибор при");
        let all = word_candidates(&text, "pri", 21..25, 10);
        assert_eq!(all, ["print", "Printer", "printf"]);
        assert_eq!(word_candidates(&text, "при", 0..0, 10), ["прибор"]);
        // The limit, an empty prefix, and nothing found.
        assert_eq!(word_candidates(&text, "pri", 21..25, 1), ["print"]);
        assert!(word_candidates(&text, "", 0..0, 10).is_empty());
        assert!(word_candidates(&text, "zz", 0..0, 10).is_empty());
        assert!(word_candidates(&text, "pri", 0..0, 0).is_empty());
    }

    #[test]
    fn the_word_typed_is_left_out_unless_it_is_elsewhere() {
        let text = Rope::from_str("prints prin");
        assert_eq!(word_candidates(&text, "prin", 7..11, 10), ["prints"]);
        let text = Rope::from_str("prints");
        assert!(word_candidates(&text, "p", 0..6, 10).is_empty());
        let text = Rope::from_str("prints prints");
        assert_eq!(word_candidates(&text, "p", 0..6, 10), ["prints"]);
    }

    #[test]
    fn word_starts() {
        let text = Rope::from_str("a.foo_bar1 д");
        assert_eq!(word_start(&text, 10), 2);
        assert_eq!(word_start(&text, 2), 2);
        assert_eq!(word_start(&text, 0), 0);
        assert_eq!(word_start(&text, text.len()), 11);
        assert_eq!(word_start(&text, 999), 11);
    }
}
