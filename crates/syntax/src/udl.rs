//! Notepad++'s User Defined Languages (UDL 2.x): languages described by lists of keywords,
//! comments, operators and delimiters, with their own colors, kept in XML files. Birchpad reads
//! them as they are and colors text with a small lexer, as Notepad++'s UDL lexer does.

use std::ops::Range;

mod definition;

pub use definition::{DELIMITERS, LISTS, UdlDefinition, udl_file};

/// What a piece of text is, in a user defined language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UdlKind {
    Comment,
    LineComment,
    Number,
    /// Keywords1 to Keywords8, as 0 to 7.
    Keyword(u8),
    Operator,
    /// Folders in code 1 (symbols) and 2 (words), and in comments.
    FolderCode1,
    FolderCode2,
    /// Delimiters1 to Delimiters8, as 0 to 7.
    Delimiter(u8),
}

/// A style of a user defined language: `RRGGBB` colors and Notepad++'s `fontStyle` bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UdlStyle {
    pub color: Option<u32>,
    pub background: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Where a line comment may start: Notepad++'s `forcePureLC`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineCommentAt {
    #[default]
    Anywhere,
    /// After white space only.
    LineStartOrIndent,
    /// In the first column only.
    LineStart,
}

/// How numbers are written: Notepad++'s "Numbers" lists. With none of them set, numbers take
/// the usual forms (`12`, `1.5`, `0xFF`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Numbers {
    prefix1: Vec<String>,
    prefix2: Vec<String>,
    /// The characters a number may have after `prefix1` (`a b c d e f` after `0x`).
    extras1: Vec<String>,
    extras2: Vec<String>,
    suffix1: Vec<String>,
    suffix2: Vec<String>,
    /// What joins two numbers into a range (`..`).
    range: Vec<String>,
}

impl Numbers {
    fn is_set(&self) -> bool {
        [
            &self.prefix1,
            &self.prefix2,
            &self.extras1,
            &self.extras2,
            &self.suffix1,
            &self.suffix2,
            &self.range,
        ]
        .iter()
        .any(|list| !list.is_empty())
    }
}

/// Notepad++'s `nesting` bits: what a delimiter may hold.
pub mod nesting {
    /// Delimiters 1 to 8 are bits 0 to 7.
    pub const COMMENT: u32 = 0x100;
    pub const LINE_COMMENT: u32 = 0x200;
    /// Keywords 1 to 8 are this bit and the seven above it.
    pub const KEYWORDS1: u32 = 0x400;
    pub const OPERATORS1: u32 = 0x4_0000;
    pub const OPERATORS2: u32 = 0x8_0000;
    pub const NUMBERS: u32 = 0x10_0000;
}

/// How deep delimiters nest before the inner ones are left as text.
const MAX_NESTING: usize = 8;

/// Colored pieces of text, in order.
type Pieces = Vec<(Range<usize>, UdlKind)>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Delimiter {
    open: String,
    escape: String,
    /// Empty: the delimiter ends with its line.
    close: String,
}

/// A user defined language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserLanguage {
    pub name: String,
    /// File extensions without the dot, lowercase.
    pub extensions: Vec<String>,
    ignore_case: bool,
    line_comments: Vec<String>,
    block_comments: Vec<(String, String)>,
    keywords: [Vec<String>; 8],
    /// Whether Keywords*n* match words that start with them.
    prefix: [bool; 8],
    operators: Vec<String>,
    operators2: Vec<String>,
    folders1: Vec<String>,
    folders2: Vec<String>,
    /// Symbols and words that open and close a fold, in code (both lists) and in comments.
    fold_open: Vec<String>,
    fold_close: Vec<String>,
    comment_fold_open: Vec<String>,
    comment_fold_close: Vec<String>,
    delimiters: Vec<(u8, Delimiter)>,
    /// What each delimiter may hold: [`nesting`] bits, by delimiter number.
    nesting: [u32; 8],
    numbers: Numbers,
    line_comment_at: LineCommentAt,
    /// Text without a style, and each kind's style.
    pub default_style: UdlStyle,
    styles: Vec<(UdlKind, UdlStyle)>,
}

/// Why a UDL file did not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", birchpad_i18n::t!("syntax.not-a-udl", reason = .0))]
pub struct UdlError(String);

/// Splits a list into its words; `"two words"` in quotes is one.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(end) = quoted.find('"')
            && end > 0
        {
            out.push(quoted[..end].to_owned());
            rest = quoted[end + 1..].trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        out.push(rest[..end].to_owned());
        rest = rest[end..].trim_start();
    }
    out
}

