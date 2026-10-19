//! What a session file writes for a document: its line ending, its language and the name of its
//! backup copy, as Notepad++'s session does.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use birchpad_core::LineEnding;

/// How a session names a line ending.
pub fn line_ending_name(line_ending: LineEnding) -> &'static str {
    match line_ending {
        LineEnding::CrLf => "crlf",
        LineEnding::Lf => "lf",
        LineEnding::Cr => "cr",
        LineEnding::Nel => "nel",
    }
}

/// The line ending a session names; `None` for a name it does not know.
pub fn parse_line_ending(name: &str) -> Option<LineEnding> {
    match name {
        "crlf" => Some(LineEnding::CrLf),
        "lf" => Some(LineEnding::Lf),
        "cr" => Some(LineEnding::Cr),
        "nel" => Some(LineEnding::Nel),
        _ => None,
    }
}

/// What starts the name of a user defined language where a language is named (`udl:Markdown`).
pub const USER_LANGUAGE_PREFIX: &str = "udl:";

/// A language as a session (and the Language menu's commands) names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageName<'a> {
    /// Normal text, no highlighting: `text`.
    Text,
    /// A user defined language, by its name: `udl:<name>`.
    User(&'a str),
    /// One of Birchpad's languages: its id, or a Notepad++ language name.
    Builtin(&'a str),
}

impl<'a> LanguageName<'a> {
    pub fn parse(name: &'a str) -> Self {
        if name == "text" {
            Self::Text
        } else if let Some(user) = name.strip_prefix(USER_LANGUAGE_PREFIX) {
            Self::User(user)
        } else {
            Self::Builtin(name)
        }
    }

    /// The name, as written.
    pub fn write(self) -> String {
        match self {
            Self::Text => "text".to_owned(),
            Self::User(name) => format!("{USER_LANGUAGE_PREFIX}{name}"),
            Self::Builtin(id) => id.to_owned(),
        }
    }
}

/// A new name for a backup copy, like Notepad++'s: the document's name (without the characters
/// file names cannot have) and when, unique within the process.
pub fn backup_name(display_name: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name: String = display_name
        .chars()
        .map(|ch| if r#"\/:*?"<>|"#.contains(ch) { '_' } else { ch })
        .collect();
    format!("{name}@{millis}-{count}")
}

/// The number of an untitled document (`new 3`): `wanted` if no other has it yet, else the
/// lowest free one; it is taken in `numbers`.
pub fn free_untitled_number(wanted: Option<usize>, numbers: &mut HashSet<usize>) -> usize {
    let number = wanted
        .filter(|number| *number > 0 && !numbers.contains(number))
        .unwrap_or_else(|| (1..).find(|n| !numbers.contains(n)).expect("a free number"));
    numbers.insert(number);
    number
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_by_name() {
        for line_ending in [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr] {
            assert_eq!(
                parse_line_ending(line_ending_name(line_ending)),
                Some(line_ending)
            );
        }
        for unknown in ["", "CRLF", "lfcr", "unix"] {
            assert_eq!(parse_line_ending(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn languages_by_name() {
        assert_eq!(LanguageName::parse("text"), LanguageName::Text);
        assert_eq!(
            LanguageName::parse("udl:Markdown"),
            LanguageName::User("Markdown")
        );
        assert_eq!(
            LanguageName::parse("udl:"),
            LanguageName::User(""),
            "an empty name"
        );
        assert_eq!(LanguageName::parse("rust"), LanguageName::Builtin("rust"));
        assert_eq!(LanguageName::parse("Text"), LanguageName::Builtin("Text"));
        assert_eq!(LanguageName::parse(""), LanguageName::Builtin(""));
        for name in ["text", "udl:My lang", "python"] {
            assert_eq!(LanguageName::parse(name).write(), name);
        }
    }

    #[test]
    fn backup_names_are_file_names_and_unique() {
        let first = backup_name(r#"a\b/c:d*e?f"g<h>i|j"#);
        assert!(first.starts_with("a_b_c_d_e_f_g_h_i_j@"), "{first}");
        assert_ne!(backup_name("new 1"), backup_name("new 1"));
        assert!(backup_name("").starts_with('@'), "an empty name");
        assert!(backup_name("заметки").starts_with("заметки@"));
    }

    #[test]
    fn untitled_numbers_are_kept_or_the_lowest_free() {
        let mut numbers = HashSet::new();
        assert_eq!(free_untitled_number(Some(3), &mut numbers), 3);
        assert_eq!(free_untitled_number(Some(3), &mut numbers), 1, "taken");
        assert_eq!(free_untitled_number(None, &mut numbers), 2);
        assert_eq!(
            free_untitled_number(Some(0), &mut numbers),
            4,
            "0 is not a number"
        );
        assert_eq!(
            free_untitled_number(Some(usize::MAX), &mut numbers),
            usize::MAX
        );
    }
}
