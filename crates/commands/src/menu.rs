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
    /// The built-in themes and those of the `themes` folder, each invoking `settings.theme`.
    Themes,
    /// The saved macros, each invoking `macro.run`.
    Macros,
    /// The saved command lines of the Run menu, each invoking `run.run`.
    RunCommands,
    /// The user defined languages, each invoking `language.set` with `udl:<name>`.
    UserLanguages,
    /// The languages of the spell checker's dictionaries, each invoking `spell.language`.
    SpellLanguages,
    /// The scripts of the `scripts` folder, each invoking `script.run`.
    Scripts,
    /// A submenu for each plugin, with its commands, each invoking `plugin.run`.
    Plugins,
    /// The documents of the active view, each invoking `view.go-to-tab` (the Window menu).
    Documents,
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

/// View > Tab, as in Notepad++.
fn tab_items() -> Vec<MenuItem> {
    const ORDINALS: [&str; 9] = [
        "1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th", "9th",
    ];
    let cmd = MenuItem::command;
    let mut items: Vec<MenuItem> = ORDINALS
        .iter()
        .zip(1..)
        .map(|(ordinal, index)| {
            MenuItem::command_with(
                "view.go-to-tab",
                json!({ "index": index }),
                &format!("{ordinal} Tab"),
            )
        })
        .collect();
    items.extend([
        cmd("view.last-tab"),
        cmd("view.next-tab"),
        cmd("view.previous-tab"),
        MenuItem::Separator,
        cmd("view.move-tab-forward"),
        cmd("view.move-tab-backward"),
        cmd("view.move-tab-to-start"),
        cmd("view.move-tab-to-end"),
        MenuItem::Separator,
        cmd("view.pin-tab"),
        MenuItem::Separator,
    ]);
    items.extend((1..=5).map(|color| {
        MenuItem::command_with(
            "view.tab-color",
            json!({ "color": color }),
            &format!("Apply Color {color}"),
        )
    }));
    items.push(cmd("view.remove-tab-color"));
    items
}