/// Splits a coded list (`00// 01 03/* 04*/`) into (code, value) pairs; a code alone has an
/// empty value.
fn coded(text: &str) -> Vec<(u8, String)> {
    text.split_whitespace()
        .filter_map(|token| {
            let code: u8 = token.get(..2)?.parse().ok()?;
            Some((code, token[2..].to_owned()))
        })
        .collect()
}

fn hex_color(value: Option<&str>) -> Option<u32> {
    let value = value?.trim();
    (value.len() == 6)
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}

fn style_of(node: roxmltree::Node) -> UdlStyle {
    let font_style: u32 = node
        .attribute("fontStyle")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    // `colorStyle`: 1 the foreground is set, 2 the background; unset means both.
    let color_style: u32 = node
        .attribute("colorStyle")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(3);
    UdlStyle {
        color: hex_color(node.attribute("fgColor")).filter(|_| color_style & 1 != 0),
        background: hex_color(node.attribute("bgColor")).filter(|_| color_style & 2 != 0),
        bold: font_style & 1 != 0,
        italic: font_style & 2 != 0,
        underline: font_style & 4 != 0,
    }
}

/// Reads the user defined languages of a Notepad++ UDL file (one or several `<UserLang>`).
pub fn parse_udl(xml: &str) -> Result<Vec<UserLanguage>, UdlError> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let document = roxmltree::Document::parse(xml).map_err(|error| UdlError(error.to_string()))?;
    let languages: Vec<UserLanguage> = document
        .descendants()
        .filter(|node| node.has_tag_name("UserLang"))
        .filter_map(language_of)
        .collect();
    if languages.is_empty() {
        return Err(UdlError("no <UserLang> with a name".into()));
    }
    Ok(languages)
}

/// Adds a "Folders in code" list to the words that open or close a fold; "middle" ones
/// (`else`) neither open nor close.
fn fold_list(language: &mut UserLanguage, list: roxmltree::Node, text: &str) {
    let name = list.attribute("name").unwrap_or_default();
    if name.ends_with("open") {
        language.fold_open.extend(words(text));
    } else if name.ends_with("close") {
        language.fold_close.extend(words(text));
    }
}

