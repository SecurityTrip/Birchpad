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
            cmd("file.load-session"),
            cmd("file.save-session"),
            Separator,
            cmd("file.exit"),
        ],
    );

    let case = |to: &str, label: &str| {
        MenuItem::command_with("edit.convert-case", json!({ "to": to }), label)
    };
    let sort = |by: &str, descending: bool, label: &str| {
        MenuItem::command_with(
            "edit.sort-lines",
            json!({ "by": by, "descending": descending }),
            label,
        )
    };
    let trim = |which: &str, label: &str| {
        MenuItem::command_with("edit.trim", json!({ "which": which }), label)
    };
    let eol = |eol: &str, label: &str| {
        MenuItem::command_with("edit.convert-eol", json!({ "eol": eol }), label)
    };
    // The four variants of Notepad++'s Multi-select All and Multi-select Next.
    let multi_select = |command: &str| {
        [
            (false, false, "Ignore Case & Whole Word"),
            (true, false, "Match Case Only"),
            (false, true, "Match Whole Word Only"),
            (true, true, "Match Case & Whole Word"),
        ]
        .into_iter()
        .map(|(match_case, whole_word, label)| {
            MenuItem::command_with(
                command,
                json!({ "match-case": match_case, "whole-word": whole_word }),
                label,
            )
        })
        .collect::<Vec<_>>()
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
            cmd("edit.begin-end-select"),
            cmd("edit.begin-end-select-column"),
            Separator,
            Submenu(Menu::new(
                "Comment/Uncomment",
                vec![
                    cmd("edit.toggle-comment"),
                    cmd("edit.comment-lines"),
                    cmd("edit.uncomment-lines"),
                    cmd("edit.block-comment"),
                ],
            )),
            Submenu(Menu::new(
                "Convert Case to",
                vec![
                    case("upper", "UPPERCASE"),
                    case("lower", "lowercase"),
                    case("proper", "Proper Case"),
                    case("proper-blend", "Proper Case (blend)"),
                    case("sentence", "Sentence case"),
                    case("sentence-blend", "Sentence case (blend)"),
                    case("invert", "iNVERT cASE"),
                    case("random", "ranDOm CasE"),
                ],
            )),
            Submenu(Menu::new(
                "Line Operations",
                vec![
                    cmd("edit.duplicate-line"),
                    cmd("edit.delete-line"),
                    cmd("edit.move-line-up"),
                    cmd("edit.move-line-down"),
                    cmd("edit.insert-line-above"),
                    cmd("edit.insert-line-below"),
                    Separator,
                    cmd("edit.join-lines"),
                    cmd("edit.split-lines"),
                    Separator,
                    MenuItem::command_with(
                        "edit.remove-duplicate-lines",
                        json!({ "consecutive": false }),
                        "Remove Duplicate Lines",
                    ),
                    MenuItem::command_with(
                        "edit.remove-duplicate-lines",
                        json!({ "consecutive": true }),
                        "Remove Consecutive Duplicate Lines",
                    ),
                    MenuItem::command_with(
                        "edit.remove-empty-lines",
                        json!({ "blank": false }),
                        "Remove Empty Lines",
                    ),
                    MenuItem::command_with(
                        "edit.remove-empty-lines",
                        json!({ "blank": true }),
                        "Remove Empty Lines (Containing Blank characters)",
                    ),
                    Separator,
                    sort(
                        "lexicographic",
                        false,
                        "Sort Lines Lexicographically Ascending",
                    ),
                    sort(
                        "lexicographic",
                        true,
                        "Sort Lines Lexicographically Descending",
                    ),
                    sort(
                        "ignore-case",
                        false,
                        "Sort Lines Lex. Ascending Ignoring Case",
                    ),
                    sort(
                        "ignore-case",
                        true,
                        "Sort Lines Lex. Descending Ignoring Case",
                    ),
                    sort("integer", false, "Sort Lines As Integers Ascending"),
                    sort("integer", true, "Sort Lines As Integers Descending"),
                    sort(
                        "decimal-comma",
                        false,
                        "Sort Lines As Decimals (Comma) Ascending",
                    ),
                    sort(
                        "decimal-comma",
                        true,
                        "Sort Lines As Decimals (Comma) Descending",
                    ),
                    sort(
                        "decimal-dot",
                        false,
                        "Sort Lines As Decimals (Dot) Ascending",
                    ),
                    sort(
                        "decimal-dot",
                        true,
                        "Sort Lines As Decimals (Dot) Descending",
                    ),
                    sort("length", false, "Sort Lines By Length Ascending"),
                    sort("length", true, "Sort Lines By Length Descending"),
                    cmd("edit.reverse-lines"),
                    cmd("edit.shuffle-lines"),
                ],
            )),
            Submenu(Menu::new(
                "Blank Operations",
                vec![
                    trim("trailing", "Trim Trailing Space"),
                    trim("leading", "Trim Leading Space"),
                    trim("both", "Trim Leading and Trailing Space"),
                    MenuItem::command_with(
                        "edit.eol-to-space",
                        json!({ "trim": false }),
                        "EOL to Space",
                    ),
                    MenuItem::command_with(
                        "edit.eol-to-space",
                        json!({ "trim": true }),
                        "Trim both and EOL to Space",
                    ),
                    Separator,
                    cmd("edit.tabs-to-spaces"),
                    MenuItem::command_with(
                        "edit.spaces-to-tabs",
                        json!({ "leading": false }),
                        "Space to TAB (All)",
                    ),
                    MenuItem::command_with(
                        "edit.spaces-to-tabs",
                        json!({ "leading": true }),
                        "Space to TAB (Leading)",
                    ),
                ],
            )),
            Submenu(Menu::new(
                "EOL Conversion",
                vec![
                    eol("crlf", "Windows (CR LF)"),
                    eol("lf", "Unix (LF)"),
                    eol("cr", "Macintosh (CR)"),
                ],
            )),
            Separator,
            Submenu(Menu::new(
                "Multi-select All",
                multi_select("edit.multi-select-all"),
            )),
            Submenu(Menu::new(
                "Multi-select Next",
                multi_select("edit.multi-select-next"),
            )),
            cmd("edit.multi-select-undo"),
            cmd("edit.multi-select-skip"),
            Separator,
            cmd("edit.column-editor"),
        ],
    );

    const ORDINALS: [&str; 5] = ["1st", "2nd", "3rd", "4th", "5th"];
    let styles = |command: &str, label: &dyn Fn(&str) -> String| {
        ORDINALS
            .iter()
            .enumerate()
            .map(|(index, ordinal)| {
                MenuItem::command_with(command, json!({ "style": index + 1 }), &label(ordinal))
            })
            .collect::<Vec<_>>()
    };
    let using = |ordinal: &str| format!("Using {ordinal} Style");
    let plain = |ordinal: &str| format!("{ordinal} Style");
    let mut clear = styles("mark.clear", &|ordinal| format!("Clear {ordinal} Style"));
    clear.push(cmd("mark.clear-all"));
    let mut jump_up = styles("mark.jump-up", &plain);
    jump_up.push(MenuItem::command_with(
        "mark.jump-up",
        json!({}),
        "Any Style",
    ));
    let mut jump_down = styles("mark.jump-down", &plain);
    jump_down.push(MenuItem::command_with(
        "mark.jump-down",
        json!({}),
        "Any Style",
    ));
    let mut copy_styled = styles("mark.copy-styled-text", &plain);
    copy_styled.push(MenuItem::command_with(
        "mark.copy-styled-text",
        json!({}),
        "All Styles",
    ));
    let search = Menu::new(
        "Search",
        vec![
            cmd("search.find"),
            cmd("search.find-next"),
            cmd("search.find-previous"),
            cmd("search.select-and-find-next"),
            cmd("search.select-and-find-previous"),
            cmd("search.replace"),
            Separator,
            cmd("search.mark"),
            Separator,
            cmd("search.results-window"),
            cmd("search.next-result"),
            cmd("search.previous-result"),
            Separator,
            cmd("search.go-to"),
            cmd("search.go-to-matching-brace"),
            cmd("search.select-to-matching-brace"),
            Separator,
            Submenu(Menu::new(
                "Style All Occurrences of Token",
                styles("mark.style-all", &using),
            )),
            Submenu(Menu::new(
                "Style One Token",
                styles("mark.style-one", &using),
            )),
            Submenu(Menu::new("Clear Style", clear)),
            Submenu(Menu::new("Jump Up", jump_up)),
            Submenu(Menu::new("Jump Down", jump_down)),
            Submenu(Menu::new("Copy Styled Text", copy_styled)),
            Separator,
            Submenu(Menu::new(
                "Bookmark",
                vec![
                    cmd("bookmark.toggle"),
                    cmd("bookmark.next"),
                    cmd("bookmark.previous"),
                    cmd("bookmark.clear-all"),
                    cmd("bookmark.cut-lines"),
                    cmd("bookmark.copy-lines"),
                    cmd("bookmark.paste-to-lines"),
                    cmd("bookmark.remove-lines"),
                    cmd("bookmark.remove-unmarked-lines"),
                    cmd("bookmark.inverse"),
                ],
            )),
        ],
    );

    let level = |command: &str| {
        (1..=8)
            .map(|level| {
                MenuItem::command_with(command, json!({ "level": level }), &level.to_string())
            })
            .collect::<Vec<_>>()
    };
    // In Notepad++'s order.
    let view = Menu::new(
        "View",
        vec![
            Submenu(Menu::new(
                "Show Symbol",
                vec![
                    cmd("view.show-whitespace"),
                    cmd("view.show-eol"),
                    cmd("view.show-all-characters"),
                    Separator,
                    cmd("view.indent-guides"),
                    cmd("view.wrap-symbol"),
                ],
            )),
            Submenu(Menu::new(
                "Zoom",
                vec![
                    cmd("view.zoom-in"),
                    cmd("view.zoom-out"),
                    cmd("view.zoom-reset"),
                ],
            )),
            Submenu(Menu::new(
                "Move/Clone Current Document",
                vec![
                    cmd("view.move-to-other-view"),
                    cmd("view.clone-to-other-view"),
                ],
            )),
            Submenu(Menu::new(
                "Tab",
                vec![cmd("view.next-tab"), cmd("view.previous-tab")],
            )),
            cmd("view.word-wrap"),
            cmd("view.focus-other-view"),
            cmd("view.rotate-split"),
            Separator,
            cmd("view.fold-all"),
            cmd("view.unfold-all"),
            cmd("view.fold-current"),
            cmd("view.unfold-current"),
            Submenu(Menu::new("Collapse Level", level("view.fold-level"))),
            Submenu(Menu::new("Uncollapse Level", level("view.unfold-level"))),
            Separator,
            cmd("view.sync-vertical-scroll"),
            cmd("view.sync-horizontal-scroll"),
            Separator,
            cmd("view.monitoring"),
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
