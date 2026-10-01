//! The main menu as data. The application turns it into a native menu bar on macOS and a drawn
//! one on Windows and Linux, filling in the [`Placeholder`]s with live content.

use serde_json::json;

use crate::catalog;
use crate::invocation::Invocation;

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuItem {
    Command {
        invocation: Invocation,
        /// Shown instead of the command's title, for items that differ only by arguments.
        label: Option<String>,
    },
    Separator,
    Submenu(Menu),
    /// Items generated at run time.
    Placeholder(Placeholder),
}

/// Menu content that depends on application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// The recently opened files, each invoking `file.open-recent`.
    RecentFiles,
    /// Every supported legacy encoding, grouped by script, each invoking `command` with
    /// `{ "encoding": <name> }`.
    CharacterSets { command: &'static str },
    /// The languages, grouped by first letter as in Notepad++, each invoking `language.set`.
    Languages,
}

impl MenuItem {
    pub fn command(id: &str) -> Self {
        Self::Command {
            invocation: Invocation::new(id),
            label: None,
        }
    }

    pub fn command_with(id: &str, args: serde_json::Value, label: &str) -> Self {
        Self::Command {
            invocation: Invocation::with_args(id, args),
            label: Some(label.to_owned()),
        }
    }

    /// The text to show for a command item: its own label, or the command's title.
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Command { invocation, label } => label
                .as_deref()
                .or_else(|| catalog::find(&invocation.command).map(|spec| spec.title)),
            Self::Submenu(menu) => Some(&menu.title),
            Self::Separator | Self::Placeholder(_) => None,
        }
    }
}

impl Menu {
    pub fn new(title: &str, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.to_owned(),
            items,
        }
    }

    /// Every command invocation in this menu and its submenus.
    pub fn invocations(&self) -> Vec<&Invocation> {
        let mut out = Vec::new();
        for item in &self.items {
            match item {
                MenuItem::Command { invocation, .. } => out.push(invocation),
                MenuItem::Submenu(menu) => out.extend(menu.invocations()),
                MenuItem::Separator | MenuItem::Placeholder(_) => {}
            }
        }
        out
    }
}

/// The Unicode encodings offered directly in the Encoding menu, as in Notepad++.
const UNICODE_ENCODINGS: [(&str, &str); 4] = [
    ("utf-8", "UTF-8"),
    ("utf-8-bom", "UTF-8-BOM"),
    ("utf-16be-bom", "UTF-16 BE BOM"),
    ("utf-16le-bom", "UTF-16 LE BOM"),
];

/// File, Edit, Search, View, Encoding and Help, laid out like Notepad++'s menus.
pub fn main_menu() -> Vec<Menu> {
    use MenuItem::{Placeholder as Dynamic, Separator, Submenu};
    let cmd = MenuItem::command;

    let file = Menu::new(
        "File",
        vec![
            cmd("file.new"),
            cmd("file.open"),
            Submenu(Menu::new(
                "Recent Files",
                vec![
                    Dynamic(Placeholder::RecentFiles),
                    Separator,
                    cmd("file.clear-recent"),
                ],
            )),
            Separator,
            cmd("file.save"),
            cmd("file.save-as"),
            cmd("file.save-all"),
            Separator,
            cmd("file.close"),
            cmd("file.close-all"),
            cmd("file.close-others"),
            Separator,
            cmd("file.exit"),
        ],
    );

    let eol = |eol: &str, label: &str| {
        MenuItem::command_with("edit.convert-eol", json!({ "eol": eol }), label)
    };
    let edit = Menu::new(
        "Edit",
        vec![
            cmd("edit.undo"),
            cmd("edit.redo"),
            Separator,
            cmd("edit.cut"),
            cmd("edit.copy"),
            cmd("edit.paste"),
            cmd("edit.delete"),
            cmd("edit.select-all"),
            Separator,
            Submenu(Menu::new(
                "EOL Conversion",
                vec![
                    eol("crlf", "Windows (CR LF)"),
                    eol("lf", "Unix (LF)"),
                    eol("cr", "Macintosh (CR)"),
                ],
            )),
        ],
    );

    let search = Menu::new(
        "Search",
        vec![
            cmd("search.find"),
            cmd("search.find-next"),
            cmd("search.find-previous"),
            cmd("search.replace"),
            Separator,
            cmd("search.go-to"),
            cmd("search.go-to-matching-brace"),
            cmd("search.select-to-matching-brace"),
        ],
    );

    let view = Menu::new(
        "View",
        vec![
            cmd("view.word-wrap"),
            Separator,
            Submenu(Menu::new(
                "Zoom",
                vec![
                    cmd("view.zoom-in"),
                    cmd("view.zoom-out"),
                    cmd("view.zoom-reset"),
                ],
            )),
            Submenu(Menu::new(
                "Tab",
                vec![cmd("view.next-tab"), cmd("view.previous-tab")],
            )),
        ],
    );

    let encode_in = |name: &str, label: &str| {
        MenuItem::command_with("encoding.encode-in", json!({ "encoding": name }), label)
    };
    let convert_to = |name: &str, label: &str| {
        MenuItem::command_with(
            "encoding.convert-to",
            json!({ "encoding": name }),
            &format!("Convert to {label}"),
        )
    };
    let mut encoding_items = vec![encode_in("ansi", "ANSI")];
    encoding_items.extend(
        UNICODE_ENCODINGS
            .iter()
            .map(|(name, label)| encode_in(name, label)),
    );
    encoding_items.push(Submenu(Menu::new(
        "Character Sets",
        vec![Dynamic(Placeholder::CharacterSets {
            command: "encoding.encode-in",
        })],
    )));
    encoding_items.push(Separator);
    encoding_items.push(convert_to("ansi", "ANSI"));
    encoding_items.extend(
        UNICODE_ENCODINGS
            .iter()
            .map(|(name, label)| convert_to(name, label)),
    );
    encoding_items.push(Submenu(Menu::new(
        "Convert to Character Set",
        vec![Dynamic(Placeholder::CharacterSets {
            command: "encoding.convert-to",
        })],
    )));
    let encoding = Menu::new("Encoding", encoding_items);

    let language = Menu::new(
        "Language",
        vec![
            MenuItem::command_with("language.set", json!({ "language": "text" }), "Normal Text"),
            Separator,
            Dynamic(Placeholder::Languages),
        ],
    );

    let help = Menu::new(
        "Help",
        vec![cmd("help.check-updates"), Separator, cmd("help.about")],
    );

    vec![file, edit, search, view, encoding, language, help]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_notepad_plus_plus_top_level_menus() {
        let titles: Vec<String> = main_menu().into_iter().map(|menu| menu.title).collect();
        assert_eq!(
            titles,
            [
                "File", "Edit", "Search", "View", "Encoding", "Language", "Help"
            ]
        );
    }

    #[test]
    fn every_menu_command_exists_and_has_a_label() {
        for menu in main_menu() {
            for invocation in menu.invocations() {
                assert!(
                    catalog::find(&invocation.command).is_some(),
                    "unknown command {} in menu {}",
                    invocation.command,
                    menu.title
                );
            }
            fn check(menu: &Menu) {
                for item in &menu.items {
                    match item {
                        MenuItem::Command { .. } => assert!(item.label().is_some()),
                        MenuItem::Submenu(sub) => check(sub),
                        _ => {}
                    }
                }
            }
            check(&menu);
        }
    }
}
