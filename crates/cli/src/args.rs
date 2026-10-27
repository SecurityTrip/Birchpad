//! Command-line arguments, compatible with Notepad++.
//!
//! ```text
//! birchpad [options] [file]...
//!   -n<line>      go to line (1-based)
//!   -c<column>    go to column (1-based)
//!   -p<position>  go to position (0-based byte offset); wins over -n/-c
//!   -l<language>  language for syntax highlighting (`-lcpp`, `-lpython`, `-lnormal`)
//!   -udl="name"   the user defined language of that name
//!   -ro           open the files read-only
//!   -fullReadOnly every document read-only for the run, which cannot be changed
//!   -fullReadOnlySavingForbidden  and saving turned off too
//!   -monitor      open the files with Monitoring (tail -f) on
//!   -r            wildcards in file names match in the folders below too
//!   -multiInst    start a separate instance instead of handing files to a running one
//!   -nosession    do not restore or save a session
//!   -openSession  the files are sessions to open
//!   -openFoldersAsWorkspace  folders go to Folder as Workspace (as they always do)
//!   -notabbar     no tab bar
//!   -alwaysOnTop  the window stays above the others
//!   -systemtray   start in the system tray (minimized where there is none)
//!   -x<left> -y<top>  where the window opens, in pixels
//!   -titleAdd="text"  text after the window's title
//!   -settingsDir="folder"  settings and data in that folder
//!   -loadingTime  say how long the files took to open
//!   -quickPrint   print the files on the default printer and quit
//!   -qt="text" -qf="file" -qSpeed<1-3>  type the text (or the file's) into a new document
//!   -qn="name"    Notepad++'s easter eggs, which Birchpad does not have
//!   -noPlugin     start without plugins (`-nopluginsupport` too)
//!   -L<code>      the interface in that language this time (`-Lde`, `-Lru`)
//!   -notepadStyleCmdline  the file names after it are one name with spaces, as Notepad reads it
//!   -z <argument> the argument after it is ignored (Notepad's path, when Birchpad replaces it)
//!   --help        Help > Command Line Arguments... at start
//!   -pluginMessage="text"  a message for the plugins that ask for it (`on-message`)
//!   --            everything after is a file name, even if it starts with "-"
//! ```
//!
//! Options are case-insensitive, except `-L`, which `-l` would otherwise be. File names with `*` and `?` are wildcards, matched in their
//! folder (and below it, with `-r`). Unknown options produce a warning and are ignored, so
//! scripts that pass other options keep working.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A parsed command line. Files are absolute, so the line can be handed to another process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandLine {
    pub files: Vec<PathBuf>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub position: Option<usize>,
    pub multi_instance: bool,
    pub no_session: bool,
    /// `-openSession`: the files are session files (Birchpad's or Notepad++'s) to open.
    pub open_session: bool,
    pub read_only: bool,
    /// `-fullReadOnly`: every document read-only for the run; Read-Only cannot be turned off.
    pub full_read_only: bool,
    /// `-fullReadOnlySavingForbidden`: that, and no document can be saved.
    pub saving_forbidden: bool,
    pub language: Option<String>,
    /// `--generate N`: open N lines of generated text (for development and benchmarks).
    pub generate: Option<usize>,
    /// `-udl="name"`: the user defined language of the files.
    pub udl: Option<String>,
    /// `-monitor`: the files open with Monitoring (tail -f) on.
    pub monitor: bool,
    /// `-r`: wildcards match in the folders below too (applied while parsing).
    pub recursive: bool,
    /// `-openFoldersAsWorkspace`: folders go to Folder as Workspace, which Birchpad does with
    /// every folder anyway.
    pub open_folders_as_workspace: bool,
    /// `-notabbar`.
    pub no_tab_bar: bool,
    /// `-alwaysOnTop`.
    pub always_on_top: bool,
    /// `-systemtray`: the window starts in the system tray on Windows and macOS, minimized
    /// elsewhere.
    pub minimized: bool,
    /// `-x<left>` and `-y<top>`: where the window opens.
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// `-titleAdd="text"`: text after the window's title.
    pub title_add: Option<String>,
    /// `-settingsDir="folder"`: the folder of the settings and data instead of the usual one.
    pub settings_dir: Option<PathBuf>,
    /// `-loadingTime`: say how long the files took to open.
    pub loading_time: bool,
    /// `-quickPrint`: print the files on the default printer, and quit if started for that.
    pub quick_print: bool,
    /// Ghost typing (`-qt`, `-qf`, `-qn`, `-qSpeed`): text typed into a new document.
    pub ghost: Option<Ghost>,
    /// `-noPlugin` (or `-nopluginsupport`): no plugins this time.
    pub no_plugins: bool,
    /// `-L<code>`: the interface's language this time, whatever the settings say.
    pub interface_language: Option<String>,
    /// `--help`: Help > Command Line Arguments... once the window opens.
    pub help: bool,
    /// `-pluginMessage="text"`: a message for the plugins.
    pub plugin_message: Option<String>,
    /// Problems with the arguments; they are reported, not fatal.
    #[serde(skip)]
    pub warnings: Vec<String>,
}

