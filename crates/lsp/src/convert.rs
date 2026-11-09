//! The servers' answers as plain values for the window: edits as byte ranges of the text,
//! locations as paths, completion items, hover text and diagnostics.

use std::collections::BTreeMap;
use std::path::PathBuf;

use birchpad_core::{Edit, Rope};
use lsp_types::{
    CompletionItem, CompletionResponse, CompletionTextEdit, DiagnosticSeverity,
    DocumentChangeOperation, DocumentChanges, GotoDefinitionResponse, HoverContents,
    InsertTextFormat, Location, MarkedString, OneOf, TextEdit, WorkspaceEdit,
};

use crate::position::{PositionEncoding, byte_range};
use crate::uri::file_path;

/// Why a server's answer cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConvertError {
    #[error("{}", t!("lsp.edits-overlap"))]
    Overlapping,
    #[error("{}", t!("lsp.not-a-file", uri = .0))]
    NotAFile(String),
    #[error("{}", t!("lsp.changes-files"))]
    ResourceOperation,
}

/// `edits` of `text` as Birchpad's edits, in the order of the text; edits at the same place
/// keep the server's order. Overlapping edits are refused, as the protocol forbids them.
pub fn text_edits(
    text: &Rope,
    edits: &[TextEdit],
    encoding: PositionEncoding,
) -> Result<Vec<Edit>, ConvertError> {
    let mut converted: Vec<Edit> = edits
        .iter()
        .map(|edit| Edit {
            range: byte_range(text, edit.range, encoding),
            text: edit.new_text.clone(),
        })
        .collect();
    converted.sort_by_key(|edit| edit.range.start);
    for pair in converted.windows(2) {
        if pair[0].range.end > pair[1].range.start {
            return Err(ConvertError::Overlapping);
        }
    }
    Ok(converted)
}

/// The text edits of each file a workspace edit changes, by path. Creating, renaming and
/// deleting files is refused as a whole (ADR 0065).
pub fn workspace_edits(
    edit: &WorkspaceEdit,
) -> Result<BTreeMap<PathBuf, Vec<TextEdit>>, ConvertError> {
    let mut files: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new();
    let mut add = |uri: &lsp_types::Uri, edits: Vec<TextEdit>| -> Result<(), ConvertError> {
        let path = file_path(uri).ok_or_else(|| ConvertError::NotAFile(uri.as_str().to_owned()))?;
        files.entry(path).or_default().extend(edits);
        Ok(())
    };
    let plain = |edits: &[OneOf<TextEdit, lsp_types::AnnotatedTextEdit>]| -> Vec<TextEdit> {
        edits
            .iter()
            .map(|edit| match edit {
                OneOf::Left(edit) => edit.clone(),
                OneOf::Right(annotated) => annotated.text_edit.clone(),
            })
            .collect()
    };
    match &edit.document_changes {
        Some(DocumentChanges::Edits(documents)) => {
            for document in documents {
                add(&document.text_document.uri, plain(&document.edits))?;
            }
        }
        Some(DocumentChanges::Operations(operations)) => {
            if operations
                .iter()
                .any(|operation| matches!(operation, DocumentChangeOperation::Op(_)))
            {
                return Err(ConvertError::ResourceOperation);
            }
            for operation in operations {
                if let DocumentChangeOperation::Edit(document) = operation {
                    add(&document.text_document.uri, plain(&document.edits))?;
                }
            }
        }
        // `changes` counts only without `documentChanges`, as the protocol says.
        None => {
            let mut changes: Vec<_> = edit.changes.iter().flatten().collect();
            changes.sort_by(|a, b| a.0.cmp(b.0));
            for (uri, edits) in changes {
                add(uri, edits.clone())?;
            }
        }
    }
    Ok(files)
}

/// A place in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRange {
    pub path: PathBuf,
    pub range: lsp_types::Range,
}

fn file_range(location: &Location) -> Option<FileRange> {
    Some(FileRange {
        path: file_path(&location.uri)?,
        range: location.range,
    })
}

