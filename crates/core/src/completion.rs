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

/// Which words [`word_candidates_with`] offers: Notepad++'s Auto-Completion options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordOptions {
    /// Words match what is typed without regard to case.
    pub ignore_case: bool,
    /// Numbers (words that start with a digit) are not offered.
    pub ignore_numbers: bool,
}

impl Default for WordOptions {
    fn default() -> Self {
        Self {
            ignore_case: true,
            ignore_numbers: true,
        }
    }
}

/// The words of `text` that start with `prefix` (ignoring case), longer than it, at most
/// `limit` of them in alphabetical order (ignoring case). The word being typed, `exclude`, is
/// left out unless it appears elsewhere too. Numbers are left out.
pub fn word_candidates(
    text: &Rope,
    prefix: &str,
    exclude: std::ops::Range<usize>,
    limit: usize,
) -> Vec<String> {
    word_candidates_with(text, prefix, exclude, limit, WordOptions::default())
}

/// [`word_candidates`] with Notepad++'s options: matching case, offering numbers.
pub fn word_candidates_with(
    text: &Rope,
    prefix: &str,
    exclude: std::ops::Range<usize>,
    limit: usize,
    options: WordOptions,
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
        let matches = if options.ignore_case {
            word.to_lowercase().starts_with(&lower_prefix)
        } else {
            word.starts_with(prefix)
        };
        if word.len() > prefix.len()
            && !(start == exclude.start && end == exclude.end)
            && matches
            && !(options.ignore_numbers
                && word.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
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

/// HTML's elements that have no closing tag.
const VOID_ELEMENTS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// The closing tag to insert after `>` typed at the end of `before` (the text up to and with
/// it): `</name>` for an opening tag `<name ...>`, nothing for a closing tag, a tag that closes
/// itself (`<br/>`), a comment, a declaration or a processing instruction, and in `html` for
/// the elements that have no closing tag.
pub fn closing_tag(before: &str, html: bool) -> Option<String> {
    let tag = before.strip_suffix('>')?;
    let open = tag.rfind('<')?;
    let inside = &tag[open + 1..];
    if inside.ends_with('/') || inside.contains('>') {
        return None;
    }
    let name: String = inside
        .chars()
        .take_while(|&ch| ch.is_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
        .collect();
    let first = name.chars().next()?;
    if !first.is_alphabetic() && first != '_' {
        return None;
    }
    // What follows the name starts the attributes.
    let rest = &inside[name.len()..];
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    // A `>` inside a quoted attribute value is not the tag's end.
    if rest.matches('"').count() % 2 == 1 || rest.matches('\'').count() % 2 == 1 {
        return None;
    }
    if html && VOID_ELEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
        return None;
    }
    Some(format!("</{name}>"))
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
    fn case_and_numbers_as_the_options_say() {
        let text = Rope::from_str("Print print 2print 20 200");
        let exact = WordOptions {
            ignore_case: false,
            ignore_numbers: true,
        };
        assert_eq!(
            word_candidates_with(&text, "pr", 0..0, 10, exact),
            ["print"]
        );
        assert_eq!(
            word_candidates_with(&text, "Pr", 0..0, 10, exact),
            ["Print"],
            "case matters"
        );
        let numbers = WordOptions {
            ignore_case: true,
            ignore_numbers: false,
        };
        assert_eq!(
            word_candidates_with(&text, "2", 0..0, 10, numbers),
            ["20", "200", "2print"]
        );
        assert!(
            word_candidates(&text, "2", 0..0, 10).is_empty(),
            "numbers ignored by default"
        );
        assert!(word_candidates_with(&text, "", 0..0, 10, numbers).is_empty());
    }

    #[test]
    fn opening_tags_are_closed() {
        assert_eq!(closing_tag("<div>", true).as_deref(), Some("</div>"));
        assert_eq!(
            closing_tag("text <a href=\"x\" class='y'>", true).as_deref(),
            Some("</a>")
        );
        assert_eq!(
            closing_tag("<ns:item>", false).as_deref(),
            Some("</ns:item>")
        );
        assert_eq!(
            closing_tag("<x-y_z.1>", false).as_deref(),
            Some("</x-y_z.1>")
        );
        assert_eq!(
            closing_tag("<Br>", false).as_deref(),
            Some("</Br>"),
            "XML has no void elements"
        );
    }

    #[test]
    fn other_tags_are_not_closed() {
        for typed in [
            "</div>",
            "<br/>",
            "<br />",
            "<!-- c -->",
            "<!DOCTYPE html>",
            "<?xml version=\"1.0\"?>",
            "a > b",
            ">",
            "<>",
            "< div>",
            "<1a>",
            "<a title=\"x>",
            "<a title='x>",
            "<div><>",
        ] {
            assert_eq!(closing_tag(typed, true), None, "{typed}");
        }
        assert_eq!(closing_tag("<br>", true), None, "a void element");
        assert_eq!(closing_tag("<IMG src=x>", true), None);
        assert_eq!(closing_tag("<div", true), None, "no > typed");
        assert_eq!(closing_tag("", true), None);
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
