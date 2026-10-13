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

#[cfg(test)]
mod tests {
    #[test]
    fn searches_are_recorded_their_own_way() {
        for command in [
            "search.find-next",
            "search.find-previous",
            "search.find-text",
            "search.replace-text",
            "macro.trim-and-save",
            "file.exit",
        ] {
            assert!(!super::is_recordable(command), "{command}");
        }
        assert!(super::is_recordable("search.select-and-find-next"));
        assert!(super::is_recordable("edit.trim"));
    }

    use super::*;
    use serde_json::json;

    fn typed(text: &str) -> Invocation {
        Invocation::with_args("edit.type", json!({ "text": text }))
    }

    #[test]
    fn typing_joins_into_one_step() {
        let mut steps = Vec::new();
        record(&mut steps, typed("a"));
        record(&mut steps, typed("bc"));
        record(&mut steps, Invocation::new("edit.newline"));
        record(&mut steps, typed("d"));
        assert_eq!(
            steps,
            [typed("abc"), Invocation::new("edit.newline"), typed("d")]
        );
        // A first step, and typed text without text, stay as they are.
        let mut steps = Vec::new();
        record(&mut steps, Invocation::new("edit.type"));
        record(&mut steps, typed("x"));
        assert_eq!(steps, [Invocation::new("edit.type"), typed("x")]);
    }

    #[test]
    fn what_is_recorded() {
        for command in ["edit.type", "file.save", "search.find-next", "edit.undo"] {
            assert!(is_recordable(command), "{command}");
        }
        for command in [
            "macro.playback",
            "macro.start-recording",
            "settings.preferences",
            "help.about",
            "file.exit",
        ] {
            assert!(!is_recordable(command), "{command}");
        }
    }

    #[test]
    fn macros_round_trip() {
        let macros = vec![
            Macro {
                name: "Quote".into(),
                steps: vec![typed("\"x\\n\""), Invocation::new("edit.newline")],
            },
            Macro {
                name: "Empty".into(),
                steps: vec![],
            },
            Macro {
                name: "Case".into(),
                steps: vec![Invocation::with_args(
                    "edit.convert-case",
                    json!({ "to": "upper", "n": 3, "flag": true, "list": [1, 2] }),
                )],
            },
        ];
        let text = write_macros(&macros);
        assert_eq!(parse_macros(&text).unwrap(), macros, "{text}");
        assert_eq!(parse_macros("").unwrap(), vec![]);
        assert_eq!(parse_macros(&write_macros(&[])).unwrap(), vec![]);
    }

    #[test]
    fn refuses_bad_files() {
        for text in [
            "[[macro]",
            "macro = 3",
            "[[macro]]\nsteps = []\n",
            "[[macro]]\nname = ' '\nsteps = []\n",
            "[[macro]]\nname = 'a'\n",
            "[[macro]]\nname = 'a'\nsteps = [{ args = {} }]\n",
            "[[macro]]\nname = 'a'\nsteps = [{ command = 'no.such' }]\n",
            "[[macro]]\nname = 'a'\nsteps = []\n[[macro]]\nname = 'a'\nsteps = []\n",
        ] {
            assert!(parse_macros(text).is_err(), "{text}");
        }
    }
}