/// The places of a definition answer; those not in files are left out.
pub fn definitions(response: Option<GotoDefinitionResponse>) -> Vec<FileRange> {
    match response {
        None => Vec::new(),
        Some(GotoDefinitionResponse::Scalar(location)) => {
            file_range(&location).into_iter().collect()
        }
        Some(GotoDefinitionResponse::Array(locations)) => {
            locations.iter().filter_map(file_range).collect()
        }
        Some(GotoDefinitionResponse::Link(links)) => links
            .iter()
            .filter_map(|link| {
                Some(FileRange {
                    path: file_path(&link.target_uri)?,
                    range: link.target_selection_range,
                })
            })
            .collect(),
    }
}

/// The places of a references answer, by path and position.
pub fn references(response: Option<Vec<Location>>) -> Vec<FileRange> {
    let mut found: Vec<FileRange> = response.iter().flatten().filter_map(file_range).collect();
    found.sort_by(|a, b| {
        (&a.path, a.range.start.line, a.range.start.character).cmp(&(
            &b.path,
            b.range.start.line,
            b.range.start.character,
        ))
    });
    found.dedup();
    found
}

/// An entry of the completion list from a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub detail: Option<String>,
    /// What is inserted in place of the word typed.
    pub insert: String,
    /// The range it replaces, when the server says, instead of the word typed.
    pub range: Option<lsp_types::Range>,
    /// What the list is filtered by.
    pub filter: String,
    /// What the list is sorted by.
    pub sort: String,
}

/// The entries of a completion answer, in the server's order of `sortText`, then label.
pub fn completions(response: Option<CompletionResponse>) -> Vec<Completion> {
    let items = match response {
        None => Vec::new(),
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
    };
    let mut entries: Vec<Completion> = items.iter().map(completion).collect();
    entries.sort_by(|a, b| (&a.sort, &a.label).cmp(&(&b.sort, &b.label)));
    entries
}

fn completion(item: &CompletionItem) -> Completion {
    let snippet = item.insert_text_format == Some(InsertTextFormat::SNIPPET);
    let (text, range) = match &item.text_edit {
        Some(CompletionTextEdit::Edit(edit)) => (edit.new_text.clone(), Some(edit.range)),
        Some(CompletionTextEdit::InsertAndReplace(edit)) => {
            (edit.new_text.clone(), Some(edit.insert))
        }
        None => (
            item.insert_text
                .clone()
                .unwrap_or_else(|| item.label.clone()),
            None,
        ),
    };
    let insert = if snippet { snippet_text(&text) } else { text };
    Completion {
        label: item.label.clone(),
        detail: item
            .detail
            .clone()
            .filter(|detail| !detail.trim().is_empty())
            .map(|detail| detail.lines().next().unwrap_or_default().trim().to_owned()),
        insert,
        range,
        filter: item
            .filter_text
            .clone()
            .unwrap_or_else(|| item.label.clone()),
        sort: item.sort_text.clone().unwrap_or_else(|| item.label.clone()),
    }
}

/// Whether the list keeps `entry` for `typed`: its filter text has the typed characters in
/// order, ignoring case, the first of them first.
pub fn matches(filter: &str, typed: &str) -> bool {
    let mut filter = filter.chars().flat_map(char::to_lowercase);
    let mut typed = typed.chars().flat_map(char::to_lowercase).peekable();
    let Some(first) = typed.next() else {
        return true;
    };
    if filter.next() != Some(first) {
        return false;
    }
    for wanted in typed {
        if !filter.by_ref().any(|c| c == wanted) {
            return false;
        }
    }
    true
}

/// A snippet's plain text: tab stops (`$1`, `${2}`) gone, placeholders (`${1:name}`) and the
/// first of choices (`${1|a,b|}`) as their text, variables (`$TM_FILENAME`) gone, `\$`, `\}`
/// and `\\` as the character.
pub fn snippet_text(snippet: &str) -> String {
    let chars: Vec<char> = snippet.chars().collect();
    let mut out = String::new();
    plain(&chars, &mut 0, &mut out, false);
    out
}

