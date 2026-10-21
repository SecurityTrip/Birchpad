//! Macros: recorded sequences of command invocations, kept by name in `macros.toml`.
//!
//! ```toml
//! [[macro]]
//! name = "Quote the word"
//! steps = [
//!     { command = "edit.type", args = { text = "\"" } },
//!     { command = "cursor.word-right" },
//! ]
//! ```

use serde::{Deserialize, Serialize};

use crate::{Invocation, KeymapError};

/// A named macro.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<Invocation>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MacroFile {
    #[serde(default, rename = "macro")]
    macros: Vec<Macro>,
}

/// Commands a macro does not record: the macro commands themselves, those that only open
/// dialogs about the application or leave it, and the searches, which are recorded with their
/// text and options instead (`search.find-text`, `search.replace-text`).
pub fn is_recordable(command: &str) -> bool {
    !(command.starts_with("macro.")
        || command.starts_with("settings.")
        || command.starts_with("help.")
        || matches!(
            command,
            "file.exit"
                | "search.find-next"
                | "search.find-previous"
                | "search.find-text"
                | "search.replace-text"
        ))
}

/// Adds `step` to a recording. Typed text joins the text typed just before, so that a word is
/// one step rather than one per letter.
pub fn record(steps: &mut Vec<Invocation>, step: Invocation) {
    if step.command == "edit.type"
        && let Some(last) = steps.last_mut()
        && last.command == "edit.type"
        && let (Some(before), Some(typed)) = (
            last.args.get("text").and_then(|text| text.as_str()),
            step.args.get("text").and_then(|text| text.as_str()),
        )
    {
        let joined = format!("{before}{typed}");
        last.args = serde_json::json!({ "text": joined });
        return;
    }
    steps.push(step);
}

/// Reads `macros.toml`. Macros with a name used before, an empty name or an unknown command
/// are refused.
pub fn parse_macros(text: &str) -> Result<Vec<Macro>, KeymapError> {
    let file: MacroFile =
        toml::from_str(text).map_err(|error| KeymapError(error.message().to_owned()))?;
    for (index, found) in file.macros.iter().enumerate() {
        if found.name.trim().is_empty() {
            return Err(KeymapError(format!("macro {} has no name", index + 1)));
        }
        if file.macros[..index]
            .iter()
            .any(|other| other.name == found.name)
        {
            return Err(KeymapError(format!(
                "two macros are named {:?}",
                found.name
            )));
        }
        if let Some(step) = found
            .steps
            .iter()
            .find(|step| crate::catalog::find(&step.command).is_none())
        {
            return Err(KeymapError(format!(
                "macro {:?} runs the unknown command `{}`",
                found.name, step.command
            )));
        }
    }
    Ok(file.macros)
}

/// The text of `macros.toml` for `macros`, which [`parse_macros`] reads back.
pub fn write_macros(macros: &[Macro]) -> String {
    let file = MacroFile {
        macros: macros.to_vec(),
    };
    toml::to_string(&file).expect("macros serialize")
}