/// File, Edit, Search, View, Encoding and Help, laid out like Notepad++'s menus.
pub fn main_menu() -> Vec<Menu> {
    use MenuItem::{Placeholder as Dynamic, Separator, Submenu};
    let cmd = MenuItem::command;

    let folder = |target: &str, label: &str| {
        MenuItem::command_with("file.containing-folder", json!({ "in": target }), label)
    };
    let file = Menu::new(
        "File",
        vec![
            cmd("file.new"),
            cmd("file.open"),
            Submenu(Menu::new(
                "Open Containing Folder",
                vec![
                    folder("file-manager", "File Manager"),
                    folder("terminal", "Terminal"),
                    folder("workspace", "Folder as Workspace"),
                ],
            )),
            cmd("file.open-default-viewer"),
            cmd("file.open-folder-as-workspace"),
            cmd("file.reload"),
            Submenu(Menu::new(
                "Recent Files",
                vec![
                    Dynamic(Placeholder::RecentFiles),
                    Separator,
                    cmd("file.restore-recent-closed"),
                    cmd("file.open-all-recent"),
                    cmd("file.clear-recent"),
                ],
            )),
            Separator,
            cmd("file.save"),
            cmd("file.save-as"),
            cmd("file.save-copy-as"),
            cmd("file.save-all"),
            cmd("file.rename"),
            Separator,
            cmd("file.close"),
            cmd("file.close-all"),
            Submenu(Menu::new(
                "Close Multiple Documents",
                vec![
                    cmd("file.close-others"),
                    cmd("file.close-all-but-pinned"),
                    cmd("file.close-left"),
                    cmd("file.close-right"),
                    cmd("file.close-unchanged"),
                ],
            )),
            cmd("file.move-to-recycle-bin"),
            Separator,
            cmd("file.load-session"),
            cmd("file.save-session"),
            Separator,
            cmd("file.export-html"),
            cmd("file.print"),
            cmd("file.print-now"),
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
    let date = |format: &str, label: &str| {
        MenuItem::command_with("edit.insert-date-time", json!({ "format": format }), label)
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
            Submenu(Menu::new(
                "Copy to Clipboard",
                vec![
                    cmd("edit.copy-full-path"),
                    cmd("edit.copy-file-name"),
                    cmd("edit.copy-folder-path"),
                ],
            )),
            Submenu(Menu::new(
                "Insert",
                vec![
                    date("short", "Date Time (short)"),
                    date("long", "Date Time (long)"),
                    date("custom", "Date Time (customized)"),
                ],
            )),
            Submenu(Menu::new(
                "Indent",
                vec![cmd("edit.indent"), cmd("edit.outdent")],
            )),
            Submenu(Menu::new(
                "Paste Special",
                vec![
                    cmd("edit.copy-binary"),
                    cmd("edit.cut-binary"),
                    cmd("edit.paste-binary"),
                    Separator,
                    cmd("edit.paste-html"),
                    cmd("edit.paste-rtf"),
                ],
            )),
            Submenu(Menu::new(
                "On Selection",
                vec![
                    cmd("edit.open-selected-file"),
                    cmd("edit.open-selected-folder"),
                    cmd("edit.search-online"),
                ],
            )),
            cmd("edit.begin-end-select"),
            cmd("edit.begin-end-select-column"),
            Separator,
            cmd("edit.text-direction-rtl"),
            cmd("edit.text-direction-ltr"),
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
            cmd("panel.character-panel"),
            cmd("panel.clipboard-history"),
            Separator,
            cmd("edit.toggle-read-only"),
            cmd("edit.clear-read-only-flag"),
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
            cmd("search.find-in-files"),
            cmd("search.find-in-projects"),
            cmd("search.find-next"),
            cmd("search.find-previous"),
            cmd("search.select-and-find-next"),
            cmd("search.select-and-find-previous"),
            cmd("search.find-volatile-next"),
            cmd("search.find-volatile-previous"),
            cmd("search.replace"),
            cmd("search.incremental"),
            Separator,
            cmd("search.mark"),
            Separator,
            Submenu(Menu::new(
                "Change History",
                vec![
                    cmd("search.next-change"),
                    cmd("search.previous-change"),
                    cmd("search.clear-changes"),
                ],
            )),
            Separator,
            cmd("search.results-window"),
            cmd("search.next-result"),
            cmd("search.previous-result"),
            Separator,
            cmd("search.go-to"),
            cmd("search.go-to-file"),
            cmd("navigate.back"),
            cmd("navigate.forward"),
            cmd("search.go-to-matching-brace"),
            cmd("search.select-to-matching-brace"),
            cmd("search.find-characters-in-range"),
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
    let browser = |name: &str, label: &str| {
        MenuItem::command_with("view.in-browser", json!({ "browser": name }), label)
    };
    let view = Menu::new(
        "View",
        vec![
            cmd("view.command-palette"),
            Separator,
            cmd("view.always-on-top"),
            cmd("view.full-screen"),
            cmd("view.post-it"),
            cmd("view.distraction-free"),
            Submenu(Menu::new(
                "View Current File in",
                vec![
                    browser("default", "Default Browser"),
                    browser("firefox", "Firefox"),
                    browser("chrome", "Chrome"),
                    browser("edge", "Edge"),
                    browser("safari", "Safari"),
                ],
            )),
            Separator,
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
                    cmd("view.move-to-new-instance"),
                    cmd("view.open-in-new-instance"),
                ],
            )),
            Submenu(Menu::new("Tab", tab_items())),
            cmd("view.word-wrap"),
            cmd("view.hide-lines"),
            cmd("view.show-hidden-lines"),
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
            Submenu(Menu::new(
                "Project",
                (1..=3)
                    .map(|panel| {
                        MenuItem::command_with(
                            "panel.project",
                            json!({ "panel": panel }),
                            &format!("Project Panel {panel}"),
                        )
                    })
                    .collect(),
            )),
            cmd("panel.folder-as-workspace"),
            Separator,
            cmd("panel.document-map"),
            cmd("panel.document-list"),
            cmd("panel.function-list"),
            Separator,
            Submenu(Menu::new(
                "Preview",
                vec![
                    cmd("view.preview-editor"),
                    cmd("view.preview-split"),
                    cmd("view.preview-only"),
                    Separator,
                    cmd("preview.install-plantuml"),
                ],
            )),
            Separator,
            cmd("view.summary"),
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
            Separator,
            Submenu(Menu::new(
                "User Defined Language",
                vec![
                    cmd("language.define-udl"),
                    Separator,
                    Dynamic(Placeholder::UserLanguages),
                    Separator,
                    cmd("language.import-udl"),
                    cmd("language.open-udl-folder"),
                    cmd("language.udl-collection"),
                ],
            )),
        ],
    );

    let hash = |id: &str, title: &str| {
        Submenu(Menu::new(
            title,
            vec![
                MenuItem::command_with(
                    "tools.hash-files",
                    json!({ "hash": id }),
                    "Generate from Files...",
                ),
                MenuItem::command_with(
                    "tools.hash-selection",
                    json!({ "hash": id }),
                    "Generate from Selection into Clipboard",
                ),
            ],
        ))
    };
    let convert =
        |to: &str, label: &str| MenuItem::command_with("edit.convert", json!({ "to": to }), label);
    let tools = Menu::new(
        "Tools",
        vec![
            hash("md5", "MD5"),
            hash("sha1", "SHA-1"),
            hash("sha256", "SHA-256"),
            hash("sha512", "SHA-512"),
            Separator,
            convert("base64-encode", "Base64 Encode"),
            convert("base64-decode", "Base64 Decode"),
            convert("url-encode", "URL Encode"),
            convert("url-decode", "URL Decode"),
            Separator,
            convert("ascii-to-hex", "ASCII to HEX"),
            convert("hex-to-ascii", "HEX to ASCII"),
        ],
    );

    let macros = Menu::new(
        "Macro",
        vec![
            cmd("macro.start-recording"),
            cmd("macro.stop-recording"),
            cmd("macro.playback"),
            cmd("macro.save"),
            cmd("macro.run-multiple"),
            Separator,
            cmd("macro.trim-and-save"),
            Dynamic(Placeholder::Macros),
        ],
    );

    let run = Menu::new("Run", {
        let mut items = vec![cmd("run.run"), Separator];
        items.extend(crate::run::builtin_run_commands().into_iter().map(|entry| {
            MenuItem::command_with("run.builtin", json!({ "name": entry.name }), &entry.name)
        }));
        items.push(Separator);
        items.push(Dynamic(Placeholder::RunCommands));
        items
    });

    let settings = Menu::new(
        "Settings",
        vec![
            cmd("settings.preferences"),
            cmd("settings.style-configurator"),
            cmd("settings.shortcut-mapper"),
            cmd("settings.edit-context-menu"),
            cmd("settings.default-app"),
            Submenu(Menu::new("Theme", vec![Dynamic(Placeholder::Themes)])),
            Submenu(Menu::new(
                "Import",
                vec![cmd("settings.import-plugins"), cmd("settings.import-theme")],
            )),
        ],
    );

    let plugins = Menu::new(
        "Plugins",
        vec![
            Submenu(Menu::new(
                "JSON Viewer",
                vec![
                    cmd("panel.json-viewer"),
                    Separator,
                    cmd("json.format"),
                    cmd("json.minify"),
                    cmd("json.sort-keys"),
                    Separator,
                    cmd("json.validate"),
                    Separator,
                    cmd("json.copy-value"),
                    cmd("json.copy-key"),
                    cmd("json.copy-path"),
                ],
            )),
            Submenu(Menu::new(
                "XML Tools",
                vec![
                    cmd("xml.pretty-print"),
                    cmd("xml.linearize"),
                    Separator,
                    cmd("xml.check"),
                    cmd("xml.validate"),
                    cmd("xml.xpath"),
                    Separator,
                    cmd("xml.escape"),
                    cmd("xml.unescape"),
                    Separator,
                    cmd("xml.tag-auto-close"),
                ],
            )),
            Submenu(Menu::new(
                "Hex Editor",
                vec![
                    cmd("hex.editor"),
                    cmd("hex.find"),
                    Separator,
                    cmd("hex.view"),
                    cmd("hex.apply"),
                ],
            )),
            Submenu(Menu::new(
                "Compare",
                vec![
                    cmd("compare.compare"),
                    cmd("compare.with-saved"),
                    cmd("compare.with-clipboard"),
                    cmd("compare.with-git"),
                    cmd("compare.clear"),
                    Separator,
                    cmd("compare.previous"),
                    cmd("compare.next"),
                    Separator,
                    cmd("compare.ignore-spaces"),
                    cmd("compare.ignore-case"),
                ],
            )),
            Submenu(Menu::new(
                "Spell Checker",
                vec![
                    cmd("spell.auto"),
                    Separator,
                    cmd("spell.next"),
                    cmd("spell.previous"),
                    cmd("spell.suggestions"),
                    cmd("spell.add-word"),
                    Separator,
                    Submenu(Menu::new(
                        "Language",
                        vec![Dynamic(Placeholder::SpellLanguages)],
                    )),
                    cmd("spell.download-languages"),
                    cmd("spell.open-dictionaries-folder"),
                ],
            )),
            Separator,
            Submenu(Menu::new(
                "Scripts",
                vec![
                    Dynamic(Placeholder::Scripts),
                    Separator,
                    cmd("script.open-folder"),
                    cmd("script.reload"),
                ],
            )),
            cmd("panel.plugin"),
            Separator,
            Dynamic(Placeholder::Plugins),
            Separator,
            cmd("plugin.admin"),
            cmd("plugin.open-folder"),
            cmd("plugin.reload"),
        ],
    );

    let sort = |by: &str, descending: bool, label: &str| {
        MenuItem::command_with(
            "window.sort",
            json!({ "by": by, "descending": descending }),
            label,
        )
    };
    let window = Menu::new(
        "Window",
        vec![
            Submenu(Menu::new(
                "Sort By",
                vec![
                    sort("name", false, "Name A to Z"),
                    sort("name", true, "Name Z to A"),
                    sort("path", false, "Path A to Z"),
                    sort("path", true, "Path Z to A"),
                    sort("type", false, "Type A to Z"),
                    sort("type", true, "Type Z to A"),
                    sort("size", false, "Content Length Ascending"),
                    sort("size", true, "Content Length Descending"),
                    sort("modified", false, "Modified Time Ascending"),
                    sort("modified", true, "Modified Time Descending"),
                ],
            )),
            cmd("window.windows"),
            Separator,
            Dynamic(Placeholder::Documents),
        ],
    );

    let help = Menu::new(
        "Help",
        vec![
            cmd("help.command-line"),
            Separator,
            cmd("help.home-page"),
            cmd("help.manual"),
            cmd("help.forum"),
            Separator,
            cmd("help.check-updates"),
            cmd("help.updater-proxy"),
            Separator,
            cmd("help.debug-info"),
            cmd("help.about"),
        ],
    );

    vec![
        file, edit, search, view, encoding, language, settings, tools, macros, run, plugins,
        window, help,
    ]
}
