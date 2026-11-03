//! The files a PlantUML diagram includes, as its preprocessor finds them (ADR 0062): the text
//! is read in order, as PlantUML reads it, with its variables (`!$HOME ?= "../.."`) and the
//! functions that return a value (`!function FromRoot($p)` ... `!return $HOME + $p`) kept, so
//! that an `!include` written with them (`!include $HOME/main.iuml`,
//! `!include FromRoot(/billing/containers.puml)`) names a file; each file it includes is read in turn
//! where its `!include` is. What Birchpad cannot work out (a function of PlantUML's own that it
//! does not know, a variable never set) leaves that include out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use birchpad_syntax::plantuml::include_argument;

use super::diagrams::normalized;

/// The most files [`walk_includes`] reads, so that a loop of includes or a huge library ends.
pub const MOST_INCLUDED_FILES: usize = 500;

/// How deep functions may call functions before Birchpad gives up on a value.
const MOST_CALLS: usize = 16;

/// An `!include` that was followed: the file it is in (`None` for the document), what it says
/// after the directive, and the file it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeLink {
    pub from: Option<PathBuf>,
    pub written: String,
    pub path: PathBuf,
}

/// What reading a document's includes found: the files, each once in the order PlantUML reads
/// them, and the `!include` lines that named them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Includes {
    pub files: Vec<PathBuf>,
    pub links: Vec<IncludeLink>,
}

/// A function of the diagram's own that returns a value.
#[derive(Debug, Clone)]
struct Function {
    /// Its parameters' names, with their `$`, and their default values.
    parameters: Vec<(String, Option<String>)>,
    /// What its `!return` returns.
    returns: Option<String>,
}

/// What the preprocessor knows at a point of the text.
#[derive(Debug, Default)]
pub struct Preprocessor {
    variables: HashMap<String, String>,
    functions: HashMap<String, Function>,
}

/// Whether `line` (trimmed) starts with the directive `word` (`!function`), and what follows.
fn directive<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let head = line.get(..word.len())?;
    let rest = &line[word.len()..];
    (head.eq_ignore_ascii_case(word) && (rest.is_empty() || rest.starts_with([' ', '\t'])))
        .then(|| rest.trim())
}

/// `text` split at the top-level `separator`s: outside quotes and parentheses.
fn split_top(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut start) = (0usize, None, 0);
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, c) if c == separator && depth == 0 => {
                parts.push(&text[start..at]);
                start = at + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The length of the name at the start of `text`: letters, digits and `_`.
fn name_length(text: &str) -> usize {
    text.find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len())
}

/// The arguments of a call whose `(` `text` starts after, and the length up to its `)`.
fn call_arguments(text: &str) -> Option<(Vec<&str>, usize)> {
    let mut depth = 1usize;
    let mut quote = None;
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    let inside = &text[..at];
                    let arguments = if inside.trim().is_empty() {
                        Vec::new()
                    } else {
                        split_top(inside, ',').into_iter().map(str::trim).collect()
                    };
                    return Some((arguments, at + 1));
                }
            }
            _ => {}
        }
    }
    None
}

impl Preprocessor {
    /// The value of the expression `expr` (`"../.."`, `$HOME + "/lib"`, `FromRoot($p)`), with
    /// `locals` (a function's parameters) and `folder` (that of the file it is in, for
    /// `%dirpath()`); `None` when a part of it is unknown.
    pub fn evaluate(
        &self,
        expr: &str,
        locals: &HashMap<String, String>,
        folder: Option<&Path>,
    ) -> Option<String> {
        self.evaluate_at(expr, locals, folder, 0)
    }

    fn evaluate_at(
        &self,
        expr: &str,
        locals: &HashMap<String, String>,
        folder: Option<&Path>,
        depth: usize,
    ) -> Option<String> {
        let mut value = String::new();
        for term in split_top(expr, '+') {
            let term = term.trim();
            let quoted = term.len() >= 2
                && ((term.starts_with('"') && term.ends_with('"'))
                    || (term.starts_with('\'') && term.ends_with('\'')));
            if quoted {
                value.push_str(&term[1..term.len() - 1]);
            } else if !term.is_empty() && term.chars().all(|c| c.is_ascii_digit()) {
                value.push_str(term);
            } else {
                // A variable or a call, and nothing after it.
                let (part, length) = self.reference(term, locals, folder, depth)?;
                if length != term.len() {
                    return None;
                }
                value.push_str(&part);
            }
        }
        Some(value)
    }

