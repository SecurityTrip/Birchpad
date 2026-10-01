//! A command call: id plus arguments.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A command id with its arguments, e.g. `encoding.convert-to { "encoding": "utf-8" }`.
///
/// Invocations are plain data: they are stored in keymaps and menus, and will be recorded in
/// macros and sent by plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invocation {
    pub command: String,
    /// `null` for commands without arguments.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
}

/// The arguments of an invocation do not match what the command expects.
#[derive(Debug, thiserror::Error)]
#[error("invalid arguments for {command}: {source}")]
pub struct ArgsError {
    pub command: String,
    #[source]
    pub source: serde_json::Error,
}

impl Invocation {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Value::Null,
        }
    }

    /// An invocation with arguments.
    ///
    /// # Panics
    ///
    /// Panics if `args` cannot be represented as JSON, which only happens for maps with
    /// non-string keys.
    pub fn with_args(command: impl Into<String>, args: impl Serialize) -> Self {
        Self {
            command: command.into(),
            args: serde_json::to_value(args).expect("command arguments serialize to JSON"),
        }
    }

    /// Decodes the arguments. Missing arguments (`null`) are read as an empty object, so argument
    /// structs whose fields all have defaults accept a bare invocation.
    pub fn args<T: DeserializeOwned>(&self) -> Result<T, ArgsError> {
        let attempt = serde_json::from_value(self.args.clone());
        match attempt {
            Err(_) if self.args.is_null() => {
                serde_json::from_value(Value::Object(Default::default()))
            }
            result => result,
        }
        .map_err(|source| ArgsError {
            command: self.command.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Eol {
        eol: String,
    }

    #[derive(Debug, Deserialize, PartialEq, Default)]
    #[serde(default)]
    struct Optional {
        count: u32,
    }

    #[test]
    fn round_trips_through_json() {
        let invocation = Invocation::with_args("edit.convert-eol", Eol { eol: "lf".into() });
        let json = serde_json::to_string(&invocation).unwrap();
        assert_eq!(
            json,
            r#"{"command":"edit.convert-eol","args":{"eol":"lf"}}"#
        );
        let back: Invocation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, invocation);
        assert_eq!(back.args::<Eol>().unwrap().eol, "lf");

        let bare: Invocation = serde_json::from_str(r#"{"command":"file.new"}"#).unwrap();
        assert_eq!(bare, Invocation::new("file.new"));
    }

    #[test]
    fn missing_args_use_defaults_but_wrong_args_fail() {
        assert!(Invocation::new("file.new").args::<()>().is_ok());
        assert_eq!(
            Invocation::new("x.y").args::<Optional>().unwrap(),
            Optional::default()
        );
        let error = Invocation::new("edit.convert-eol")
            .args::<Eol>()
            .unwrap_err();
        assert_eq!(error.command, "edit.convert-eol");
        let wrong = Invocation::with_args("edit.convert-eol", serde_json::json!({ "eol": 1 }));
        assert!(wrong.args::<Eol>().is_err());
    }
}
