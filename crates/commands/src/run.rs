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

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> RunVariables {
        RunVariables {
            full_current_path: r"C:\work\notes.final.txt".into(),
            file_name: "notes.final.txt".into(),
            current_directory: r"C:\work".into(),
            current_word: "birch".into(),
            current_line: 12,
            current_column: 3,
            current_line_text: "a birch tree".into(),
            executable: r"C:\Birchpad\birchpad.exe".into(),
            executable_directory: r"C:\Birchpad".into(),
        }
    }

    fn expand(line: &str) -> String {
        expand_variables(line, &vars(), |name| {
            (name == "HOME").then(|| "/home/me".into())
        })
    }

    #[test]
    fn every_variable() {
        assert_eq!(expand("$(FULL_CURRENT_PATH)"), r"C:\work\notes.final.txt");
        assert_eq!(
            expand("$(CURRENT_DIRECTORY)|$(FILE_NAME)"),
            r"C:\work|notes.final.txt"
        );
        assert_eq!(expand("$(NAME_PART)$(EXT_PART)"), "notes.final.txt");
        assert_eq!(expand("[$(NAME_PART)][$(EXT_PART)]"), "[notes.final][.txt]");
        assert_eq!(
            expand("$(CURRENT_WORD) $(CURRENT_LINE):$(CURRENT_COLUMN)"),
            "birch 12:3"
        );
        assert_eq!(expand("$(CURRENT_LINESTR)"), "a birch tree");
        assert_eq!(
            expand("$(NPP_DIRECTORY) $(NPP_FULL_FILE_PATH)"),
            r"C:\Birchpad C:\Birchpad\birchpad.exe"
        );
        assert_eq!(expand("$(SYS.HOME)/x $(SYS.NOPE)."), "/home/me/x .");
        assert_eq!(expand("$(current_word)"), "birch");
    }

    #[test]
    fn what_is_not_a_variable_stays() {
        for line in [
            "",
            "plain",
            "$(UNKNOWN)",
            "$(",
            "$(CURRENT_WORD",
            "$",
            "($CURRENT_WORD)",
            "$()",
            "a $(x) b",
        ] {
            assert_eq!(expand(line), line, "{line:?}");
        }
        assert_eq!(expand("$(UNKNOWN)$(CURRENT_WORD)"), "$(UNKNOWN)birch");
        assert_eq!(expand("$($(CURRENT_WORD))"), "$($(CURRENT_WORD))");
        // Without an extension, or a name that starts with a dot.
        let mut vars = vars();
        vars.file_name = "Makefile".into();
        assert_eq!(
            expand_variables("$(NAME_PART)|$(EXT_PART)", &vars, |_| None),
            "Makefile|"
        );
        vars.file_name = ".bashrc".into();
        assert_eq!(
            expand_variables("$(NAME_PART)|$(EXT_PART)", &vars, |_| None),
            ".bashrc|"
        );
        vars.file_name = "д.тxt".into();
        assert_eq!(expand_variables("$(EXT_PART)", &vars, |_| None), ".тxt");
    }

    #[test]
    fn run_commands_round_trip() {
        assert!(
            default_run_commands().is_empty(),
            "the built-in ones are not saved"
        );
        let commands = builtin_run_commands();
        assert_eq!(commands.len(), 4);
        assert_eq!(
            parse_run_commands(&write_run_commands(&commands)).unwrap(),
            commands
        );
        assert_eq!(parse_run_commands("").unwrap(), vec![]);
        for bad in [
            "[[command]",
            "[[command]]\nline = 'x'\n",
            "[[command]]\nname = ''\nline = 'x'\n",
            "[[command]]\nname = 'a'\n",
            "[[command]]\nname = 'a'\nline = 'x'\n[[command]]\nname = 'a'\nline = 'y'\n",
        ] {
            assert!(parse_run_commands(bad).is_err(), "{bad}");
        }
    }
}