/// Copies snippet text from `at`, up to the `}` that closes a placeholder when `nested`.
fn plain(chars: &[char], at: &mut usize, out: &mut String, nested: bool) {
    while *at < chars.len() {
        let c = chars[*at];
        match c {
            '\\' if chars
                .get(*at + 1)
                .is_some_and(|next| matches!(next, '$' | '}' | '\\' | ',' | '|')) =>
            {
                out.push(chars[*at + 1]);
                *at += 2;
            }
            '}' if nested => {
                *at += 1;
                return;
            }
            '$' => {
                *at += 1;
                match chars.get(*at) {
                    Some(d) if d.is_ascii_digit() || d.is_ascii_alphabetic() || *d == '_' => {
                        while chars
                            .get(*at)
                            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                        {
                            *at += 1;
                        }
                    }
                    Some('{') => {
                        *at += 1;
                        while chars
                            .get(*at)
                            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                        {
                            *at += 1;
                        }
                        match chars.get(*at) {
                            Some(':') => {
                                *at += 1;
                                plain(chars, at, out, true);
                            }
                            Some('|') => {
                                *at += 1;
                                let mut first = true;
                                while let Some(&c) = chars.get(*at) {
                                    *at += 1;
                                    match c {
                                        '|' => break,
                                        ',' => first = false,
                                        '\\' => {
                                            if let Some(&next) = chars.get(*at) {
                                                if first {
                                                    out.push(next);
                                                }
                                                *at += 1;
                                            }
                                        }
                                        c if first => out.push(c),
                                        _ => {}
                                    }
                                }
                                if chars.get(*at) == Some(&'}') {
                                    *at += 1;
                                }
                            }
                            Some('}') => *at += 1,
                            // `${` not closed: the rest as it is.
                            _ => {}
                        }
                    }
                    _ => out.push('$'),
                }
            }
            c => {
                out.push(c);
                *at += 1;
            }
        }
    }
}

/// A hover's text, plain: Markdown's code fences dropped, the parts separated by a blank line.
pub fn hover_text(contents: &HoverContents) -> String {
    let marked = |marked: &MarkedString| match marked {
        MarkedString::String(text) => markdown_text(text),
        MarkedString::LanguageString(code) => code.value.trim().to_owned(),
    };
    let parts: Vec<String> = match contents {
        HoverContents::Scalar(one) => vec![marked(one)],
        HoverContents::Array(many) => many.iter().map(marked).collect(),
        HoverContents::Markup(markup) => vec![match markup.kind {
            lsp_types::MarkupKind::Markdown => markdown_text(&markup.value),
            lsp_types::MarkupKind::PlainText => markup.value.trim().to_owned(),
        }],
    };
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Markdown as plain text: code fences and rules dropped, backslash escapes undone.
fn markdown_text(markdown: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            continue;
        }
        if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-' || c == '_' || c == '*') {
            lines.push(String::new());
            continue;
        }
        let mut plain = String::with_capacity(line.len());
        let mut chars = line.trim_end().chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek().is_some_and(|next| next.is_ascii_punctuation()) {
                plain.extend(chars.next());
            } else {
                plain.push(c);
            }
        }
        lines.push(plain);
    }
    // Blank lines at either end, and runs of them, go.
    let mut text = String::new();
    let mut blank = false;
    for line in lines {
        if line.trim().is_empty() {
            blank = !text.is_empty();
            continue;
        }
        if blank {
            text.push_str("\n\n");
        } else if !text.is_empty() {
            text.push('\n');
        }
        blank = false;
        text.push_str(&line);
    }
    text
}

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    /// The severity a server gave; one it left out is an error, as clients mostly read it.
    pub fn of(severity: Option<DiagnosticSeverity>) -> Self {
        match severity {
            Some(DiagnosticSeverity::WARNING) => Self::Warning,
            Some(DiagnosticSeverity::INFORMATION) => Self::Information,
            Some(DiagnosticSeverity::HINT) => Self::Hint,
            _ => Self::Error,
        }
    }

    /// Its name, in the interface's language.
    pub fn label(self) -> String {
        match self {
            Self::Error => t!("lsp.error"),
            Self::Warning => t!("lsp.warning"),
            Self::Information => t!("lsp.information"),
            Self::Hint => t!("lsp.hint"),
        }
    }
}

/// A diagnostic's message as shown: `source: message`.
pub fn diagnostic_message(diagnostic: &lsp_types::Diagnostic) -> String {
    let message = diagnostic.message.trim();
    match diagnostic.source.as_deref().map(str::trim) {
        Some(source) if !source.is_empty() => format!("{source}: {message}"),
        _ => message.to_owned(),
    }
}