/// What ghost typing types, and how fast.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ghost {
    /// `-qt="text"`.
    pub text: Option<String>,
    /// `-qf="file"`: the file's text.
    pub file: Option<PathBuf>,
    /// `-qn="name"`: one of Notepad++'s easter eggs, which are its own and not Birchpad's.
    pub name: Option<String>,
    /// `-qSpeed1` (slow) to `-qSpeed3` (at once); 2 by default.
    pub speed: u8,
}

/// Where to put the caret in the files opened from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretTarget {
    /// 1-based line and column.
    LineColumn { line: usize, column: usize },
    /// 0-based byte offset.
    Position(usize),
}

/// An option of the command line, as Help > Command Line Arguments... lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliOption {
    /// How it is written: `-n<line>`.
    pub syntax: &'static str,
    /// What it does, in English; the interface's language has it under `cli.<slug>`.
    pub description: &'static str,
}

const fn option(syntax: &'static str, description: &'static str) -> CliOption {
    CliOption {
        syntax,
        description,
    }
}

/// The options Birchpad takes, in the order Help > Command Line Arguments... lists them.
pub const OPTIONS: &[CliOption] = &[
    option("-n<line>", "Go to the line (from 1)"),
    option("-c<column>", "Go to the column (from 1)"),
    option(
        "-p<position>",
        "Go to the position (a byte offset from 0); wins over -n and -c",
    ),
    option(
        "-l<language>",
        "Highlight as the language (-lcpp, -lpython, -lnormal)",
    ),
    option(
        "-udl=<name>",
        "Highlight as the user defined language of that name",
    ),
    option("-ro", "Open the files read-only"),
    option(
        "-fullReadOnly",
        "Every document read-only for this run, which cannot be changed",
    ),
    option(
        "-fullReadOnlySavingForbidden",
        "Every document read-only, and nothing saved, for this run",
    ),
    option("-monitor", "Open the files with Monitoring (tail -f) on"),
    option(
        "-r",
        "Wildcards (*, ?) in file names match in the folders below too",
    ),
    option(
        "-multiInst",
        "Start a window of its own instead of handing the files to the running one",
    ),
    option("-nosession", "Neither restore nor save the session"),
    option("-openSession", "The files are sessions to open"),
    option(
        "-openFoldersAsWorkspace",
        "Folders go to Folder as Workspace, as they always do",
    ),
    option("-notabbar", "Hide the tab bar"),
    option("-alwaysOnTop", "Keep the window above the others"),
    option("-systemtray", "Start in the system tray"),
    option("-x<left>", "Open the window this many pixels from the left"),
    option("-y<top>", "Open the window this many pixels from the top"),
    option("-titleAdd=<text>", "Add the text to the window's title"),
    option(
        "-settingsDir=<folder>",
        "Keep the settings and the data in that folder",
    ),
    option("-loadingTime", "Say how long the files took to open"),
    option(
        "-quickPrint",
        "Print the files on the default printer, and quit if started for that",
    ),
    option("-qt=<text>", "Type the text into a new document"),
    option("-qf=<file>", "Type the file's text into a new document"),
    option(
        "-qSpeed<1-3>",
        "How fast -qt and -qf type: 1 slow, 2 fast, 3 at once",
    ),
    option("-noPlugin", "Start without plugins"),
    option(
        "-L<code>",
        "Show the interface in that language this time (-Lde, -Lru)",
    ),
    option(
        "-notepadStyleCmdline",
        "The file names after it are one name with spaces, as Notepad reads them",
    ),
    option(
        "-z",
        "Ignore the argument after it (Notepad's path, when Birchpad stands in for Notepad)",
    ),
    option("--help", "Show these options"),
    option(
        "--",
        "Everything after it is a file, even if it starts with a dash",
    ),
    option("file...", "The files to open"),
];

