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
    workspace("file.load-session", "Load Session..."),
    workspace("file.save-session", "Save Session..."),
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
    // Esc: back to one caret (the primary selection), or out of a rectangular selection
    editor("edit.cancel-selection", "Cancel Multiple Selection"),
    editor("edit.begin-end-select", "Begin/End Select"),
    editor(
        "edit.begin-end-select-column",
        "Begin/End Select in Column Mode",
    ),
    // Multi-select; { "match-case": false, "whole-word": false }
    editor("edit.multi-select-all", "Multi-select All"),
    editor("edit.multi-select-next", "Multi-select Next"),
    editor(
        "edit.multi-select-undo",
        "Undo the Latest Added Multi-Select",
    ),
    editor(
        "edit.multi-select-skip",
        "Skip Current & Go to Next Multi-select",
    ),
    editor("edit.column-editor", "Column Editor..."),
    // { "text": "..." } or { "initial": 1, "step": 1, "repeat": 1,
    //   "leading": "none" | "zeros" | "spaces", "format": "dec" | "hex" | "oct" | "bin",
    //   "uppercase": true }
    editor("edit.column-insert", "Insert in Column"),
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
    // Rectangular (column) selection
    editor("select.block-left", "Extend Column Selection Left"),
    editor("select.block-right", "Extend Column Selection Right"),
    editor("select.block-up", "Extend Column Selection Up"),
    editor("select.block-down", "Extend Column Selection Down"),
    editor("select.block-home", "Extend Column Selection to Line Start"),
    editor("select.block-end", "Extend Column Selection to Line End"),
    editor("select.block-page-up", "Extend Column Selection Page Up"),
    editor(
        "select.block-page-down",
        "Extend Column Selection Page Down",
    ),
    // Search
    workspace("search.find", "Find..."),
    workspace("search.replace", "Replace..."),
    workspace("search.find-next", "Find Next"),
    workspace("search.find-previous", "Find Previous"),
    workspace("search.select-and-find-next", "Select and Find Next"),
    workspace(
        "search.select-and-find-previous",
        "Select and Find Previous",
    ),
    workspace("search.find-in-files", "Find in Files..."),
    workspace("search.mark", "Mark..."),
    workspace("search.results-window", "Search Results Window"),
    workspace("search.next-result", "Next Search Result"),
    workspace("search.previous-result", "Previous Search Result"),
    workspace("search.go-to", "Go To..."),
    editor("search.go-to-matching-brace", "Go to Matching Brace"),
    editor(
        "search.select-to-matching-brace",
        "Select All In-between {} [] or ()",
    ),
    workspace("search.close", "Close Find Panel"),
    // Token styles (Search > Style All Occurrences of Token and below); { "style": 1..5 }
    editor("mark.style-all", "Style All Occurrences of Token"),
    editor("mark.style-one", "Style One Token"),
    editor("mark.clear", "Clear Style"),
    editor("mark.clear-all", "Clear all Styles"),
    // { "style": 1..5 }, or no arguments for any style
    editor("mark.jump-up", "Jump Up"),
    editor("mark.jump-down", "Jump Down"),
    // { "style": 1..5 }, or no arguments for all styles
    editor("mark.copy-styled-text", "Copy Styled Text"),
    // Search > Bookmark
    editor("bookmark.toggle", "Toggle Bookmark"),
    editor("bookmark.next", "Next Bookmark"),
    editor("bookmark.previous", "Previous Bookmark"),
    editor("bookmark.clear-all", "Clear All Bookmarks"),
    editor("bookmark.cut-lines", "Cut Bookmarked Lines"),
    editor("bookmark.copy-lines", "Copy Bookmarked Lines"),
    editor(
        "bookmark.paste-to-lines",
        "Paste to (Replace) Bookmarked Lines",
    ),
    editor("bookmark.remove-lines", "Remove Bookmarked Lines"),
    editor("bookmark.remove-unmarked-lines", "Remove Unmarked Lines"),
    editor("bookmark.inverse", "Inverse Bookmark"),
    // View
    workspace("view.show-whitespace", "Show Space and Tab"),
    workspace("view.show-eol", "Show End of Line"),
    workspace("view.show-all-characters", "Show All Characters"),
    workspace("view.indent-guides", "Show Indent Guide"),
    workspace("view.wrap-symbol", "Show Wrap Symbol"),
    workspace("view.word-wrap", "Word Wrap"),
    workspace("view.move-to-other-view", "Move to Other View"),
    workspace("view.clone-to-other-view", "Clone to Other View"),
    workspace("view.focus-other-view", "Focus on Another View"),
    workspace("view.rotate-split", "Rotate Split View"),
    workspace(
        "view.sync-vertical-scroll",
        "Synchronize Vertical Scrolling",
    ),
    workspace(
        "view.sync-horizontal-scroll",
        "Synchronize Horizontal Scrolling",
    ),
    workspace("view.monitoring", "Monitoring (tail -f)"),
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
    editor("encoding.edit-anyway", "Edit Anyway..."),
    // Language; { "language": "rust" | "cpp" | ... | "text" }
    workspace("language.set", "Language"),
    // Help
    workspace("help.check-updates", "Check for Updates..."),
    // From the update dialogs and notifications.
    workspace("help.update-now", "Update and Restart"),
    workspace("help.restart-to-update", "Restart to Update"),
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
