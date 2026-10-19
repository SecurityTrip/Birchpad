//! Command-line arguments, compatible with Notepad++ where that makes sense.
//!
//! ```text
//! birchpad [options] [file]...
//!   -n<line>      go to line (1-based)
//!   -c<column>    go to column (1-based)
//!   -p<position>  go to position (0-based byte offset); wins over -n/-c
//!   -multiInst    start a separate instance instead of handing files to a running one
//!   -nosession    do not restore or save a session
//!   -openSession  the files are sessions to open
//!   -ro           open the files read-only
//!   -l<language>  language for syntax highlighting (`-lcpp`, `-lpython`, `-lnormal`)
//!   --            everything after is a file name, even if it starts with "-"
//! ```
//!
//! Options are case-insensitive. Unknown options produce a warning and are ignored, so
//! Notepad++ shortcuts and scripts that pass extra options keep working.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A parsed command line. Files are absolute, so the line can be handed to another process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandLine {
    pub files: Vec<PathBuf>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub position: Option<usize>,
    pub multi_instance: bool,
    pub no_session: bool,
    /// `-openSession`: the files are session files (Birchpad's or Notepad++'s) to open.
    #[serde(default)]
    pub open_session: bool,
    pub read_only: bool,
    pub language: Option<String>,
    /// `--generate N`: open N lines of generated text (for development and benchmarks).
    #[serde(default)]
    pub generate: Option<usize>,
    /// Problems with the arguments; they are reported, not fatal.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

/// Where to put the caret in the files opened from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretTarget {
    /// 1-based line and column.
    LineColumn { line: usize, column: usize },
    /// 0-based byte offset.
    Position(usize),
}

/// Options that Notepad++ accepts and Birchpad ignores (window placement, printing, ...).
const IGNORED_FLAGS: &[&str] = &[
    "-notabbar",
    "-systemtray",
    "-alwaysontop",
    "-loadingtime",
    "-r",
    "-quickprint",
    "-monitor",
    "-nopluginsupport",
    "-openfoldersasworkspace",
];

const IGNORED_PREFIXES: &[&str] = &[
    "-x",
    "-y",
    "-qn",
    "-qt",
    "-qf",
    "-qspeed",
    "-settingsdir=",
    "-titleadd=",
    "-udl=",
    "-pluginmessage=",
];

impl CommandLine {
    /// Parses arguments (without the program name). Relative file names are resolved against
    /// `cwd`.
    pub fn parse(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Self {
        let mut line = Self::default();
        let mut args = args.into_iter().peekable();
        let mut files_only = false;
        while let Some(arg) = args.next() {
            let text = arg.to_string_lossy();
            if files_only || !text.starts_with('-') || text == "-" {
                line.files.push(cwd.join(PathBuf::from(&arg)));
                continue;
            }
            if text == "--" {
                files_only = true;
                continue;
            }
            if text == "--generate" {
                let count = args
                    .next_if(|next| next.to_string_lossy().parse::<usize>().is_ok())
                    .and_then(|next| next.to_string_lossy().parse().ok())
                    .unwrap_or(1_000_000);
                line.generate = Some(count);
                continue;
            }
            line.option(&text);
        }
        line
    }

    fn option(&mut self, arg: &str) {
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "-multiinst" => self.multi_instance = true,
            "-nosession" => self.no_session = true,
            "-opensession" => self.open_session = true,
            "-ro" => self.read_only = true,
            flag if IGNORED_FLAGS.contains(&flag) => {}
            _ => self.prefixed_option(arg, &lower),
        }
    }

    fn prefixed_option(&mut self, arg: &str, lower: &str) {
        let number =
            |prefix: &str, slot: &mut Option<usize>, warnings: &mut Vec<String>| match lower
                [prefix.len()..]
                .parse::<usize>()
            {
                Ok(value) => *slot = Some(value),
                Err(_) => warnings.push(format!("{arg}: expected a number after {prefix}")),
            };
        if lower.starts_with("-n") {
            number("-n", &mut self.line, &mut self.warnings);
        } else if lower.starts_with("-c") {
            number("-c", &mut self.column, &mut self.warnings);
        } else if lower.starts_with("-p") && !lower.starts_with("-pluginmessage=") {
            number("-p", &mut self.position, &mut self.warnings);
        } else if lower.starts_with("-l") && !lower.starts_with("-loadingtime") {
            // The language for syntax highlighting.
            self.language = Some(arg[2..].to_owned());
        } else if IGNORED_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
        } else {
            self.warnings.push(format!("unknown option {arg} ignored"));
        }
    }

    /// Where the caret goes in the opened files, if the command line says.
    pub fn caret_target(&self) -> Option<CaretTarget> {
        match (self.position, self.line, self.column) {
            (Some(position), _, _) => Some(CaretTarget::Position(position)),
            (None, None, None) => None,
            (None, line, column) => Some(CaretTarget::LineColumn {
                line: line.unwrap_or(1).max(1),
                column: column.unwrap_or(1).max(1),
            }),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/args/tests.rs"]
mod tests;
