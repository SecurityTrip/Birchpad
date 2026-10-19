//! The text's context menu as data: `context-menu.toml` next to the settings, Birchpad's
//! counterpart of Notepad++'s `contextMenu.xml`.
//!
//! ```toml
//! [[item]]
//! command = "edit.cut"
//!
//! [[item]]
//! separator = true
//!
//! [[item]]
//! command = "mark.style-all"
//! args = { style = 1 }
//! label = "Using 1st Style"
//! folder = "Style All Occurrences of Token"
//! ```
//!
//! `label` replaces the command's title; items with the same `folder` that follow each other
//! form a submenu. Commands the catalog does not know are left out.

use serde::{Deserialize, Serialize};

use crate::Invocation;

/// The file name, next to `settings.toml`.
pub const FILE_NAME: &str = "context-menu.toml";

/// The menu Birchpad starts with, close to Notepad++'s.
pub const DEFAULT: &str = r#"# The text's context menu (Settings > Edit Popup ContextMenu). Each [[item]] is a command
# from the catalog with optional `args`, a `label` instead of its title, and a `folder`:
# items with the same folder, one after the other, form a submenu. `separator = true` draws
# a line. Changes apply the next time the menu opens.

[[item]]
command = "edit.cut"

[[item]]
command = "edit.copy"

[[item]]
command = "edit.paste"

[[item]]
command = "edit.delete"

[[item]]
command = "edit.select-all"

[[item]]
command = "edit.begin-end-select"

[[item]]
separator = true

[[item]]
command = "mark.style-all"
args = { style = 1 }
label = "Using 1st Style"
folder = "Style All Occurrences of Token"

[[item]]
command = "mark.style-all"
args = { style = 2 }
label = "Using 2nd Style"
folder = "Style All Occurrences of Token"

[[item]]
command = "mark.style-all"
args = { style = 3 }
label = "Using 3rd Style"
folder = "Style All Occurrences of Token"

[[item]]
command = "mark.style-all"
args = { style = 4 }
label = "Using 4th Style"
folder = "Style All Occurrences of Token"

[[item]]
command = "mark.style-all"
args = { style = 5 }
label = "Using 5th Style"
folder = "Style All Occurrences of Token"

[[item]]
command = "mark.clear-all"

[[item]]
separator = true

[[item]]
command = "edit.convert-case"
args = { to = "upper" }
label = "UPPERCASE"
folder = "Convert Case to"

[[item]]
command = "edit.convert-case"
args = { to = "lower" }
label = "lowercase"
folder = "Convert Case to"

[[item]]
separator = true

[[item]]
command = "edit.open-selected-file"

[[item]]
command = "edit.search-online"

[[item]]
separator = true

[[item]]
command = "edit.toggle-comment"

[[item]]
command = "edit.block-comment"
"#;

/// An item of the file.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ItemDef {
    command: Option<String>,
    args: Option<toml::Value>,
    label: Option<String>,
    folder: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    separator: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MenuFile {
    item: Vec<ItemDef>,
}

/// What the menu shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Command {
        invocation: Invocation,
        /// The file's label; the command's title if none.
        label: Option<String>,
    },
    Separator,
    Folder {
        name: String,
        items: Vec<Node>,
    },
}

/// Reads a menu file. Items with a command the catalog does not know, or neither a command
/// nor `separator`, are left out; separators at either end or twice in a row too.
pub fn parse(text: &str) -> Result<Vec<Node>, ContextMenuError> {
    let file: MenuFile =
        toml::from_str(text).map_err(|error| ContextMenuError(error.message().to_owned()))?;
    let mut nodes: Vec<Node> = Vec::new();
    for item in file.item {
        let node = if item.separator {
            Node::Separator
        } else {
            let Some(command) = item.command.filter(|id| crate::find(id).is_some()) else {
                continue;
            };
            let invocation = match item.args {
                Some(args) => Invocation::with_args(
                    &command,
                    serde_json::to_value(args)
                        .map_err(|error| ContextMenuError(error.to_string()))?,
                ),
                None => Invocation::new(&command),
            };
            Node::Command {
                invocation,
                label: item.label,
            }
        };
        match item.folder.filter(|name| !name.trim().is_empty()) {
            Some(name) => match nodes.last_mut() {
                Some(Node::Folder { name: last, items }) if *last == name => {
                    items.push(node);
                }
                _ => nodes.push(Node::Folder {
                    name,
                    items: vec![node],
                }),
            },
            None => nodes.push(node),
        }
    }
    Ok(tidy(nodes))
}

/// Drops separators at either end, twice in a row, and empty folders; joins folders of one
/// name that end up next to each other.
fn tidy(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    for node in nodes {
        let node = match node {
            Node::Folder { name, items } => {
                let items = tidy(items);
                if items.is_empty() {
                    continue;
                }
                Node::Folder { name, items }
            }
            node => node,
        };
        if node == Node::Separator && matches!(out.last(), None | Some(Node::Separator)) {
            continue;
        }
        // A folder that follows one of its name (an empty one between them was dropped) is
        // the same submenu.
        if let Node::Folder { name, items } = &node
            && let Some(Node::Folder {
                name: last,
                items: last_items,
            }) = out.last_mut()
            && last == name
        {
            last_items.extend(items.iter().cloned());
            continue;
        }
        out.push(node);
    }
    if out.last() == Some(&Node::Separator) {
        out.pop();
    }
    out
}

/// Why a menu file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ContextMenuError(String);

/// The menu as a file that [`parse`] reads back as the same menu.
pub fn write(nodes: &[Node]) -> String {
    fn items(nodes: &[Node], folder: Option<&str>, out: &mut Vec<ItemDef>) {
        for node in nodes {
            match node {
                Node::Separator => out.push(ItemDef {
                    separator: true,
                    folder: folder.map(str::to_owned),
                    ..ItemDef::default()
                }),
                Node::Command { invocation, label } => out.push(ItemDef {
                    command: Some(invocation.command.clone()),
                    args: (!invocation.args.is_null())
                        .then(|| toml::Value::try_from(&invocation.args).ok())
                        .flatten(),
                    label: label.clone(),
                    folder: folder.map(str::to_owned),
                    separator: false,
                }),
                Node::Folder { name, items: inner } => items(inner, Some(name), out),
            }
        }
    }
    let mut file = MenuFile::default();
    items(nodes, None, &mut file.item);
    toml::to_string(&file).expect("a menu serializes")
}

#[cfg(test)]
#[path = "../tests/unit/context_menu/tests.rs"]
mod tests;
