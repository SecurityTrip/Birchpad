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
    // { "in": "file-manager" | "terminal" | "workspace" }: the active file's folder
    workspace("file.containing-folder", "Open Containing Folder"),
    workspace("file.open-default-viewer", "Open in Default Viewer"),
    workspace("file.reload", "Reload from Disk"),
    workspace(
        "file.open-folder-as-workspace",
        "Open Folder as Workspace...",
    ),
    // { "path": "C:\\notes.txt" }
    workspace("file.open-recent", "Open Recent File"),
    workspace("file.restore-recent-closed", "Restore Recent Closed File"),
    workspace("file.open-all-recent", "Open All Recent Files"),
    workspace("file.clear-recent", "Empty Recent Files List"),
    workspace("file.save", "Save"),
    workspace("file.save-as", "Save As..."),
    workspace("file.save-copy-as", "Save a Copy As..."),
    workspace("file.save-all", "Save All"),
    workspace("file.rename", "Rename..."),
    workspace("file.close", "Close"),
    workspace("file.close-all", "Close All"),
    workspace("file.close-others", "Close All but This"),
    workspace("file.close-all-but-pinned", "Close All but Pinned"),
    // The tabs left or right of the active one, in its view
    workspace("file.close-left", "Close All to the Left"),
    workspace("file.close-right", "Close All to the Right"),
    workspace("file.close-unchanged", "Close All Unchanged"),
    workspace("file.move-to-recycle-bin", "Move to Recycle Bin"),
    workspace("file.load-session", "Load Session..."),
    workspace("file.save-session", "Save Session..."),
    workspace("file.exit", "Exit"),
    // Edit
    // The active document's full path (its name if untitled), file name, folder
    workspace("edit.copy-full-path", "Copy Current Full File Path"),
    workspace("edit.copy-file-name", "Copy Current Filename"),
    workspace("edit.copy-folder-path", "Copy Current Dir. Path"),
    // { "format": "short" | "long" | "custom" }: the local time at every caret
    editor("edit.insert-date-time", "Insert Date Time"),
    // The document read-only in Birchpad, or the file's attribute
    workspace("edit.toggle-read-only", "Read-Only in Birchpad"),
    workspace("edit.clear-read-only-flag", "Clear Read-Only Flag"),
    workspace(
        "edit.toggle-read-only-attribute",
        "Read-Only Attribute of the File",
    ),
    workspace("edit.set-read-only-all", "Set Read-Only for All Documents"),
    workspace(
        "edit.clear-read-only-all",
        "Clear Read-Only for All Documents",
    ),
    // A message on selecting columns (Alt+drag, Alt+Shift+arrows)
    workspace("edit.column-mode", "Column Mode..."),
    // Every character of the selections but line breaks becomes █
    editor("edit.redact-selection", "Redact Selection"),
    // Preferences > Search Engine
    workspace("edit.change-search-engine", "Change Search Engine..."),
    // Paste Special: text with NUL characters, the clipboard's HTML or RTF
    editor("edit.copy-binary", "Copy Binary Content"),
    editor("edit.cut-binary", "Cut Binary Content"),
    editor("edit.paste-binary", "Paste Binary Content"),
    editor("edit.paste-html", "Paste HTML Content"),
    editor("edit.paste-rtf", "Paste RTF Content"),
    // On Selection: the primary selection names a file or is searched for
    workspace("edit.open-selected-file", "Open File"),
    workspace("edit.open-selected-folder", "Open Containing Folder"),
    workspace("edit.search-online", "Search on Internet"),
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
    editor("edit.indent", "Increase Line Indent"),
    editor("edit.outdent", "Decrease Line Indent"),
    // { "text": "..." }
    editor("edit.insert-text", "Insert Text"),
    // Auto-completion
    editor("completion.complete", "Function Completion"),
    editor("completion.parameter-hint", "Function Parameters Hint"),
    // The document's words alone; the names in the folder of the path typed
    editor("completion.words", "Word Completion"),
    editor("completion.path", "Path Completion"),
    editor("completion.next", "Next Completion"),
    editor("completion.previous", "Previous Completion"),
    editor("completion.accept", "Accept Completion"),
    editor("completion.cancel", "Close Completion"),
    editor("completion.next-overload", "Next Overload"),
    editor("completion.previous-overload", "Previous Overload"),
    // { "text": "..." }: typed text, overwriting in overwrite mode (what macros record)
    editor("edit.type", "Type Text"),
    editor("edit.text-direction-rtl", "Text Direction RTL"),
    editor("edit.text-direction-ltr", "Text Direction LTR"),
    editor("edit.toggle-overwrite", "Toggle Insert/Overwrite"),
    // { "eol": "crlf" | "lf" | "cr" }
    editor("edit.convert-eol", "EOL Conversion"),
    // Esc: back to one caret (the primary selection), or out of a rectangular selection
    editor("edit.cancel-selection", "Cancel Multiple Selection"),
    editor("edit.begin-end-select", "Begin/End Select"),
    // By the syntax tree (else word, line, all), and back the way it grew
    editor("edit.expand-selection", "Expand Selection"),
    editor("edit.shrink-selection", "Shrink Selection"),
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
    // What macros record of a search: { "text", "match-case", "whole-word", "mode": "normal" |
    // "extended" | "regex", "dot-all", "backward", "wrap" }; replace-text also takes "with" and
    // "all"
    workspace("search.find-text", "Find Text"),
    workspace("search.replace-text", "Replace Text"),
    workspace("search.find-previous", "Find Previous"),
    workspace("search.select-and-find-next", "Select and Find Next"),
    workspace(
        "search.select-and-find-previous",
        "Select and Find Previous",
    ),
    // As Select and Find, leaving the find panel's text as it is
    workspace("search.find-volatile-next", "Find (Volatile) Next"),
    workspace("search.find-volatile-previous", "Find (Volatile) Previous"),
    workspace("search.find-in-projects", "Find in Projects..."),
    workspace(
        "search.find-characters-in-range",
        "Find Characters in Range...",
    ),
    // { "from": 128, "to": 1114111, "backward": false, "wrap": true }: character codes
    editor("search.find-in-range", "Find Character in Range"),
    editor("search.next-change", "Go to Next Change"),
    editor("search.previous-change", "Go to Previous Change"),
    editor("search.clear-changes", "Clear All Changes"),
    workspace("search.find-in-files", "Find in Files..."),
    workspace("search.incremental", "Incremental Search"),
    workspace("search.close-incremental", "Close Incremental Search"),
    workspace("search.mark", "Mark..."),
    workspace("search.copy-marked-text", "Copy Marked Text"),
    // The search results' lines searched again: a search of its own
    workspace("search.find-in-results", "Find in These Search Results..."),
    workspace("search.results-window", "Search Results Window"),
    workspace("search.next-result", "Next Search Result"),
    workspace("search.previous-result", "Previous Search Result"),
    workspace("search.go-to", "Go To..."),
    workspace("navigate.back", "Go Back"),
    workspace("navigate.forward", "Go Forward"),
    // Side panels (View and Edit menus); { "panel": 1..3 } for the project panels
    workspace("panel.folder-as-workspace", "Folder as Workspace"),
    workspace("panel.project", "Project Panel"),
    workspace("panel.document-map", "Document Map"),
    workspace("panel.plugin", "Plugin Panel"),
    workspace("panel.document-list", "Document List"),
    workspace("panel.function-list", "Function List"),
    workspace("panel.clipboard-history", "Clipboard History"),
    workspace("panel.character-panel", "Character Panel"),
    workspace("panel.json-viewer", "Show JSON Viewer"),
    // View > Preview: a Markdown or PlantUML document's text, its preview beside it, or the
    // preview alone
    workspace("view.preview-editor", "Editor Only"),
    workspace("view.preview-split", "Editor and Preview"),
    workspace("view.preview-only", "Preview Only"),
    workspace("preview.zoom-in", "Zoom In on Diagrams"),
    workspace("preview.zoom-out", "Zoom Out of Diagrams"),
    workspace("preview.actual-size", "Diagrams at Actual Size"),
    workspace("preview.fit", "Fit Diagrams to Width"),
    workspace("preview.white-background", "White Diagram Background"),
    workspace("preview.copy-diagram", "Copy Diagram"),
    workspace("preview.export-png", "Export Diagram as PNG..."),
    workspace("preview.export-svg", "Export Diagram as SVG..."),
    workspace("preview.install-plantuml", "Install PlantUML"),
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
    // Spaces that are not plain spaces, characters without width (U+200B, U+FEFF, ...)
    workspace("view.show-non-printing", "Show Non-Printing Characters"),
    // C0 and C1 control characters, U+2028 and U+2029, as pictures
    workspace(
        "view.show-control-chars",
        "Show Control Characters & Unicode EOL",
    ),
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
    // Off: each view zooms on its own
    workspace("view.zoom-sync", "Synchronize Across Views"),
    // { "browser": "default" | "firefox" | "chrome" | "edge" | "safari" }: the active file
    workspace("view.in-browser", "View Current File in"),
    editor("view.hide-lines", "Hide Lines"),
    editor("view.show-hidden-lines", "Show Hidden Lines"),
    workspace("view.summary", "Summary..."),
    // Every command by its name, the recently used first
    workspace("view.command-palette", "Command Palette..."),
    // The files of the workspace's folders and the recent files, by parts of their path
    workspace("search.go-to-file", "Go to File..."),
    workspace("view.always-on-top", "Always on Top"),
    workspace("view.full-screen", "Toggle Full Screen Mode"),
    workspace("view.post-it", "Post-It"),
    workspace("view.distraction-free", "Distraction Free Mode"),
    // The active file in another Birchpad, at the caret; Move closes it here
    workspace("view.move-to-new-instance", "Move to New Instance"),
    workspace("view.open-in-new-instance", "Open in New Instance"),
    workspace("view.next-tab", "Next Tab"),
    workspace("view.previous-tab", "Previous Tab"),
    // { "index": 1..9 }, counted from the first tab of the active view
    workspace("view.go-to-tab", "Go to Tab"),
    workspace("view.last-tab", "Last Tab"),
    workspace("view.move-tab-forward", "Move Tab Forward"),
    workspace("view.move-tab-backward", "Move Tab Backward"),
    workspace("view.move-tab-to-start", "Move Tab to Start"),
    workspace("view.move-tab-to-end", "Move Tab to End"),
    workspace("view.pin-tab", "Pin Tab"),
    // { "color": 1..5 }
    workspace("view.tab-color", "Apply Color"),
    workspace("view.remove-tab-color", "Remove Color"),
    // { "backward": false }: the next document of the switcher's list (Ctrl+Tab)
    workspace("view.switch-document", "Document Switcher"),
    // Encoding; { "encoding": "utf-8" | "utf-8-bom" | "utf-16le-bom" | "ansi" | "windows-1251" | ... }
    workspace("encoding.encode-in", "Encode in"),
    workspace("encoding.convert-to", "Convert to"),
    editor("encoding.edit-anyway", "Edit Anyway..."),
    // Language; { "language": "rust" | "cpp" | ... | "text" }
    workspace("file.print", "Print..."),
    workspace("file.print-now", "Print Now"),
    workspace("file.export-html", "Export to HTML..."),
    workspace("language.set", "Language"),
    workspace("language.define-udl", "Define Your Language..."),
    workspace("language.import-udl", "Import..."),
    workspace(
        "language.open-udl-folder",
        "Open User Defined Language Folder...",
    ),
    workspace(
        "language.udl-collection",
        "Notepad++ User Defined Languages Collection",
    ),
    // Tools; { "hash": "md5" | "sha1" | "sha256" | "sha512" }
    workspace("tools.hash-files", "Generate from Files..."),
    // The hash of a text typed in a dialog, of each line or of all
    workspace("tools.hash-text", "Generate..."),
    editor(
        "tools.hash-selection",
        "Generate from Selection into Clipboard",
    ),
    // { "to": "base64-encode" | "base64-decode" | "url-encode" | "url-decode" | "ascii-to-hex" | "hex-to-ascii" }
    editor("edit.convert", "Convert"),
    // Run; { "line": <a command line> } or { "name": <a saved one> }; neither asks for it
    workspace("run.run", "Run..."),
    // { "name": "Google Search" }: one of Notepad++'s own Run menu entries
    workspace("run.builtin", "Run Built-in Command"),
    // Macro; macro.run takes { "name": <a saved macro>, "times": 1 } or { "until-end": true }
    workspace("macro.start-recording", "Start Recording"),
    workspace("macro.stop-recording", "Stop Recording"),
    workspace("macro.playback", "Playback"),
    workspace("macro.save", "Save Current Recorded Macro..."),
    workspace("macro.run-multiple", "Run a Macro Multiple Times..."),
    workspace("macro.run", "Run Macro"),
    workspace("macro.trim-and-save", "Trim Trailing Space and Save"),
    // Window; window.sort takes { "by": "name" | "path" | "type" | "size" | "modified", "descending": false }
    workspace("window.sort", "Sort By"),
    workspace("window.windows", "Windows..."),
    // Settings; settings.theme takes { "name": "Default" | "Dark" | <a theme file's name> }
    workspace("settings.preferences", "Preferences..."),
    workspace("settings.style-configurator", "Style Configurator..."),
    workspace("settings.shortcut-mapper", "Shortcut Mapper..."),
    workspace("settings.edit-context-menu", "Edit Popup ContextMenu"),
    workspace(
        "settings.default-app",
        "Make Birchpad the Default Editor...",
    ),
    workspace("settings.theme", "Theme"),
    workspace("settings.import-theme", "Import Style Themes..."),
    // settings.import-plugins takes { "paths": [<plugin.toml>, ...] } or asks for them
    workspace("settings.import-plugins", "Import plugin(s)..."),
    // Plugins: the built-in JSON Viewer and XML Tools work on the selection, or the whole text
    editor("json.format", "Format JSON"),
    editor("json.minify", "Compress JSON"),
    editor("json.validate", "Validate JSON"),
    editor("json.sort-keys", "Sort JSON Keys"),
    // json.find-path takes { "path": "$.store.book[0]" }
    workspace("json.find-path", "Find JSON Path"),
    workspace("json.copy-value", "Copy Value"),
    workspace("json.copy-key", "Copy Key"),
    workspace("json.copy-path", "Copy Path"),
    editor("xml.pretty-print", "Pretty Print XML"),
    editor("xml.linearize", "Linearize XML"),
    editor("xml.check", "Check XML Syntax"),
    // xml.validate takes { "schema": <an .xsd> } or validates as the document says
    workspace("xml.validate", "Validate Now"),
    // xml.xpath takes { "expression": "//item" } or opens its dialog
    workspace("xml.xpath", "Evaluate XPath Expression..."),
    editor("xml.escape", "Escape Selected Text"),
    editor("xml.unescape", "Unescape Selected Text"),
    workspace("xml.tag-auto-close", "Tag Auto-Close"),
    // Hex Editor: the active document's bytes in a new document, and back
    workspace("hex.view", "Hex Dump in New Document"),
    workspace("hex.apply", "Apply Hex to Document"),
    workspace("hex.editor", "View in HEX"),
    // hex.find takes { "pattern": "4A 6F" or "\"text\"", "backward": false }, or goes to
    // the hex view's field
    workspace("hex.find", "Find in HEX"),
    workspace("hex.switch-column", "Switch Between Hex and Text"),
    // Compare: the active document with the other view's, or with the tab before it
    workspace("compare.compare", "Compare"),
    workspace("compare.clear", "Clear Results"),
    workspace("compare.with-saved", "Diff since Last Save"),
    workspace("compare.with-clipboard", "Compare to Clipboard"),
    workspace("compare.with-git", "Diff to Git HEAD"),
    editor("compare.next", "Next Difference"),
    editor("compare.previous", "Previous Difference"),
    workspace("compare.ignore-spaces", "Ignore Spaces"),
    workspace("compare.ignore-case", "Ignore Case"),
    // Spell Checker; spell.language takes { "language": "en_US" }
    workspace("spell.auto", "Spell Check Document Automatically"),
    editor("spell.next", "Find Next Misspelling"),
    editor("spell.previous", "Find Previous Misspelling"),
    editor("spell.suggestions", "Suggestions"),
    editor("spell.add-word", "Add Word to Dictionary"),
    workspace("spell.language", "Dictionary Language"),
    workspace("spell.open-dictionaries-folder", "Open Dictionaries Folder"),
    // Scripts; script.run takes { "name": <a script of the scripts folder, without .rhai> },
    // and "command": <a function the script declares as a command> to run that after it
    workspace("script.run", "Run Script"),
    workspace("script.open-folder", "Open Scripts Folder"),
    workspace("script.reload", "Reload Scripts"),
    // Plugins; plugin.run takes { "plugin": <its id>, "command": <one of its commands> }
    workspace("plugin.run", "Run Plugin Command"),
    workspace("plugin.open-folder", "Open Plugins Folder"),
    workspace("plugin.reload", "Reload Plugins"),
    // plugin.set-setting takes { "plugin": <id>, "key": <a setting>, "value": <text> }
    workspace("plugin.set-setting", "Set Plugin Setting"),
    // Plugins Admin; plugin.install takes { "path": <a plugin.toml> } or asks for it,
    // plugin.uninstall { "plugin": <id> }, plugin.enable { "plugin": <id>, "enabled": true }
    workspace("plugin.admin", "Plugins Admin..."),
    workspace("plugin.install", "Install Plugin..."),
    workspace("plugin.uninstall", "Remove Plugin"),
    workspace("plugin.enable", "Turn Plugin On or Off"),
    // The online catalog of plugins and dictionaries; plugin.install-online takes
    // { "plugin": <id> }, spell.download { "language": "en_GB" }
    workspace("catalog.refresh", "Refresh Online List"),
    workspace("plugin.install-online", "Install Plugin from the List"),
    workspace("spell.download", "Download Dictionary"),
    workspace("spell.download-languages", "Download Languages..."),
    // plugin.allow-files takes { "plugin": <id>, "allowed": true }
    workspace("plugin.allow-files", "Allow Plugin to Use Files"),
    // Help
    workspace("help.check-updates", "Check for Updates..."),
    // From the update dialogs and notifications.
    workspace("help.update-now", "Update and Restart"),
    workspace("help.restart-to-update", "Restart to Update"),
    workspace("help.command-line", "Command Line Arguments..."),
    workspace("help.home-page", "Birchpad Home"),
    workspace("help.manual", "Online Manual"),
    workspace("help.forum", "Community (Forum)"),
    workspace("help.updater-proxy", "Set Updater Proxy..."),
    workspace("help.debug-info", "Debug Info..."),
    workspace("help.about", "About Birchpad"),
];

/// Looks up a command by id.
pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}
