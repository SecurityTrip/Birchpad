//! The catalog of commands: every id the application understands, with its title and scope.

/// Where a command is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// The window: files, tabs, search, view and encoding settings of the active document.
    Workspace,
    /// The focused editor view: caret movement and editing.
    Editor,
}

/// Static description of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// Stable id, used in keymaps, menus, macros and plugins. Never rename a released id.
    pub id: &'static str,
    /// Human-readable name, shown in menus and, later, in the command palette and shortcut mapper.
    pub title: &'static str,
    pub scope: Scope,
}

const fn workspace(id: &'static str, title: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        title,
        scope: Scope::Workspace,
    }
}

const fn editor(id: &'static str, title: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        title,
        scope: Scope::Editor,
    }
}

/// Every command, grouped like the menus.
///
/// Arguments, where a command takes any, are documented next to its entry.
pub const COMMANDS: &[CommandSpec] = &[
    // File
    workspace("file.new", "New"),
    workspace("file.open", "Open..."),
    // { "path": "C:\\notes.txt" }
    workspace("file.open-recent", "Open Recent File"),
    workspace("file.clear-recent", "Empty Recent Files List"),
    workspace("file.save", "Save"),
    workspace("file.save-as", "Save As..."),
    workspace("file.save-all", "Save All"),
    workspace("file.close", "Close"),
    workspace("file.close-all", "Close All"),
    workspace("file.close-others", "Close All but This"),
    workspace("file.exit", "Exit"),
    // Edit
    editor("edit.undo", "Undo"),
    editor("edit.redo", "Redo"),
    editor("edit.cut", "Cut"),
    editor("edit.copy", "Copy"),
    editor("edit.paste", "Paste"),
    editor("edit.delete", "Delete"),
    editor("edit.select-all", "Select All"),
    editor("edit.backspace", "Delete Previous Character"),
    editor("edit.delete-word-left", "Delete to Start of Word"),
    editor("edit.delete-word-right", "Delete to End of Word"),
    editor("edit.newline", "New Line"),
    editor("edit.tab", "Insert Tab"),
    // { "text": "..." }
    editor("edit.insert-text", "Insert Text"),
    editor("edit.toggle-overwrite", "Toggle Insert/Overwrite"),
    // { "eol": "crlf" | "lf" | "cr" }
    editor("edit.convert-eol", "EOL Conversion"),
    // Line Operations
    editor("edit.duplicate-line", "Duplicate Current Line"),
    editor("edit.delete-line", "Delete Current Line"),
    editor("edit.move-line-up", "Move Up Current Line"),
    editor("edit.move-line-down", "Move Down Current Line"),
    editor("edit.insert-line-above", "Insert Blank Line Above Current"),
    editor("edit.insert-line-below", "Insert Blank Line Below Current"),
    editor("edit.join-lines", "Join Lines"),
    editor("edit.split-lines", "Split Lines"),
    // { "by": "lexicographic" | "ignore-case" | "integer" | "decimal-comma" | "decimal-dot"
    //   | "length", "descending": false }
    editor("edit.sort-lines", "Sort Lines"),
    editor("edit.reverse-lines", "Reverse Line Order"),
    editor("edit.shuffle-lines", "Randomize Line Order"),
    // { "consecutive": false }
    editor("edit.remove-duplicate-lines", "Remove Duplicate Lines"),
    // { "blank": false } - with true, also lines of only spaces and tabs
    editor("edit.remove-empty-lines", "Remove Empty Lines"),
    // Blank Operations; { "which": "trailing" | "leading" | "both" }
    editor("edit.trim", "Trim"),
    // { "trim": false }
    editor("edit.eol-to-space", "EOL to Space"),
    editor("edit.tabs-to-spaces", "TAB to Space"),
    // { "leading": false }
    editor("edit.spaces-to-tabs", "Space to TAB"),
    // { "to": "upper" | "lower" | "proper" | "proper-blend" | "sentence" | "sentence-blend"
    //   | "invert" | "random" }
    editor("edit.convert-case", "Convert Case"),
    // Comment/Uncomment
    editor("edit.toggle-comment", "Toggle Single Line Comment"),
    editor("edit.comment-lines", "Single Line Comment"),
    editor("edit.uncomment-lines", "Single Line Uncomment"),
    editor("edit.block-comment", "Block Comment"),
    // Caret movement; every `cursor.*` has a `select.*` twin that extends the selection.
    editor("cursor.left", "Move Left"),
    editor("cursor.right", "Move Right"),
    editor("cursor.up", "Move Up"),
    editor("cursor.down", "Move Down"),
    editor("cursor.word-left", "Move to Previous Word"),
    editor("cursor.word-right", "Move to Next Word"),
    editor("cursor.home", "Move to Line Start"),
    editor("cursor.end", "Move to Line End"),
    editor("cursor.page-up", "Page Up"),
    editor("cursor.page-down", "Page Down"),
    editor("cursor.document-start", "Move to Document Start"),
    editor("cursor.document-end", "Move to Document End"),
    editor("select.left", "Extend Selection Left"),
    editor("select.right", "Extend Selection Right"),
    editor("select.up", "Extend Selection Up"),
    editor("select.down", "Extend Selection Down"),
    editor("select.word-left", "Extend Selection to Previous Word"),
    editor("select.word-right", "Extend Selection to Next Word"),
    editor("select.home", "Extend Selection to Line Start"),
    editor("select.end", "Extend Selection to Line End"),
    editor("select.page-up", "Extend Selection Page Up"),
    editor("select.page-down", "Extend Selection Page Down"),
    editor(
        "select.document-start",
        "Extend Selection to Document Start",
    ),
    editor("select.document-end", "Extend Selection to Document End"),
    // Search
    workspace("search.find", "Find..."),
    workspace("search.replace", "Replace..."),
    workspace("search.find-next", "Find Next"),
    workspace("search.find-previous", "Find Previous"),
    workspace("search.go-to", "Go To..."),
    editor("search.go-to-matching-brace", "Go to Matching Brace"),
    editor(
        "search.select-to-matching-brace",
        "Select All In-between {} [] or ()",
    ),
    workspace("search.close", "Close Find Panel"),
    // View
    workspace("view.word-wrap", "Word Wrap"),
    editor("view.fold-all", "Fold All"),
    editor("view.unfold-all", "Unfold All"),
    editor("view.fold-current", "Collapse Current Level"),
    editor("view.unfold-current", "Uncollapse Current Level"),
    // { "level": 1..8 }
    editor("view.fold-level", "Collapse Level"),
    editor("view.unfold-level", "Uncollapse Level"),
    workspace("view.zoom-in", "Zoom In"),
    workspace("view.zoom-out", "Zoom Out"),
    workspace("view.zoom-reset", "Restore Default Zoom"),
    workspace("view.next-tab", "Next Tab"),
    workspace("view.previous-tab", "Previous Tab"),
    // Encoding; { "encoding": "utf-8" | "utf-8-bom" | "utf-16le-bom" | "ansi" | "windows-1251" | ... }
    workspace("encoding.encode-in", "Encode in"),
    workspace("encoding.convert-to", "Convert to"),
    // Language; { "language": "rust" | "cpp" | ... | "text" }
    workspace("language.set", "Language"),
    // Help
    workspace("help.check-updates", "Check for Updates..."),
    workspace("help.about", "About Birchpad"),
];

/// Looks up a command by id.
pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_well_formed() {
        let mut seen = HashSet::new();
        for spec in COMMANDS {
            assert!(seen.insert(spec.id), "duplicate command id {}", spec.id);
            let (group, name) = spec.id.split_once('.').expect("ids are `group.name`");
            for part in [group, name] {
                assert!(
                    !part.is_empty()
                        && part
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "id {} must be lowercase kebab-case",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn every_cursor_motion_has_a_select_twin() {
        for spec in COMMANDS {
            if let Some(motion) = spec.id.strip_prefix("cursor.") {
                assert!(
                    find(&format!("select.{motion}")).is_some(),
                    "missing select.{motion}"
                );
            }
        }
    }
}
