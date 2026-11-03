//! The files a PlantUML diagram includes, as its preprocessor finds them (ADR 0062): the text
//! is read in order, as PlantUML reads it, with its variables (`!$HOME ?= "../.."`) and the
//! functions that return a value (`!function FromRoot($p)` ... `!return $HOME + $p`) kept, so
//! that an `!include` written with them (`!include $HOME/main.iuml`,
//! `!include FromRoot(/billing/containers.puml)`) names a file; each file it includes is read
//! in turn where its `!include` is. A function's body is run as PlantUML runs it: its `!local`
//! variables, its `!if`, `!elseif` and `!else`, and the built-in functions paths are made with
//! (`%get_variable_value`, `%strpos`, `%string` and their kin). What Birchpad cannot work out (a
//! function it does not know, a variable never set) leaves that include out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use birchpad_syntax::plantuml::include_argument;

use super::diagrams::normalized;

/// The most files [`walk_includes`] reads, so that a loop of includes or a huge library ends.
pub const MOST_INCLUDED_FILES: usize = 500;

/// How deep functions may call functions before Birchpad gives up on a value.
const MOST_CALLS: usize = 16;

/// The most lines a function's body may run in one call, so that no body loops for long.
const MOST_LINES: usize = 10_000;

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
    /// The lines of its body, trimmed: `!local`, `!if` and `!return` lines among them.
    body: Vec<String>,
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

/// Whether a value counts as true in an `!if`: anything but an empty text, `0` and `false`.
fn truthy(value: &str) -> bool {
    !matches!(value.trim(), "" | "0" | "false")
}

fn boolean(value: bool) -> String {
    String::from(if value { "1" } else { "0" })
}

/// Where a value is worked out: the preprocessor, a function's `locals`, the folder of the file
/// being read (for `%dirpath()`), and how deep calls are.
struct Scope<'a> {
    preprocessor: &'a Preprocessor,
    locals: &'a HashMap<String, String>,
    folder: Option<&'a Path>,
    depth: usize,
}

impl Scope<'_> {
    fn variable(&self, name: &str) -> Option<&String> {
        self.locals
            .get(name)
            .or_else(|| self.preprocessor.variables.get(name))
    }

    /// The value of the whole of `expr`; `None` when a part of it is unknown, or it is no
    /// expression.
    fn evaluate(&self, expr: &str) -> Option<String> {
        let mut parser = Parser {
            scope: self,
            text: expr,
            at: 0,
        };
        let value = parser.or()?;
        parser.skip_spaces();
        (parser.at == expr.len()).then_some(value)
    }

    /// Calls `name` (`FromRoot`, `$twice`, `%strpos`) with its arguments as written.
    fn call(&self, name: &str, arguments: &[&str]) -> Option<String> {
        if self.depth >= MOST_CALLS {
            return None;
        }
        // Arguments: expressions, or text as written (`FromRoot(/billing/containers.puml)`,
        // as PlantUML takes an unquoted function's).
        let values: Vec<String> = arguments
            .iter()
            .map(|argument| {
                self.evaluate(argument)
                    .unwrap_or_else(|| (*argument).to_owned())
            })
            .collect();
        let text = |index: usize| values.get(index).map(String::as_str);
        let number = |index: usize| text(index)?.trim().parse::<i64>().ok();
        let value = match name.to_ascii_lowercase().as_str() {
            "%dirpath" => self.folder?.display().to_string(),
            "%getenv" => std::env::var(text(0)?).ok().unwrap_or_default(),
            "%get_variable_value" => self.variable(text(0)?).cloned().unwrap_or_default(),
            "%variable_exists" => boolean(self.variable(text(0)?).is_some()),
            "%string" => text(0)?.to_owned(),
            "%intval" => number(0)?.to_string(),
            "%strlen" => text(0)?.chars().count().to_string(),
            "%strpos" => {
                let (haystack, needle) = (text(0)?, text(1)?);
                match haystack.find(needle) {
                    Some(at) => haystack[..at].chars().count().to_string(),
                    None => String::from("-1"),
                }
            }
            "%substr" => {
                let start = usize::try_from(number(1)?).ok()?;
                let chars = text(0)?.chars().skip(start);
                match number(2) {
                    Some(length) => chars.take(usize::try_from(length).ok()?).collect(),
                    None => chars.collect(),
                }
            }
            "%lower" => text(0)?.to_lowercase(),
            "%upper" => text(0)?.to_uppercase(),
            "%true" => boolean(true),
            "%false" => boolean(false),
            "%not" => boolean(!truthy(text(0)?)),
            "%boolval" => boolean(truthy(text(0)?)),
            _ => {
                let function = self.preprocessor.functions.get(name)?;
                let mut locals = HashMap::new();
                for (index, (parameter, default)) in function.parameters.iter().enumerate() {
                    let value = values.get(index).cloned().or_else(|| default.clone())?;
                    locals.insert(parameter.clone(), value);
                }
                self.run(&function.body, locals)?
            }
        };
        Some(value)
    }

    /// Runs a function's `body` with its parameters in `locals`: its `!local` and `!$`
    /// assignments, its `!if`, `!elseif`, `!else` and `!endif`, up to the `!return` it reaches.
    fn run(&self, body: &[String], mut locals: HashMap<String, String>) -> Option<String> {
        // For each `!if` met: whether its branch runs, and whether one of its branches ran.
        let mut branches: Vec<(bool, bool)> = Vec::new();
        for (count, line) in body.iter().enumerate() {
            if count == MOST_LINES {
                return None;
            }
            let running = branches.iter().all(|(runs, _)| *runs);
            let inner = Scope {
                preprocessor: self.preprocessor,
                locals: &locals,
                folder: self.folder,
                depth: self.depth + 1,
            };
            let condition = |expr: &str| inner.evaluate(expr).map(|value| truthy(&value));
            if let Some(expr) = directive(line, "!if") {
                let runs = running && condition(expr)?;
                branches.push((runs, runs));
            } else if let Some(expr) = directive(line, "!elseif") {
                let outer =
                    branches.len() < 2 || branches[..branches.len() - 1].iter().all(|b| b.0);
                let (runs, ran) = branches.last_mut()?;
                *runs = outer && !*ran && condition(expr)?;
                *ran |= *runs;
            } else if directive(line, "!else").is_some() {
                let outer =
                    branches.len() < 2 || branches[..branches.len() - 1].iter().all(|b| b.0);
                let (runs, ran) = branches.last_mut()?;
                *runs = outer && !*ran;
                *ran = true;
            } else if directive(line, "!endif").is_some() {
                branches.pop()?;
            } else if !running {
                continue;
            } else if let Some(expr) = directive(line, "!return") {
                return inner.evaluate(expr);
            } else if let Some((name, expr)) = assignment(line) {
                let value = inner.evaluate(expr)?;
                locals.insert(name, value);
            }
        }
        None
    }
}

