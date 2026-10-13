//! Notepad++'s User Defined Languages (UDL 2.x): languages described by lists of keywords,
//! comments, operators and delimiters, with their own colors, kept in XML files. Birchpad reads
//! them as they are and colors text with a small lexer, as Notepad++'s UDL lexer does.

use std::ops::Range;

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
    delimiters: Vec<(u8, Delimiter)>,
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
            }
            "Folders in code2, open" | "Folders in code2, middle" | "Folders in code2, close" => {
                language.folders2.extend(words(text));
            }
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
    for list in [&mut language.operators, &mut language.folders1] {
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

    fn word_kind(&self, word: &str) -> Option<UdlKind> {
        let is_number = word.chars().next().is_some_and(|ch| ch.is_ascii_digit())
            && word
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, '.' | 'x' | 'X' | '_'));
        if is_number {
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
        let line_end = |at: usize| {
            text[at..]
                .find(['\n', '\r'])
                .map_or(text.len(), |end| at + end)
        };
        while pos < text.len() {
            let rest = &text[pos..];
            if let Some((open, close)) = self
                .block_comments
                .iter()
                .find(|(open, _)| self.starts_with(rest, open))
            {
                let after = pos + open.len();
                let end = text[after..]
                    .find(close.as_str())
                    .map_or(text.len(), |end| after + end + close.len());
                push(pos..end, UdlKind::Comment);
                pos = end;
                continue;
            }
            if let Some(open) = self
                .line_comments
                .iter()
                .find(|open| self.starts_with(rest, open))
            {
                let end = line_end(pos + open.len());
                push(pos..end, UdlKind::LineComment);
                pos = end;
                continue;
            }
            if let Some((index, delimiter)) = self
                .delimiters
                .iter()
                .find(|(_, delimiter)| self.starts_with(rest, &delimiter.open))
            {
                let mut at = pos + delimiter.open.len();
                let end = loop {
                    if at >= text.len() {
                        break text.len();
                    }
                    let tail = &text[at..];
                    if !delimiter.escape.is_empty() && tail.starts_with(delimiter.escape.as_str()) {
                        at += delimiter.escape.len();
                        at += text[at..].chars().next().map_or(0, char::len_utf8);
                        continue;
                    }
                    if delimiter.close.is_empty() {
                        if tail.starts_with(['\n', '\r']) {
                            break at;
                        }
                    } else if self.starts_with(tail, &delimiter.close) {
                        break at + delimiter.close.len();
                    }
                    at += tail.chars().next().map_or(1, char::len_utf8);
                };
                push(pos..end, UdlKind::Delimiter(*index));
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
            // A word: up to white space or a symbol.
            let mut end = pos + ch.len_utf8();
            while end < text.len() {
                let next = text[end..].chars().next().expect("not at the end");
                if next.is_whitespace() || self.symbol_at(&text[end..]) {
                    break;
                }
                end += next.len_utf8();
            }
            if let Some(kind) = self.word_kind(&text[pos..end]) {
                push(pos..end, kind);
            }
            pos = end;
        }
        out
    }
}

#[cfg(test)]
mod tests;
