//! Command-line arguments, compatible with Notepad++ where that makes sense.
//!
//! ```text
//! birchpad [options] [file]...
//!   -n<line>      go to line (1-based)
//!   -c<column>    go to column (1-based)
//!   -p<position>  go to position (0-based byte offset); wins over -n/-c
//!   -multiInst    start a separate instance instead of handing files to a running one
//!   -nosession    do not restore or save a session
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
    "-opensession",
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
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CommandLine {
        CommandLine::parse(args.iter().map(OsString::from), Path::new("/work"))
    }

    #[test]
    fn files_are_made_absolute() {
        let line = parse(&["a.txt", "/abs/b.txt", "sub dir/with spaces.txt"]);
        assert_eq!(
            line.files,
            [
                PathBuf::from("/work/a.txt"),
                PathBuf::from("/abs/b.txt"),
                PathBuf::from("/work/sub dir/with spaces.txt"),
            ]
        );
        assert!(line.warnings.is_empty());
    }

    #[test]
    fn notepad_plus_plus_options() {
        let line = parse(&[
            "-n120",
            "-c5",
            "-multiInst",
            "-nosession",
            "-ro",
            "-lcpp",
            "f.txt",
        ]);
        assert_eq!(line.line, Some(120));
        assert_eq!(line.column, Some(5));
        assert!(line.multi_instance && line.no_session && line.read_only);
        assert_eq!(line.language.as_deref(), Some("cpp"));
        assert_eq!(line.files, [PathBuf::from("/work/f.txt")]);
        assert_eq!(
            line.caret_target(),
            Some(CaretTarget::LineColumn {
                line: 120,
                column: 5
            })
        );
    }

    #[test]
    fn options_are_case_insensitive_and_position_wins() {
        let line = parse(&["-MULTIINST", "-N7", "-p42"]);
        assert!(line.multi_instance);
        assert_eq!(line.caret_target(), Some(CaretTarget::Position(42)));
        assert_eq!(
            parse(&["-c3"]).caret_target(),
            Some(CaretTarget::LineColumn { line: 1, column: 3 })
        );
        assert_eq!(parse(&["x"]).caret_target(), None);
    }

    #[test]
    fn unknown_and_malformed_options_warn() {
        let line = parse(&[
            "-nfoo",
            "-frobnicate",
            "-notabbar",
            "-x100",
            "-settingsDir=C:\\cfg",
        ]);
        assert_eq!(line.warnings.len(), 2, "{:?}", line.warnings);
        assert_eq!(line.line, None);
    }

    #[test]
    fn double_dash_ends_options() {
        let line = parse(&["-ro", "--", "-n5", "-"]);
        assert!(line.read_only);
        assert_eq!(line.line, None);
        assert_eq!(
            line.files,
            [PathBuf::from("/work/-n5"), PathBuf::from("/work/-")]
        );
    }

    #[test]
    fn generate_takes_an_optional_count() {
        assert_eq!(parse(&["--generate", "500"]).generate, Some(500));
        let line = parse(&["--generate", "file.txt"]);
        assert_eq!(line.generate, Some(1_000_000));
        assert_eq!(line.files.len(), 1);
    }

    #[test]
    fn round_trips_through_json() {
        let line = parse(&["-n3", "-ro", "a.txt"]);
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(serde_json::from_str::<CommandLine>(&json).unwrap(), line);
    }
}