fn language_of(node: roxmltree::Node) -> Option<UserLanguage> {
    let name = node.attribute("name")?.trim();
    if name.is_empty() {
        return None;
    }
    let mut language = UserLanguage {
        name: name.to_owned(),
        extensions: words(node.attribute("ext").unwrap_or_default())
            .into_iter()
            .map(|ext| ext.trim_start_matches('.').to_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect(),
        ..UserLanguage::default()
    };
    for setting in node.descendants() {
        if setting.has_tag_name("Global") {
            language.ignore_case = setting.attribute("caseIgnored") == Some("yes");
            language.line_comment_at = match setting.attribute("forcePureLC") {
                Some("1") => LineCommentAt::LineStartOrIndent,
                Some("2") => LineCommentAt::LineStart,
                _ => LineCommentAt::Anywhere,
            };
        } else if setting.has_tag_name("Prefix") {
            for (index, prefix) in language.prefix.iter_mut().enumerate() {
                *prefix =
                    setting.attribute(format!("Keywords{}", index + 1).as_str()) == Some("yes");
            }
        }
    }
    for list in node
        .descendants()
        .filter(|list| list.has_tag_name("Keywords"))
    {
        let text = list.text().unwrap_or_default();
        match list.attribute("name").unwrap_or_default() {
            "Comments" => {
                let codes = coded(text);
                let of = |code: u8| {
                    codes
                        .iter()
                        .filter(move |(c, _)| *c == code)
                        .map(|(_, v)| v.clone())
                };
                language.line_comments = of(0).filter(|value| !value.is_empty()).collect();
                language.block_comments = of(3)
                    .zip(of(4))
                    .filter(|(open, close)| !open.is_empty() && !close.is_empty())
                    .collect();
            }
            "Delimiters" => {
                let codes = coded(text);
                for index in 0..8u8 {
                    let value = |part: u8| {
                        codes
                            .iter()
                            .find(|(code, _)| *code == index * 3 + part)
                            .map(|(_, value)| value.clone())
                            .unwrap_or_default()
                    };
                    let open = value(0);
                    if !open.is_empty() {
                        language.delimiters.push((
                            index,
                            Delimiter {
                                open,
                                escape: value(1),
                                close: value(2),
                            },
                        ));
                    }
                }
            }
            "Operators1" => language.operators = words(text),
            "Operators2" => language.operators2 = words(text),
            "Folders in code1, open" | "Folders in code1, middle" | "Folders in code1, close" => {
                language.folders1.extend(words(text));
                fold_list(&mut language, list, text);
            }
            "Folders in code2, open" | "Folders in code2, middle" | "Folders in code2, close" => {
                language.folders2.extend(words(text));
                fold_list(&mut language, list, text);
            }
            "Folders in comment, open" => language.comment_fold_open = words(text),
            "Folders in comment, close" => language.comment_fold_close = words(text),
            "Numbers, prefix1" => language.numbers.prefix1 = words(text),
            "Numbers, prefix2" => language.numbers.prefix2 = words(text),
            "Numbers, extras1" => language.numbers.extras1 = words(text),
            "Numbers, extras2" => language.numbers.extras2 = words(text),
            "Numbers, suffix1" => language.numbers.suffix1 = words(text),
            "Numbers, suffix2" => language.numbers.suffix2 = words(text),
            "Numbers, range" => language.numbers.range = words(text),
            name => {
                if let Some(number) = name.strip_prefix("Keywords")
                    && let Ok(number @ 1..=8) = number.parse::<usize>()
                {
                    language.keywords[number - 1] = words(text);
                }
            }
        }
    }
    // Longest first, so that `<=` wins over `<`.
    for list in [
        &mut language.operators,
        &mut language.folders1,
        &mut language.numbers.prefix1,
        &mut language.numbers.prefix2,
        &mut language.numbers.suffix1,
        &mut language.numbers.suffix2,
    ] {
        list.sort_by_key(|operator| std::cmp::Reverse(operator.len()));
    }
    for style in node
        .descendants()
        .filter(|style| style.has_tag_name("WordsStyle"))
    {
        let kind = match style.attribute("name").unwrap_or_default() {
            "DEFAULT" => {
                language.default_style = style_of(style);
                continue;
            }
            "COMMENTS" => UdlKind::Comment,
            "LINE COMMENTS" => UdlKind::LineComment,
            "NUMBERS" => UdlKind::Number,
            "OPERATORS" => UdlKind::Operator,
            "FOLDER IN CODE1" => UdlKind::FolderCode1,
            "FOLDER IN CODE2" => UdlKind::FolderCode2,
            name => {
                let numbered = |prefix: &str| {
                    name.strip_prefix(prefix)
                        .and_then(|number| number.parse::<u8>().ok())
                        .filter(|number| (1..=8).contains(number))
                        .map(|number| number - 1)
                };
                if let Some(number) = numbered("KEYWORDS") {
                    UdlKind::Keyword(number)
                } else if let Some(number) = numbered("DELIMITERS") {
                    language.nesting[usize::from(number)] = style
                        .attribute("nesting")
                        .and_then(|value| value.trim().parse().ok())
                        .unwrap_or(0);
                    UdlKind::Delimiter(number)
                } else {
                    continue;
                }
            }
        };
        language.styles.push((kind, style_of(style)));
    }
    Some(language)
}

impl UserLanguage {
    /// The style of `kind`, if the language gives it one.
    pub fn style(&self, kind: UdlKind) -> Option<UdlStyle> {
        self.styles
            .iter()
            .find(|(known, _)| *known == kind)
            .map(|(_, style)| *style)
    }

    /// Whether a file named `name` is in this language, by its extension.
    pub fn matches_file(&self, name: &str) -> bool {
        name.rsplit_once('.').is_some_and(|(_, ext)| {
            self.extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
    }

    fn same(&self, a: &str, b: &str) -> bool {
        if self.ignore_case {
            a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    }

    fn starts_with(&self, text: &str, prefix: &str) -> bool {
        text.get(..prefix.len())
            .is_some_and(|start| self.same(start, prefix))
    }

    /// Whether a symbol (operator, comment, delimiter) starts at `rest`.
    fn symbol_at(&self, rest: &str) -> bool {
        self.operators
            .iter()
            .any(|operator| rest.starts_with(operator.as_str()))
            || self
                .folders1
                .iter()
                .any(|folder| rest.starts_with(folder.as_str()))
            || self
                .line_comments
                .iter()
                .any(|open| self.starts_with(rest, open))
            || self
                .block_comments
                .iter()
                .any(|(open, _)| self.starts_with(rest, open))
            || self
                .delimiters
                .iter()
                .any(|(_, delimiter)| self.starts_with(rest, &delimiter.open))
    }

    /// Whether `word` is a number: as the "Numbers" lists say, or in the usual forms.
    fn is_number(&self, word: &str) -> bool {
        let numbers = &self.numbers;
        if !numbers.is_set() {
            return word.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                && word
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, '.' | 'x' | 'X' | '_'));
        }
        let mut parts = vec![word];
        for separator in numbers
            .range
            .iter()
            .filter(|separator| !separator.is_empty())
        {
            parts = parts
                .into_iter()
                .flat_map(|part| part.split(separator.as_str()))
                .collect();
        }
        parts.iter().all(|part| self.is_single_number(part))
    }

    fn is_single_number(&self, word: &str) -> bool {
        let numbers = &self.numbers;
        let mut body = word;
        if let Some(suffix) = numbers
            .suffix1
            .iter()
            .chain(&numbers.suffix2)
            .find(|suffix| body.len() > suffix.len() && self.ends_with(body, suffix))
        {
            body = &body[..body.len() - suffix.len()];
        }
        let prefixed = |prefixes: &[String]| {
            prefixes
                .iter()
                .find(|prefix| !prefix.is_empty() && self.starts_with(body, prefix))
                .map(|prefix| &body[prefix.len()..])
        };
        let (digits, extras) = if let Some(rest) = prefixed(&numbers.prefix1) {
            (rest, numbers.extras1.as_slice())
        } else if let Some(rest) = prefixed(&numbers.prefix2) {
            (rest, numbers.extras2.as_slice())
        } else {
            if !body.starts_with(|ch: char| ch.is_ascii_digit()) {
                return false;
            }
            (body, &[][..])
        };
        let extra = |ch: char| {
            extras.iter().any(|extra| {
                let mut chars = extra.chars();
                chars
                    .next()
                    .is_some_and(|first| self.same_char(first, ch) && chars.next().is_none())
            })
        };
        !digits.is_empty()
            && digits.matches('.').count() <= 1
            && digits
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '.' || extra(ch))
    }

    fn same_char(&self, a: char, b: char) -> bool {
        a == b || (self.ignore_case && a.to_lowercase().eq(b.to_lowercase()))
    }

    fn ends_with(&self, text: &str, suffix: &str) -> bool {
        text.len() >= suffix.len()
            && text.is_char_boundary(text.len() - suffix.len())
            && self.same(&text[text.len() - suffix.len()..], suffix)
    }

    fn word_kind(&self, word: &str) -> Option<UdlKind> {
        if self.is_number(word) {
            return Some(UdlKind::Number);
        }
        for (index, list) in self.keywords.iter().enumerate() {
            let found = list.iter().any(|keyword| {
                if self.prefix[index] {
                    self.starts_with(word, keyword)
                } else {
                    self.same(word, keyword)
                }
            });
            if found {
                return Some(UdlKind::Keyword(index as u8));
            }
        }
        if self.folders2.iter().any(|folder| self.same(word, folder)) {
            return Some(UdlKind::FolderCode2);
        }
        if self
            .operators2
            .iter()
            .any(|operator| self.same(word, operator))
        {
            return Some(UdlKind::Operator);
        }
        None
    }

    /// Whether a line comment may start at `pos` of `text`.
    fn line_comment_allowed(&self, text: &str, pos: usize) -> bool {
        let line_start = text[..pos].rfind(['\n', '\r']).map_or(0, |at| at + 1);
        match self.line_comment_at {
            LineCommentAt::Anywhere => true,
            LineCommentAt::LineStartOrIndent => text[line_start..pos].trim().is_empty(),
            LineCommentAt::LineStart => line_start == pos,
        }
    }

    /// The end of the word that starts at `pos`: up to white space or a symbol.
    fn word_end(&self, text: &str, pos: usize) -> usize {
        let mut end = pos + text[pos..].chars().next().map_or(0, char::len_utf8);
        while end < text.len() {
            let next = text[end..].chars().next().expect("not at the end");
            if next.is_whitespace() || self.symbol_at(&text[end..]) {
                break;
            }
            end += next.len_utf8();
        }
        end
    }

    /// The end of a block comment that starts at `pos` with `open`.
    fn block_comment_end(text: &str, pos: usize, open: &str, close: &str) -> usize {
        let after = pos + open.len();
        text[after..]
            .find(close)
            .map_or(text.len(), |end| after + end + close.len())
    }

    fn line_end(text: &str, at: usize) -> usize {
        text[at..]
            .find(['\n', '\r'])
            .map_or(text.len(), |end| at + end)
    }

    /// The delimiter `index` that starts at `pos`: where it ends, and its pieces: the
    /// delimiter's own, with what its `nesting` lets it hold between them.
    fn delimiter_at(
        &self,
        text: &str,
        pos: usize,
        index: u8,
        delimiter: &Delimiter,
        depth: usize,
    ) -> (usize, Pieces) {
        let kind = UdlKind::Delimiter(index);
        let mask = self.nesting[usize::from(index)];
        let mut pieces = Vec::new();
        let mut run = pos;
        let mut at = pos + delimiter.open.len();
        // Whether `at` follows white space or a symbol, where a nested word may start.
        let mut word_start = true;
        let end = loop {
            if at >= text.len() {
                break text.len();
            }
            let tail = &text[at..];
            if !delimiter.escape.is_empty() && tail.starts_with(delimiter.escape.as_str()) {
                at += delimiter.escape.len();
                at += text[at..].chars().next().map_or(0, char::len_utf8);
                word_start = false;
                continue;
            }
            if delimiter.close.is_empty() {
                if tail.starts_with(['\n', '\r']) {
                    break at;
                }
            } else if self.starts_with(tail, &delimiter.close) {
                break at + delimiter.close.len();
            }
            if mask != 0
                && depth < MAX_NESTING
                && let Some((nested_end, nested)) =
                    self.nested_at(text, at, mask, word_start, depth)
            {
                if run < at {
                    pieces.push((run..at, kind));
                }
                pieces.extend(nested);
                at = nested_end;
                run = at;
                word_start = true;
                continue;
            }
            let ch = tail.chars().next().expect("not at the end");
            word_start = ch.is_whitespace() || self.symbol_at(tail);
            at += ch.len_utf8();
        };
        if run < end {
            pieces.push((run..end, kind));
        }
        (end, pieces)
    }

    /// What a delimiter holds at `at`, if its `mask` lets it: a comment, another delimiter,
    /// an operator, or (at the start of a word) a keyword or a number.
    fn nested_at(
        &self,
        text: &str,
        at: usize,
        mask: u32,
        word_start: bool,
        depth: usize,
    ) -> Option<(usize, Pieces)> {
        let rest = &text[at..];
        if mask & nesting::COMMENT != 0
            && let Some((open, close)) = self
                .block_comments
                .iter()
                .find(|(open, _)| self.starts_with(rest, open))
        {
            let end = Self::block_comment_end(text, at, open, close);
            return Some((end, vec![(at..end, UdlKind::Comment)]));
        }
        if mask & nesting::LINE_COMMENT != 0
            && self
                .line_comments
                .iter()
                .any(|open| self.starts_with(rest, open))
        {
            let end = Self::line_end(text, at);
            return Some((end, vec![(at..end, UdlKind::LineComment)]));
        }
        if let Some((index, delimiter)) = self.delimiters.iter().find(|(index, delimiter)| {
            mask & (1 << index) != 0 && self.starts_with(rest, &delimiter.open)
        }) {
            return Some(self.delimiter_at(text, at, *index, delimiter, depth + 1));
        }
        if mask & nesting::OPERATORS1 != 0
            && let Some(operator) = self
                .operators
                .iter()
                .find(|operator| rest.starts_with(operator.as_str()))
        {
            let end = at + operator.len();
            return Some((end, vec![(at..end, UdlKind::Operator)]));
        }
        let ch = rest.chars().next()?;
        if !word_start || ch.is_whitespace() || self.symbol_at(rest) {
            return None;
        }
        let end = self.word_end(text, at);
        let kind = self.word_kind(&text[at..end])?;
        let allowed = match kind {
            UdlKind::Number => mask & nesting::NUMBERS != 0,
            UdlKind::Keyword(index) => mask & (nesting::KEYWORDS1 << index) != 0,
            UdlKind::Operator => mask & nesting::OPERATORS2 != 0,
            _ => false,
        };
        allowed.then(|| (end, vec![(at..end, kind)]))
    }

    /// The colored pieces of `text` that end after `from`, in order: comments, delimiters,
    /// operators, numbers and keywords. Lexing starts at the beginning of `text`, since a block
    /// comment or a delimiter may come from far above.
    pub fn highlight(&self, text: &str, from: usize) -> Vec<(Range<usize>, UdlKind)> {
        let mut out = Vec::new();
        let mut pos = 0;
        let mut push = |range: Range<usize>, kind: UdlKind| {
            if range.end > from && !range.is_empty() {
                out.push((range, kind));
            }
        };
        while pos < text.len() {
            let rest = &text[pos..];
            if let Some((open, close)) = self
                .block_comments
                .iter()
                .find(|(open, _)| self.starts_with(rest, open))
            {
                let end = Self::block_comment_end(text, pos, open, close);
                push(pos..end, UdlKind::Comment);
                pos = end;
                continue;
            }
            if self
                .line_comments
                .iter()
                .any(|open| self.starts_with(rest, open))
                && self.line_comment_allowed(text, pos)
            {
                let end = Self::line_end(text, pos);
                push(pos..end, UdlKind::LineComment);
                pos = end;
                continue;
            }
            if let Some((index, delimiter)) = self
                .delimiters
                .iter()
                .find(|(_, delimiter)| self.starts_with(rest, &delimiter.open))
            {
                let (end, pieces) = self.delimiter_at(text, pos, *index, delimiter, 0);
                for (range, kind) in pieces {
                    push(range, kind);
                }
                pos = end;
                continue;
            }
            if let Some(operator) = self
                .operators
                .iter()
                .find(|operator| rest.starts_with(operator.as_str()))
            {
                push(pos..pos + operator.len(), UdlKind::Operator);
                pos += operator.len();
                continue;
            }
            if let Some(folder) = self
                .folders1
                .iter()
                .find(|folder| rest.starts_with(folder.as_str()))
            {
                push(pos..pos + folder.len(), UdlKind::FolderCode1);
                pos += folder.len();
                continue;
            }
            let ch = rest.chars().next().expect("not at the end");
            if ch.is_whitespace() {
                pos += ch.len_utf8();
                continue;
            }
            let end = self.word_end(text, pos);
            if let Some(kind) = self.word_kind(&text[pos..end]) {
                push(pos..end, kind);
            }
            pos = end;
        }
        out
    }

    /// The folds of `text`, as byte ranges from the word or symbol that opens each to the one
    /// that closes it, sorted by start: "Folders in code" outside comments and delimiters,
    /// "Folders in comment" inside comments. A fold that closes on the line it opens is none;
    /// one never closed runs to the end of the text.
    pub fn folds(&self, text: &str) -> Vec<Range<usize>> {
        let mut folds = Vec::new();
        let mut code: Vec<usize> = Vec::new();
        let mut comment: Vec<usize> = Vec::new();
        let same_line = |a: usize, b: usize| !text[a..b].contains(['\n', '\r']);
        let close = |stack: &mut Vec<usize>, end: usize, folds: &mut Vec<Range<usize>>| {
            if let Some(start) = stack.pop()
                && !same_line(start, end)
            {
                folds.push(start..end);
            }
        };
        for (range, kind) in self.highlight(text, 0) {
            let piece = &text[range.clone()];
            match kind {
                UdlKind::FolderCode1 | UdlKind::FolderCode2 => {
                    if self.fold_open.iter().any(|open| self.same(piece, open)) {
                        code.push(range.start);
                    } else if self.fold_close.iter().any(|shut| self.same(piece, shut)) {
                        close(&mut code, range.end, &mut folds);
                    }
                }
                UdlKind::Comment | UdlKind::LineComment => {
                    for (offset, word) in comment_words(piece) {
                        let at = range.start + offset;
                        if self
                            .comment_fold_open
                            .iter()
                            .any(|open| self.same(word, open))
                        {
                            comment.push(at);
                        } else if self
                            .comment_fold_close
                            .iter()
                            .any(|shut| self.same(word, shut))
                        {
                            close(&mut comment, at + word.len(), &mut folds);
                        }
                    }
                }
                _ => {}
            }
        }
        for start in code.into_iter().chain(comment) {
            if !same_line(start, text.len()) {
                folds.push(start..text.len());
            }
        }
        folds.sort_by_key(|fold| (fold.start, std::cmp::Reverse(fold.end)));
        // One fold per starting line: the first, widest.
        let mut last_line_start = None;
        folds.retain(|fold| {
            let line_start = text[..fold.start].rfind('\n').map_or(0, |at| at + 1);
            let keep = last_line_start != Some(line_start);
            last_line_start = Some(line_start);
            keep
        });
        folds
    }
}

/// The words of a comment with their offsets, split at white space.
fn comment_words(comment: &str) -> impl Iterator<Item = (usize, &str)> {
    comment
        .split_whitespace()
        .map(move |word| (word.as_ptr() as usize - comment.as_ptr() as usize, word))
}

#[cfg(test)]
#[path = "../tests/unit/udl/tests.rs"]
mod tests;