/// `!local $x = value`, `!$x = value` or `!$x ?= value`: the variable and its expression.
fn assignment(line: &str) -> Option<(String, &str)> {
    let line = directive(line, "!local")
        .or_else(|| directive(line, "!global"))
        .map_or(line, |rest| rest);
    let rest = line.strip_prefix("!$").or_else(|| line.strip_prefix('$'))?;
    let length = name_length(rest);
    let name = format!("${}", &rest[..length]);
    let after = rest[length..].trim_start();
    let expr = after
        .strip_prefix("?=")
        .or_else(|| after.strip_prefix('='))?;
    Some((name, expr.trim()))
}

/// Reads an expression: `||`, `&&`, comparisons, `+` and `-`, `!`, and values: strings in
/// double or single quotes, numbers, variables, calls and parentheses.
struct Parser<'a, 'b> {
    scope: &'a Scope<'b>,
    text: &'a str,
    at: usize,
}

impl<'a> Parser<'a, '_> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn skip_spaces(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    /// Takes `token` if the text goes on with it.
    fn take(&mut self, token: &str) -> bool {
        self.skip_spaces();
        if self.rest().starts_with(token) {
            self.at += token.len();
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Option<String> {
        let mut value = self.and()?;
        while self.take("||") {
            let right = self.and()?;
            value = boolean(truthy(&value) || truthy(&right));
        }
        Some(value)
    }

    fn and(&mut self) -> Option<String> {
        let mut value = self.comparison()?;
        while self.take("&&") {
            let right = self.comparison()?;
            value = boolean(truthy(&value) && truthy(&right));
        }
        Some(value)
    }

    fn comparison(&mut self) -> Option<String> {
        let left = self.sum()?;
        for operator in ["==", "!=", "<=", ">=", "<", ">"] {
            if self.take(operator) {
                let right = self.sum()?;
                let order = match (left.trim().parse::<i64>(), right.trim().parse::<i64>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    _ => left.as_str().cmp(right.as_str()),
                };
                let holds = match operator {
                    "==" => order.is_eq(),
                    "!=" => order.is_ne(),
                    "<=" => order.is_le(),
                    ">=" => order.is_ge(),
                    "<" => order.is_lt(),
                    _ => order.is_gt(),
                };
                return Some(boolean(holds));
            }
        }
        Some(left)
    }

    fn sum(&mut self) -> Option<String> {
        let mut value = self.unary()?;
        loop {
            if self.take("+") {
                let right = self.unary()?;
                value = match (value.parse::<i64>(), right.parse::<i64>()) {
                    (Ok(left), Ok(right)) => left.checked_add(right)?.to_string(),
                    _ => value + right.as_str(),
                };
            } else if self.take("-") {
                let right = self.unary()?.parse::<i64>().ok()?;
                value = value.parse::<i64>().ok()?.checked_sub(right)?.to_string();
            } else {
                return Some(value);
            }
        }
    }

    fn unary(&mut self) -> Option<String> {
        self.skip_spaces();
        if self.rest().starts_with('!') && !self.rest().starts_with("!=") {
            self.at += 1;
            return Some(boolean(!truthy(&self.unary()?)));
        }
        self.value()
    }

    fn value(&mut self) -> Option<String> {
        self.skip_spaces();
        let rest = self.rest();
        let first = rest.chars().next()?;
        if first == '"' || first == '\'' {
            let end = rest[1..].find(first)?;
            self.at += end + 2;
            return Some(rest[1..=end].to_owned());
        }
        if first.is_ascii_digit() {
            let length = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            self.at += length;
            return Some(rest[..length].to_owned());
        }
        if first == '(' {
            self.at += 1;
            let value = self.or()?;
            return self.take(")").then_some(value);
        }
        let sigil = usize::from(first == '$' || first == '%');
        let length = sigil + name_length(&rest[sigil..]);
        if length == sigil {
            return None;
        }
        let name = &rest[..length];
        if let Some(after) = rest[length..].strip_prefix('(') {
            let (arguments, used) = call_arguments(after)?;
            self.at += length + 1 + used;
            return self.scope.call(name, &arguments);
        }
        if sigil == 1 && first == '$' {
            self.at += length;
            return self.scope.variable(name).cloned();
        }
        None
    }
}

impl Preprocessor {
    fn scope<'a>(
        &'a self,
        locals: &'a HashMap<String, String>,
        folder: Option<&'a Path>,
    ) -> Scope<'a> {
        Scope {
            preprocessor: self,
            locals,
            folder,
            depth: 0,
        }
    }

    /// The value of the expression `expr` (`"../.."`, `$HOME + "/lib"`, `FromRoot($p)`,
    /// `%strpos($a, "x") >= 0`), with `locals` (a function's parameters) and `folder` (that of
    /// the file it is in, for `%dirpath()`); `None` when a part of it is unknown.
    pub fn evaluate(
        &self,
        expr: &str,
        locals: &HashMap<String, String>,
        folder: Option<&Path>,
    ) -> Option<String> {
        self.scope(locals, folder).evaluate(expr)
    }

    /// `text` (what follows an `!include`) with its variables and calls replaced by their values;
    /// `None` when one of them is unknown.
    pub fn substitute(&self, text: &str, folder: Option<&Path>) -> Option<String> {
        let locals = HashMap::new();
        let scope = self.scope(&locals, folder);
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
                let name = &rest[..length];
                let call = rest[length..].strip_prefix('(');
                if let Some(after) = call
                    && (sigil == 1 || self.functions.contains_key(name))
                {
                    let (arguments, used) = call_arguments(after)?;
                    out.push_str(&scope.call(name, &arguments)?);
                    at += length + 1 + used;
                    continue;
                }
                if rest.starts_with('$') && length > 1 {
                    out.push_str(scope.variable(name)?);
                    at += length;
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

    /// Reads the variables and functions `text` sets, its includes left alone.
    pub fn read(&mut self, text: &str, folder: Option<&Path>) {
        let mut body = Body::default();
        for line in text.lines() {
            body.read(line.trim(), self, folder);
        }
    }

    /// Learns what `line` (trimmed) sets: a variable (`!$x = ...`, `!$x ?= ...`, with `!global`)
    /// or a function on one line (`!function $f($a) !return $a`). Returns the function a line
    /// opens, whose body follows.
    fn learn(&mut self, line: &str, folder: Option<&Path>) -> Option<(String, Function)> {
        for word in ["!function", "!unquoted function"] {
            if let Some(rest) = directive(line, word) {
                let (head, one_line) = match rest.find("!return") {
                    Some(at) => (&rest[..at], Some(rest[at..].trim())),
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
                    body: one_line.map(str::to_owned).into_iter().collect(),
                };
                if one_line.is_some() {
                    self.functions.insert(name, function);
                    return None;
                }
                return Some((name, function));
            }
        }
        if directive(line, "!local").is_some() {
            return None;
        }
        let (name, expr) = assignment(line)?;
        if !line.starts_with('!') {
            return None;
        }
        let optional = line.contains("?=");
        if optional && self.variables.contains_key(&name) {
            return None;
        }
        if let Some(value) = self.evaluate(expr, &HashMap::new(), folder) {
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
            let ends = ["!endfunction", "!endprocedure", "!enddefinelong"]
                .iter()
                .any(|end| directive(line, end).is_some());
            if ends {
                if let Some(Some((name, function))) = self.0.take() {
                    preprocessor.functions.insert(name, function);
                }
            } else if let Some((_, function)) = open {
                function.body.push(line.to_owned());
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
