//! The Run menu's commands: command lines with Notepad++'s variables, saved by name in
//! `run.toml`.
//!
//! ```toml
//! [[command]]
//! name = "Wikipedia Search"
//! line = "https://en.wikipedia.org/wiki/Special:Search?search=$(CURRENT_WORD)"
//! ```

use serde::{Deserialize, Serialize};

use crate::KeymapError;

/// What the variables of a command line stand for, from the active document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunVariables {
    /// `$(FULL_CURRENT_PATH)`: the document's file; empty for an untitled one.
    pub full_current_path: String,
    /// The document's name (`new 1` for an untitled one).
    pub file_name: String,
    /// The folder of the document.
    pub current_directory: String,
    /// The word at the caret, or the selection on one line.
    pub current_word: String,
    /// The caret's line and column, from 1.
    pub current_line: usize,
    pub current_column: usize,
    /// The text of the caret's line, without its line break.
    pub current_line_text: String,
    /// The running executable, and its folder.
    pub executable: String,
    pub executable_directory: String,
}

/// Replaces Notepad++'s variables in `line`: `$(FULL_CURRENT_PATH)`, `$(CURRENT_DIRECTORY)`,
/// `$(FILE_NAME)`, `$(NAME_PART)`, `$(EXT_PART)`, `$(CURRENT_WORD)`, `$(CURRENT_LINE)`,
/// `$(CURRENT_COLUMN)`, `$(CURRENT_LINESTR)`, `$(NPP_DIRECTORY)`, `$(NPP_FULL_FILE_PATH)` and
/// `$(SYS.NAME)` for the environment variable `NAME`. Names are matched ignoring case; others are
/// left as they are.
pub fn expand_variables(
    line: &str,
    vars: &RunVariables,
    env: impl Fn(&str) -> Option<String>,
) -> String {
    let (name_part, ext_part) = match vars.file_name.rfind('.') {
        Some(dot) if dot > 0 => (&vars.file_name[..dot], &vars.file_name[dot..]),
        _ => (vars.file_name.as_str(), ""),
    };
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find("$(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let value = match name.to_ascii_uppercase().as_str() {
            "FULL_CURRENT_PATH" => Some(vars.full_current_path.clone()),
            "CURRENT_DIRECTORY" => Some(vars.current_directory.clone()),
            "FILE_NAME" => Some(vars.file_name.clone()),
            "NAME_PART" => Some(name_part.to_owned()),
            "EXT_PART" => Some(ext_part.to_owned()),
            "CURRENT_WORD" => Some(vars.current_word.clone()),
            "CURRENT_LINE" => Some(vars.current_line.to_string()),
            "CURRENT_COLUMN" => Some(vars.current_column.to_string()),
            "CURRENT_LINESTR" => Some(vars.current_line_text.clone()),
            "NPP_DIRECTORY" => Some(vars.executable_directory.clone()),
            "NPP_FULL_FILE_PATH" => Some(vars.executable.clone()),
            upper => upper
                .strip_prefix("SYS.")
                .map(|_| env(&name[4..]).unwrap_or_default()),
        };
        match value {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A saved command of the Run menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEntry {
    pub name: String,
    pub line: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RunFile {
    #[serde(default, rename = "command")]
    commands: Vec<RunEntry>,
}

/// The commands a new `run.toml` starts with, as Notepad++'s Run menu has.
pub fn default_run_commands() -> Vec<RunEntry> {
    Vec::new()
}

/// Notepad++'s own Run menu entries: always there, before the saved ones (`run.builtin`).
pub fn builtin_run_commands() -> Vec<RunEntry> {
    let entry = |name: &str, line: &str| RunEntry {
        name: name.to_owned(),
        line: line.to_owned(),
    };
    vec![
        entry("Get PHP Help", "https://www.php.net/$(CURRENT_WORD)"),
        entry(
            "Google Search",
            "https://www.google.com/search?q=$(CURRENT_WORD)",
        ),
        entry(
            "Wikipedia Search",
            "https://en.wikipedia.org/wiki/Special:Search?search=$(CURRENT_WORD)",
        ),
        entry(
            "Open File in Another Instance",
            "\"$(NPP_FULL_FILE_PATH)\" \"$(FULL_CURRENT_PATH)\" -multiInst -nosession",
        ),
    ]
}

/// Reads `run.toml`; two commands of one name, or a nameless one, are refused.
pub fn parse_run_commands(text: &str) -> Result<Vec<RunEntry>, KeymapError> {
    let file: RunFile =
        toml::from_str(text).map_err(|error| KeymapError(error.message().to_owned()))?;
    for (index, command) in file.commands.iter().enumerate() {
        if command.name.trim().is_empty() {
            return Err(KeymapError(format!("command {} has no name", index + 1)));
        }
        if file.commands[..index]
            .iter()
            .any(|other| other.name == command.name)
        {
            return Err(KeymapError(format!(
                "two commands are named {:?}",
                command.name
            )));
        }
    }
    Ok(file.commands)
}

/// The text of `run.toml` for `commands`.
pub fn write_run_commands(commands: &[RunEntry]) -> String {
    toml::to_string(&RunFile {
        commands: commands.to_vec(),
    })
    .expect("run commands serialize")
}

/// How a command line runs: through `cmd /C` on Windows, `sh -c` elsewhere.
pub fn shell_command(line: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let mut command = std::process::Command::new("cmd");
        // As typed: cmd reads its own quotes, which the usual escaping of arguments breaks.
        command.arg("/C").raw_arg(line);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", line]);
        command
    }
}

/// Whether a command line is a web address, opened in the browser.
pub fn is_url(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}