    /// The variable or call `text` starts with (`$HOME`, `FromRoot(...)`, `%dirpath()`): its
    /// value and its length; `None` when it is none of them or its value is unknown.
    fn reference(
        &self,
        text: &str,
        locals: &HashMap<String, String>,
        folder: Option<&Path>,
        depth: usize,
    ) -> Option<(String, usize)> {
        let sigil = usize::from(text.starts_with(['$', '%']));
        let length = sigil + name_length(&text[sigil..]);
        if length == sigil {
            return None;
        }
        let name = &text[..length];
        let Some(after) = text[length..].strip_prefix('(') else {
            // A variable.
            if !name.starts_with('$') {
                return None;
            }
            let value = locals.get(name).or_else(|| self.variables.get(name))?;
            return Some((value.clone(), length));
        };
        let (arguments, used) = call_arguments(after)?;
        let length = length + 1 + used;
        if depth >= MOST_CALLS {
            return None;
        }
        // Arguments: expressions, or text as written (`FromRoot(/billing/containers.puml)`).
        let values: Vec<String> = arguments
            .iter()
            .map(|argument| {
                self.evaluate_at(argument, locals, folder, depth + 1)
                    .unwrap_or_else(|| (*argument).to_owned())
            })
            .collect();
        let value = match name.to_ascii_lowercase().as_str() {
            "%dirpath" => folder?.display().to_string(),
            "%getenv" => std::env::var(values.first()?).ok()?,
            _ => {
                let function = self.functions.get(name)?;
                let mut inner = HashMap::new();
                for (index, (parameter, default)) in function.parameters.iter().enumerate() {
                    let value = values.get(index).cloned().or_else(|| default.clone())?;
                    inner.insert(parameter.clone(), value);
                }
                let returns = function.returns.as_deref()?;
                self.evaluate_at(returns, &inner, folder, depth + 1)?
            }
        };
        Some((value, length))
    }

    /// `text` (what follows an `!include`) with its variables and calls replaced by their values;
    /// `None` when one of them is unknown.
    pub fn substitute(&self, text: &str, folder: Option<&Path>) -> Option<String> {
        let locals = HashMap::new();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            let starts_name = rest.starts_with(['$', '%'])
                || (rest.starts_with(|c: char| c.is_alphabetic() || c == '_')
                    && !text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_'));
            if starts_name {
                let sigil = usize::from(rest.starts_with(['$', '%']));
                let length = sigil + name_length(&rest[sigil..]);
                let is_call = rest[length..].starts_with('(');
                let known = sigil == 1 || (is_call && self.functions.contains_key(&rest[..length]));
                if known {
                    let (value, used) = self.reference(rest, &locals, folder, 0)?;
                    out.push_str(&value);
                    at += used;
                    continue;
                }
                out.push_str(&rest[..length.max(1)]);
                at += length.max(1);
                continue;
            }
            let c = rest.chars().next().expect("not at the end");
            out.push(c);
            at += c.len_utf8();
        }
        Some(out)
    }

    /// Learns what `line` (trimmed) sets: a variable (`!$x = ...`, `!$x ?= ...`, with `!global`)
    /// or a function on one line (`!function $f($a) !return $a`). Returns the function a line
    /// opens, whose body follows, to learn its `!return`.
    fn learn(&mut self, line: &str, folder: Option<&Path>) -> Option<(String, Function)> {
        for word in ["!function", "!unquoted function"] {
            if let Some(rest) = directive(line, word) {
                let (head, one_line) = match rest.find("!return") {
                    Some(at) => (&rest[..at], Some(rest[at + "!return".len()..].trim())),
                    None => (rest, None),
                };
                let open = head.find('(')?;
                let name = head[..open].trim().to_owned();
                let (parameters, _) = call_arguments(&head[open + 1..])?;
                let parameters = parameters
                    .iter()
                    .map(|parameter| match parameter.split_once('=') {
                        Some((name, default)) => (
                            name.trim().to_owned(),
                            self.evaluate(default, &HashMap::new(), folder),
                        ),
                        None => (parameter.trim().to_owned(), None),
                    })
                    .collect();
                let function = Function {
                    parameters,
                    returns: one_line.map(str::to_owned),
                };
                if one_line.is_some() {
                    self.functions.insert(name, function);
                    return None;
                }
                return Some((name, function));
            }
        }
        let assignment = directive(line, "!global")
            .map(|rest| format!("!{rest}"))
            .unwrap_or_else(|| line.to_owned());
        let Some(rest) = assignment.strip_prefix("!$") else {
            return None;
        };
        let length = name_length(rest);
        let name = format!("${}", &rest[..length]);
        let after = rest[length..].trim_start();
        let (optional, expr) = match (after.strip_prefix("?="), after.strip_prefix('=')) {
            (Some(expr), _) => (true, expr),
            (None, Some(expr)) => (false, expr),
            _ => return None,
        };
        if optional && self.variables.contains_key(&name) {
            return None;
        }
        if let Some(value) = self.evaluate(expr.trim(), &HashMap::new(), folder) {
            self.variables.insert(name, value);
        }
        None
    }
}