/// The value of an option written `-name=value` (or `-name="value"`, quotes kept by the
/// shell), `None` if `arg` is not that option.
fn value_of<'a>(arg: &'a str, lower: &str, name: &str) -> Option<&'a str> {
    if !lower.starts_with(name) {
        return None;
    }
    let value = &arg[name.len()..];
    Some(
        value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value),
    )
}

/// Whether `name` (a file's name) matches `pattern`, with `*` for any run of characters and
/// `?` for one; without regard to case on Windows, whose file names have none.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    let fold = |text: &str| -> Vec<char> {
        if cfg!(windows) {
            text.chars().flat_map(char::to_lowercase).collect()
        } else {
            text.chars().collect()
        }
    };
    let (pattern, name) = (fold(pattern), fold(name));
    // Matching with backtracking to the last `*`.
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(&ch) if ch == '?' || ch == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((star_p, star_n)) => {
                    p = star_p + 1;
                    n = star_n + 1;
                    star = Some((star_p, star_n + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|ch| *ch == '*')
}

/// The files in `dir` (and below it, if `recursive`) whose names match `pattern`, sorted.
fn matching_files(dir: &Path, pattern: &str, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if recursive {
                matching_files(&path, pattern, true, out);
            }
        } else if wildcard_match(pattern, &entry.file_name().to_string_lossy()) {
            out.push(path);
        }
    }
}

