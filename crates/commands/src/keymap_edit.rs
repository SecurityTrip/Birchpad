//! Changes to the user's `keymap.toml`, as the Shortcut Mapper makes them. The file's other
//! bindings, its comments and its layout stay as they are; bindings for other platforms are
//! never touched.

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use crate::keymap::{Keymap, KeymapError};
use crate::{Invocation, Keystroke, Platform};

/// The bindings of a keymap file, made if missing.
fn bindings(document: &mut DocumentMut) -> Result<&mut ArrayOfTables, KeymapError> {
    if document.get("binding").is_none() {
        document.insert("binding", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    document["binding"]
        .as_array_of_tables_mut()
        .ok_or_else(|| KeymapError("`binding` is not a list of [[binding]] tables".into()))
}

fn parse(text: &str) -> Result<DocumentMut, KeymapError> {
    text.parse()
        .map_err(|error: toml_edit::TomlError| KeymapError(error.message().to_owned()))
}

fn parse_keys(keys: &str, platform: Platform) -> Result<Vec<Keystroke>, KeymapError> {
    Keystroke::parse_sequence(keys, platform).map_err(|error| KeymapError(error.to_string()))
}

/// Whether a `[[binding]]` of the file applies on `platform` to `keys` in `context`.
fn is_for(table: &Table, platform: Platform, keys: &[Keystroke], context: Option<&str>) -> bool {
    let applies = table
        .get("platforms")
        .and_then(Item::as_array)
        .is_none_or(|platforms| {
            platforms
                .iter()
                .any(|name| name.as_str() == Some(platform.name()))
        });
    let same_keys = table
        .get("keys")
        .and_then(Item::as_str)
        .and_then(|text| Keystroke::parse_sequence(text, platform).ok())
        .is_some_and(|parsed| parsed == keys);
    let same_context = table.get("context").and_then(Item::as_str) == context;
    applies && same_keys && same_context
}

/// Whether a `[[binding]]` of the file runs `invocation` on `platform`.
fn runs(table: &Table, platform: Platform, invocation: &Invocation) -> bool {
    let applies = table
        .get("platforms")
        .and_then(Item::as_array)
        .is_none_or(|platforms| {
            platforms
                .iter()
                .any(|name| name.as_str() == Some(platform.name()))
        });
    let command = table.get("command").and_then(Item::as_str);
    let args = table
        .get("args")
        .map(|args| {
            let text = format!("args = {}", args.to_string().trim());
            toml::from_str::<toml::Table>(&text)
                .ok()
                .and_then(|table| serde_json::to_value(&table["args"]).ok())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    applies && command == Some(invocation.command.as_str()) && args == invocation.args
}

fn new_table(keys: &[Keystroke], context: Option<&str>) -> Table {
    let mut table = Table::new();
    let keys: Vec<String> = keys.iter().map(Keystroke::to_string).collect();
    table.insert("keys", value(keys.join(" ")));
    if let Some(context) = context {
        table.insert("context", value(context));
    }
    table
}

/// Binds `keys` in `context` to `invocation`, replacing what the file bound them to before.
pub fn bind(
    text: &str,
    platform: Platform,
    keys: &str,
    context: Option<&str>,
    invocation: &Invocation,
) -> Result<String, KeymapError> {
    let keys = parse_keys(keys, platform)?;
    if crate::catalog::find(&invocation.command).is_none() {
        return Err(KeymapError(format!(
            "unknown command `{}`",
            invocation.command
        )));
    }
    let mut document = parse(text)?;
    let list = bindings(&mut document)?;
    list.retain(|table| !is_for(table, platform, &keys, context));
    let mut table = new_table(&keys, context);
    table.insert("command", value(invocation.command.as_str()));
    if !invocation.args.is_null() {
        let args: toml::Value = serde_json::from_value(invocation.args.clone())
            .map_err(|error| KeymapError(format!("arguments cannot be written: {error}")))?;
        let args: toml_edit::Value = args
            .to_string()
            .parse()
            .map_err(|_| KeymapError("arguments cannot be written".into()))?;
        table.insert("args", value(args));
    }
    list.push(table);
    Ok(document.to_string())
}

/// Takes `keys` in `context` off whatever they run: the file's own binding is removed, and a
/// default binding is unbound.
pub fn unbind(
    text: &str,
    platform: Platform,
    keys: &str,
    context: Option<&str>,
) -> Result<String, KeymapError> {
    let keys = parse_keys(keys, platform)?;
    let mut document = parse(text)?;
    let list = bindings(&mut document)?;
    list.retain(|table| !is_for(table, platform, &keys, context));
    let defaults = Keymap::with_defaults(platform);
    let bound_by_default = defaults
        .bindings()
        .iter()
        .any(|binding| binding.keys == keys && binding.context.as_deref() == context);
    if bound_by_default {
        let mut table = new_table(&keys, context);
        table.insert("unbind", value(true));
        list.push(table);
    }
    if list.is_empty() {
        document.remove("binding");
    }
    Ok(document.to_string())
}

/// Gives `invocation` its default keys back: the file's bindings of it are removed, and so are
/// its unbound default keys.
pub fn reset(
    text: &str,
    platform: Platform,
    invocation: &Invocation,
) -> Result<String, KeymapError> {
    let mut document = parse(text)?;
    let defaults = Keymap::with_defaults(platform);
    let default_keys: Vec<(&[Keystroke], Option<&str>)> = defaults
        .bindings_for(invocation)
        .map(|binding| (binding.keys.as_slice(), binding.context.as_deref()))
        .collect();
    let list = bindings(&mut document)?;
    list.retain(|table| {
        let unbinds_default = table.get("unbind").and_then(Item::as_bool) == Some(true)
            && default_keys
                .iter()
                .any(|(keys, context)| is_for(table, platform, keys, *context));
        !runs(table, platform, invocation) && !unbinds_default
    });
    if list.is_empty() {
        document.remove("binding");
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Layer;
    use serde_json::json;

    const P: Platform = Platform::Windows;

    fn effective(text: &str) -> Keymap {
        let mut keymap = Keymap::with_defaults(P);
        keymap.add_layer(Layer::User, text);
        assert!(
            keymap.diagnostics().is_empty(),
            "{:?}\n{text}",
            keymap.diagnostics()
        );
        keymap
    }

    fn keys_of(keymap: &Keymap, invocation: &Invocation) -> Vec<String> {
        keymap
            .bindings_for(invocation)
            .map(crate::Binding::keys_string)
            .collect()
    }

    #[test]
    fn binds_keys_keeping_the_file() {
        let text = "# my keys\n[[binding]]\nkeys = \"ctrl-alt-x\"\ncommand = \"file.new\" # mine\n";
        let save = Invocation::new("file.save");
        let edited = bind(text, P, "ctrl-shift-k", None, &save).unwrap();
        assert!(edited.starts_with(text), "{edited}");
        let keymap = effective(&edited);
        assert!(keys_of(&keymap, &save).contains(&"ctrl-shift-k".to_owned()));
        // Binding the same keys again replaces the file's binding rather than adding one.
        let edited = bind(
            &edited,
            P,
            "ctrl-shift-k",
            None,
            &Invocation::new("file.close"),
        )
        .unwrap();
        assert_eq!(edited.matches("ctrl-shift-k").count(), 1, "{edited}");
        let keymap = effective(&edited);
        assert!(!keys_of(&keymap, &save).contains(&"ctrl-shift-k".to_owned()));
        // A context, and arguments.
        let case = Invocation::with_args("edit.convert-case", json!({ "to": "upper" }));
        let edited = bind("", P, "ctrl-alt-u", Some("Editor"), &case).unwrap();
        let keymap = effective(&edited);
        let binding = keymap
            .bindings_for(&case)
            .find(|b| b.layer == Layer::User)
            .unwrap();
        assert_eq!(binding.context.as_deref(), Some("Editor"));
    }

    #[test]
    fn unbinds_file_and_default_keys() {
        let new = Invocation::new("file.new");
        let edited = unbind("", P, "ctrl-n", None).unwrap();
        assert!(edited.contains("unbind = true"), "{edited}");
        assert!(keys_of(&effective(&edited), &new).is_empty());
        // Keys only the file binds are simply removed from it.
        let bound = bind("", P, "ctrl-alt-q", None, &new).unwrap();
        let edited = unbind(&bound, P, "ctrl-alt-q", None).unwrap();
        assert_eq!(edited, "");
        // Keys nothing binds change nothing.
        assert_eq!(unbind("", P, "ctrl-alt-shift-f12", None).unwrap(), "");
    }

    #[test]
    fn reset_gives_the_default_keys_back() {
        let new = Invocation::new("file.new");
        let text = unbind("", P, "ctrl-n", None).unwrap();
        let text = bind(&text, P, "ctrl-alt-q", None, &new).unwrap();
        let text = bind(&text, P, "ctrl-alt-w", None, &Invocation::new("file.close")).unwrap();
        let edited = reset(&text, P, &new).unwrap();
        let keymap = effective(&edited);
        assert_eq!(
            keys_of(&keymap, &new),
            keys_of(&Keymap::with_defaults(P), &new)
        );
        // Other commands keep their keys.
        assert!(
            keys_of(&keymap, &Invocation::new("file.close")).contains(&"ctrl-alt-w".to_owned())
        );
        // Arguments tell invocations apart.
        let upper = Invocation::with_args("edit.convert-case", json!({ "to": "upper" }));
        let lower = Invocation::with_args("edit.convert-case", json!({ "to": "lower" }));
        let text = bind("", P, "ctrl-alt-1", None, &upper).unwrap();
        let text = bind(&text, P, "ctrl-alt-2", None, &lower).unwrap();
        let edited = reset(&text, P, &upper).unwrap();
        assert!(
            !edited.contains("ctrl-alt-1") && edited.contains("ctrl-alt-2"),
            "{edited}"
        );
        // Nothing to reset.
        assert_eq!(reset("", P, &new).unwrap(), "");
    }

    #[test]
    fn other_platforms_are_left_alone() {
        let text =
            "[[binding]]\nkeys = \"ctrl-alt-x\"\ncommand = \"file.new\"\nplatforms = [\"macos\"]\n";
        assert_eq!(unbind(text, P, "ctrl-alt-x", None).unwrap(), text);
        assert_eq!(reset(text, P, &Invocation::new("file.new")).unwrap(), text);
        let edited = bind(text, P, "ctrl-alt-x", None, &Invocation::new("file.save")).unwrap();
        assert!(edited.starts_with(text), "{edited}");
    }

    #[test]
    fn refuses_what_it_cannot_write() {
        let new = Invocation::new("file.new");
        for keys in ["", "ctrl-", "ctrl-nosuchkey-x", "hyper-a"] {
            assert!(bind("", P, keys, None, &new).is_err(), "{keys:?}");
            assert!(unbind("", P, keys, None).is_err(), "{keys:?}");
        }
        assert!(bind("", P, "ctrl-alt-x", None, &Invocation::new("no.such")).is_err());
        for text in ["[[binding]", "binding = 3", "binding = [1]"] {
            assert!(bind(text, P, "ctrl-alt-x", None, &new).is_err(), "{text}");
            assert!(unbind(text, P, "ctrl-alt-x", None).is_err(), "{text}");
            assert!(reset(text, P, &new).is_err(), "{text}");
        }
    }
}