/// A function whose body is being read, or a procedure's (`Some(None)`), whose lines set
/// nothing; `None` outside of them.
#[derive(Default)]
struct Body(Option<Option<(String, Function)>>);

impl Body {
    /// Reads `line` (trimmed) into `preprocessor`: whether it was a definition's, or one that
    /// sets something, rather than one to look at for an include.
    fn read(&mut self, line: &str, preprocessor: &mut Preprocessor, folder: Option<&Path>) -> bool {
        if let Some(open) = &mut self.0 {
            if let Some(returns) = directive(line, "!return")
                && let Some((_, function)) = open
                && function.returns.is_none()
            {
                function.returns = Some(returns.to_owned());
            }
            let ends = ["!endfunction", "!endprocedure", "!enddefinelong"]
                .iter()
                .any(|end| directive(line, end).is_some());
            if ends && let Some(Some((name, function))) = self.0.take() {
                preprocessor.functions.insert(name, function);
            } else if ends {
                self.0 = None;
            }
            return true;
        }
        let procedure = ["!procedure", "!unquoted procedure", "!definelong"]
            .iter()
            .any(|word| directive(line, word).is_some());
        if procedure {
            self.0 = Some(None);
            return true;
        }
        if let Some(function) = preprocessor.learn(line, folder) {
            self.0 = Some(Some(function));
            return true;
        }
        false
    }
}

impl Preprocessor {
    /// Reads the variables and functions `text` sets, its includes left alone.
    pub fn read(&mut self, text: &str, folder: Option<&Path>) {
        let mut body = Body::default();
        for line in text.lines() {
            body.read(line.trim(), self, folder);
        }
    }
}

/// Reads the includes of `text`, the document's (in `folder`), as PlantUML does: in order, each
/// included file read where its `!include` is, a target looked for next to the file that
/// includes it, then in each folder of `folders` (the include path, the document's folder
/// first). Files that are not found are left out; at most [`MOST_INCLUDED_FILES`].
pub fn walk_includes(text: &str, folder: Option<&Path>, folders: &[PathBuf]) -> Includes {
    let mut preprocessor = Preprocessor::default();
    let mut found = Includes::default();
    visit(text, None, folder, folders, &mut preprocessor, &mut found);
    found
}

/// Reads one file's lines; false once [`MOST_INCLUDED_FILES`] are found.
fn visit(
    text: &str,
    from: Option<&Path>,
    base: Option<&Path>,
    folders: &[PathBuf],
    preprocessor: &mut Preprocessor,
    found: &mut Includes,
) -> bool {
    let mut body = Body::default();
    for line in text.lines() {
        if body.read(line.trim(), preprocessor, base) {
            continue;
        }
        let Some(range) = include_argument(line) else {
            continue;
        };
        let written = &line[range];
        let Some(target) = preprocessor.substitute(written, base) else {
            continue;
        };
        // `"with space.puml"`, `common.puml!2` once its variables are in: the file alone.
        let target = match target.trim().strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => target.split('!').next().unwrap_or_default().trim(),
        };
        if target.is_empty() || target.starts_with('<') || target.contains("://") {
            continue;
        }
        let target = Path::new(target);
        let file = if target.is_absolute() {
            Some(target.to_path_buf()).filter(|file| file.is_file())
        } else {
            base.into_iter()
                .chain(folders.iter().map(PathBuf::as_path))
                .map(|folder| folder.join(target))
                .find(|file| file.is_file())
        };
        let Some(file) = file.map(|file| normalized(&file)) else {
            continue;
        };
        found.links.push(IncludeLink {
            from: from.map(Path::to_path_buf),
            written: written.to_owned(),
            path: file.clone(),
        });
        if found.files.contains(&file) {
            continue;
        }
        if found.files.len() == MOST_INCLUDED_FILES {
            return false;
        }
        found.files.push(file.clone());
        if let Ok(bytes) = std::fs::read(&file) {
            let content = String::from_utf8_lossy(&bytes).into_owned();
            if !visit(
                &content,
                Some(&file),
                file.parent(),
                folders,
                preprocessor,
                found,
            ) {
                return false;
            }
        }
    }
    true
}