impl CommandLine {
    /// Parses arguments (without the program name). Relative file names are resolved against
    /// `cwd`, and wildcards in them are matched there.
    pub fn parse(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Self {
        let mut line = Self::parse_unexpanded(args, cwd);
        line.expand_wildcards();
        line
    }

    /// Parses arguments as [`parse`](Self::parse) does without looking at the file system:
    /// wildcards stay in the file names as written.
    pub fn parse_unexpanded(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Self {
        let mut line = Self::default();
        let mut args = args.into_iter().peekable();
        let mut files_only = false;
        // `-notepadStyleCmdline`: the words of the one file name, joined at the end.
        let mut notepad_style: Option<Vec<OsString>> = None;
        while let Some(arg) = args.next() {
            let text = arg.to_string_lossy();
            if files_only || !text.starts_with('-') || text == "-" {
                match &mut notepad_style {
                    Some(words) => words.push(arg),
                    None => line.files.push(cwd.join(PathBuf::from(&arg))),
                }
                continue;
            }
            if text == "--" {
                files_only = true;
                continue;
            }
            if text == "--help" {
                line.help = true;
                continue;
            }
            if text.eq_ignore_ascii_case("-z") {
                args.next();
                continue;
            }
            if text.eq_ignore_ascii_case("-notepadStyleCmdline") {
                notepad_style.get_or_insert_with(Vec::new);
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
            line.option(&text, cwd);
        }
        if let Some(words) = notepad_style.filter(|words| !words.is_empty()) {
            let mut name = OsString::new();
            for (index, word) in words.iter().enumerate() {
                if index > 0 {
                    name.push(" ");
                }
                name.push(word);
            }
            line.files.push(cwd.join(PathBuf::from(name)));
        }
        line
    }

    /// File names with `*` or `?` in their last part become the files they match.
    fn expand_wildcards(&mut self) {
        let files = std::mem::take(&mut self.files);
        for path in files {
            let pattern = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| name.contains(['*', '?']));
            let Some(pattern) = pattern else {
                self.files.push(path);
                continue;
            };
            let dir = path.parent().unwrap_or(Path::new("."));
            let before = self.files.len();
            matching_files(dir, &pattern, self.recursive, &mut self.files);
            if self.files.len() == before {
                self.warnings
                    .push(format!("no file matches {}", path.display()));
            }
        }
    }

    fn option(&mut self, arg: &str, cwd: &Path) {
        let lower = arg.to_ascii_lowercase();
        match lower.as_str() {
            "-multiinst" => self.multi_instance = true,
            "-nosession" => self.no_session = true,
            "-opensession" => self.open_session = true,
            "-ro" => self.read_only = true,
            "-fullreadonly" => self.full_read_only = true,
            "-fullreadonlysavingforbidden" => {
                self.full_read_only = true;
                self.saving_forbidden = true;
            }
            "-r" => self.recursive = true,
            "-monitor" => self.monitor = true,
            "-notabbar" => self.no_tab_bar = true,
            "-alwaysontop" => self.always_on_top = true,
            "-systemtray" => self.minimized = true,
            "-loadingtime" => self.loading_time = true,
            "-quickprint" => self.quick_print = true,
            "-openfoldersasworkspace" => self.open_folders_as_workspace = true,
            "-noplugin" | "-nopluginsupport" => self.no_plugins = true,
            _ => self.valued_option(arg, &lower, cwd),
        }
    }

    fn ghost(&mut self) -> &mut Ghost {
        self.ghost.get_or_insert_with(|| Ghost {
            speed: 2,
            ..Ghost::default()
        })
    }

    fn valued_option(&mut self, arg: &str, lower: &str, cwd: &Path) {
        if let Some(value) = value_of(arg, lower, "-udl=") {
            self.udl = Some(value.to_owned());
        } else if let Some(value) = value_of(arg, lower, "-titleadd=") {
            self.title_add = Some(value.to_owned());
        } else if let Some(value) = value_of(arg, lower, "-settingsdir=") {
            if value.is_empty() {
                self.warnings
                    .push(format!("{arg}: expected a folder after -settingsDir="));
            } else {
                self.settings_dir = Some(cwd.join(value));
            }
        } else if let Some(value) = value_of(arg, lower, "-pluginmessage=") {
            self.plugin_message = Some(value.to_owned());
        } else if let Some(value) = value_of(arg, lower, "-qt=") {
            self.ghost().text = Some(value.to_owned());
        } else if let Some(value) = value_of(arg, lower, "-qf=") {
            self.ghost().file = Some(cwd.join(value));
        } else if let Some(value) = value_of(arg, lower, "-qn=") {
            self.ghost().name = Some(value.to_owned());
        } else if let Some(speed) = lower.strip_prefix("-qspeed") {
            match speed.parse::<u8>() {
                Ok(speed @ 1..=3) => self.ghost().speed = speed,
                _ => self
                    .warnings
                    .push(format!("{arg}: expected 1, 2 or 3 after -qSpeed")),
            }
        } else {
            self.prefixed_option(arg, lower);
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
        let pixels = |prefix: &str, slot: &mut Option<i32>, warnings: &mut Vec<String>| match lower
            [prefix.len()..]
            .parse::<i32>()
        {
            Ok(value) => *slot = Some(value),
            Err(_) => warnings.push(format!("{arg}: expected a number after {prefix}")),
        };
        if let Some(code) = arg.strip_prefix("-L") {
            if code.is_empty() {
                self.warnings
                    .push(format!("{arg}: expected a language code after -L"));
            } else {
                self.interface_language = Some(code.to_owned());
            }
        } else if lower.starts_with("-n") {
            number("-n", &mut self.line, &mut self.warnings);
        } else if lower.starts_with("-c") {
            number("-c", &mut self.column, &mut self.warnings);
        } else if lower.starts_with("-p") {
            number("-p", &mut self.position, &mut self.warnings);
        } else if lower.starts_with("-x") {
            pixels("-x", &mut self.x, &mut self.warnings);
        } else if lower.starts_with("-y") {
            pixels("-y", &mut self.y, &mut self.warnings);
        } else if lower.starts_with("-l") {
            // The language for syntax highlighting.
            self.language = Some(arg[2..].to_owned());
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
