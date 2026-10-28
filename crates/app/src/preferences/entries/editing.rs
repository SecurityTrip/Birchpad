//! Preferences > Editing.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Editing",
        "editor.tab-width",
        "Tab size",
        Kind::Number { min: 1, max: 16 },
    ),
    entry(
        "Editing",
        "editor.insert-spaces",
        "Replace tabs by spaces",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.auto-indent",
        "Auto-indent",
        Kind::Choice(&[
            ("off", "None"),
            ("basic", "Basic"),
            ("advanced", "Advanced"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line",
        "Current line",
        Kind::Choice(&[
            ("off", "None"),
            ("background", "Background"),
            ("frame", "Frame"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line-frame-width",
        "Frame width",
        Kind::Number { min: 1, max: 6 },
    ),
    entry(
        "Editing",
        "editor.sticky-scroll",
        "Sticky scroll: the first lines of the blocks around the top stay there",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.sticky-scroll-lines",
        "Sticky scroll lines at most",
        Kind::Number { min: 1, max: 10 },
    ),
    entry(
        "Editing",
        "editor.caret-width",
        "Caret width",
        Kind::Choice(&[
            ("0", "0"),
            ("1", "1"),
            ("2", "2"),
            ("3", "3"),
            ("block", "Block"),
            ("block-after", "Block After"),
        ]),
    ),
    entry(
        "Editing",
        "editor.caret-blink-ms",
        "Caret blink rate (ms, 0: no blinking)",
        Kind::Number { min: 0, max: 2000 },
    ),
    entry(
        "Editing",
        "editor.wrap-indent",
        "Line wrap",
        Kind::Choice(&[
            ("default", "Default"),
            ("aligned", "Aligned"),
            ("indent", "Indent"),
        ]),
    ),
    entry(
        "Editing",
        "editor.virtual-space",
        "Enable virtual space",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.scroll-beyond-last-line",
        "Scrolling beyond the last line",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.keep-selection-on-right-click",
        "Keep the selection on a right click outside it",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.copy-line-without-selection",
        "Copy and cut the line without a selection",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.selection-foreground",
        "Selected text in its own color",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.drag-text",
        "Drag and drop the selected text",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.multi-editing",
        "Enable multi-editing (Ctrl+click)",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.column-to-multi-editing",
        "Column selection to multi-editing",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.eol-style",
        "End of line rendering",
        Kind::Choice(&[("box", "Default"), ("plain-text", "Plain text")]),
    ),
    entry(
        "Editing",
        "editor.non-printing-style",
        "Non-printing characters",
        Kind::Choice(&[
            ("marks", "Framed and marked"),
            ("abbreviation", "Abbreviation"),
            ("code-point", "Code point"),
        ]),
    ),
    entry(
        "Editing",
        "editor.smooth-font",
        "Enable smooth font",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.prevent-control-typing",
        "Prevent typing control characters",
        Kind::Bool,
    ),
];
