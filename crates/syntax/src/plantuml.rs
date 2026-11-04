//! What Birchpad reads of PlantUML's text outside its grammar: the files a line includes, for
//! the preview, which follows them, and for Ctrl+click, which opens them.

use std::ops::Range;

/// The directives that include a file by its path, longest first.
const INCLUDES: [&str; 5] = [
    "!include_many",
    "!include_once",
    "!includesub",
    "!include",
    "!import",
];

/// Where in `line` the file it includes is named: after `!include`, `!include_many`,
/// `!include_once`, `!includesub` or `!import`, without the quotes around it or a `!part` after
/// it. Files of the standard library (`<C4/C4_Context>`), URLs and paths made of variables are
/// no file to open: `None`, as for a line that includes nothing.
pub fn include_target(line: &str) -> Option<Range<usize>> {
    let range = include_argument(line)?;
    let written = &line[range.clone()];
    let (start, target) = match written.strip_prefix('"') {
        Some(quoted) => (
            range.start + 1,
            quoted.split('"').next().unwrap_or_default(),
        ),
        // `common.puml!2` and `parts.puml!PART`: the file without the part.
        None => (
            range.start,
            written.split('!').next().unwrap_or_default().trim_end(),
        ),
    };
    let skipped = target.is_empty() || target.contains('$');
    (!skipped).then(|| start..start + target.len())
}

/// Where in `line` what follows an include directive is, as written: a path, or one made of
/// variables and calls (`$HOME/main.iuml`, `FromRoot(/billing/containers.puml)`) for the preprocessor
/// to work out. Files of the standard library and URLs are left out, as is an empty one.
pub fn include_argument(line: &str) -> Option<Range<usize>> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    let directive = INCLUDES.iter().find(|directive| {
        trimmed
            .get(..directive.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(directive))
            && trimmed[directive.len()..].starts_with([' ', '\t'])
    })?;
    let after = indent + directive.len();
    let rest = &line[after..];
    let start = after + (rest.len() - rest.trim_start().len());
    let written = line[start..].trim_end();
    let unquoted = written.trim_start_matches('"');
    let skipped = written.is_empty()
        || unquoted.is_empty()
        || unquoted.starts_with('<')
        || written.contains("://");
    (!skipped).then(|| start..start + written.len())
}

/// The files the lines of `text` include, as written ([`include_target`]).
pub fn include_targets(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| include_target(line).map(|range| &line[range]))
        .collect()
}
