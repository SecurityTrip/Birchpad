//! Birchpad's editor: a document ([`buffer::Buffer`]) and the editor view, one view of a buffer,
//! with its own selection, scroll position and layout; the active theme ([`theme`]).
//!
//! Several views may show the same buffer (split view, "Clone to Other View"); each maps its
//! selection through the changes the others make. Layout in cells (tab stops, word wrap, rows)
//! comes from `birchpad-view`; [`element::EditorElement`] turns it into pixels and paints it.
//!
//! The editor registers its commands in `birchpad_ui`'s registry ([`register_commands`]) and
//! asks the application for the rest through [`EditorHooks`] and [`ActiveFindOptions`].

#[macro_use]
extern crate birchpad_i18n;

pub mod banner;
pub mod buffer;
mod hooks;

pub mod column_editor;
pub mod completion;
pub mod element;
pub mod folding;
pub mod layout;
pub mod marks;
pub mod multi;
pub mod operations;
pub mod smart_highlight;
pub mod symbols;
pub mod theme;

pub use hooks::{ActiveFindOptions, EditorHooks, Ranges};

use std::ops::Range as ByteRange;
use std::time::Duration;

use birchpad_core::motion::{self, line_of, line_range, line_range_with_break};
use birchpad_core::{
    Edit, LineEnding, LineMarkers, Range, RevisionId, Rope, Selection, Transaction, UndoGrouping,
};
use birchpad_view::{Block, BlockPoint, DisplayMap, LayoutConfig, tab_advance};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, Subscription, Task, UTF16Selection, Window, div, point, prelude::*, px,
};
use serde::Deserialize;

use birchpad_cli::CaretTarget;
use birchpad_config::{AutoIndent, CurrentLine, Edge};

use crate::buffer::{Buffer, BufferEvent, ReadOnly};
use anyhow::Result;
use birchpad_commands::Scope;
use birchpad_ui::{AppState, CommandRegistry, CommandTarget, RunCommand};
use element::EditorElement;
use layout::Layout;

impl CommandTarget for EditorView {
    const SCOPE: Scope = Scope::Editor;
}

/// Registering the handlers of editor commands.
pub trait EditorCommands {
    /// Registers an editor command whose arguments decode to `A` (`()` for none).
    fn editor<A: serde::de::DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    );
}

impl EditorCommands for CommandRegistry {
    fn editor<A: serde::de::DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }
}

/// The platform's monospaced font: the editor's when the theme names none.
pub const MONOSPACE: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

/// How many colors a tab can be given (Notepad++'s five).
pub const TAB_COLORS: u8 = 5;

/// What a tab carries besides its document: pinned, and its color.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TabMarks {
    /// Pinned tabs stay at the start of the bar and Close All but Pinned leaves them.
    pub pinned: bool,
    /// 1 to [`TAB_COLORS`]; none if unset.
    pub color: Option<u8>,
}

/// Font size at zoom level 0, in pixels.
const BASE_FONT_SIZE: f32 = 14.;
/// Zoom levels are font size steps of one pixel, like Notepad++'s points.
pub const ZOOM_RANGE: std::ops::RangeInclusive<i32> = -8..=40;
const CARET_BLINK: Duration = Duration::from_millis(530);

/// Caret motions; each is registered as `cursor.<name>` and `select.<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

const MOTIONS: [(&str, &str, Motion); 12] = [
    ("cursor.left", "select.left", Motion::Left),
    ("cursor.right", "select.right", Motion::Right),
    ("cursor.up", "select.up", Motion::Up),
    ("cursor.down", "select.down", Motion::Down),
    ("cursor.word-left", "select.word-left", Motion::WordLeft),
    ("cursor.word-right", "select.word-right", Motion::WordRight),
    ("cursor.home", "select.home", Motion::Home),
    ("cursor.end", "select.end", Motion::End),
    ("cursor.page-up", "select.page-up", Motion::PageUp),
    ("cursor.page-down", "select.page-down", Motion::PageDown),
    (
        "cursor.document-start",
        "select.document-start",
        Motion::DocumentStart,
    ),
    (
        "cursor.document-end",
        "select.document-end",
        Motion::DocumentEnd,
    ),
];

#[derive(Deserialize)]
struct InsertTextArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum EolName {
    Crlf,
    Lf,
    Cr,
}

#[derive(Deserialize)]
struct ConvertEolArgs {
    eol: EolName,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    completion::register_commands(registry);
    folding::register_commands(registry);
    marks::register_commands(registry);
    multi::register_commands(registry);
    operations::register_commands(registry);
    for (cursor, select, motion) in MOTIONS {
        registry.editor(cursor, move |this, (), _, cx| {
            this.move_carets(motion, false, cx);
            Ok(())
        });
        registry.editor(select, move |this, (), _, cx| {
            this.move_carets(motion, true, cx);
            Ok(())
        });
    }
    registry.editor("edit.undo", |this, (), _, cx| {
        this.undo(cx);
        Ok(())
    });
    registry.editor("edit.redo", |this, (), _, cx| {
        this.redo(cx);
        Ok(())
    });
    registry.editor("edit.copy", |this, (), _, cx| {
        let _ = this.copy(cx);
        Ok(())
    });
    registry.editor("edit.cut", |this, (), _, cx| {
        this.cut(cx);
        Ok(())
    });
    registry.editor("edit.paste", |this, (), _, cx| {
        this.paste(cx);
        Ok(())
    });
    registry.editor("edit.delete", |this, (), _, cx| {
        this.delete_with(cx, motion::next_boundary);
        Ok(())
    });
    registry.editor("edit.backspace", |this, (), _, cx| {
        if !this.backspace_pair(cx) {
            this.delete_with(cx, motion::prev_boundary);
        }
        let head = this.selection.primary().head;
        if this.call_tip.as_ref().is_some_and(|tip| head < tip.open) {
            this.call_tip = None;
        }
        if this.completion.is_some() {
            this.update_completion(false, cx);
        }
        Ok(())
    });
    registry.editor("edit.delete-word-left", |this, (), _, cx| {
        this.delete_with(cx, motion::word_left);
        Ok(())
    });
    registry.editor("edit.delete-word-right", |this, (), _, cx| {
        this.delete_with(cx, motion::word_right);
        Ok(())
    });
    registry.editor("edit.select-all", |this, (), _, cx| {
        this.select_all(cx);
        Ok(())
    });
    registry.editor("edit.newline", |this, (), _, cx| {
        this.newline(cx);
        Ok(())
    });
    registry.editor("search.go-to-matching-brace", |this, (), _, cx| {
        this.go_to_matching_brace(false, cx);
        Ok(())
    });
    registry.editor("search.select-to-matching-brace", |this, (), _, cx| {
        this.go_to_matching_brace(true, cx);
        Ok(())
    });
    registry.editor("edit.tab", |this, (), _, cx| {
        // A selection over several lines indents them, as in Notepad++.
        let text = this.text(cx).clone();
        let several_lines = this.selection.iter().any(|range| {
            !range.is_empty() && birchpad_core::ops::lines_of(&text, *range).len() > 1
        });
        if several_lines {
            this.indent(true, cx);
        } else {
            this.insert_tab(cx);
        }
        Ok(())
    });
    registry.editor("edit.indent", |this, (), _, cx| {
        this.indent(true, cx);
        Ok(())
    });
    registry.editor("edit.outdent", |this, (), _, cx| {
        this.indent(false, cx);
        Ok(())
    });
    registry.editor("edit.insert-text", |this, args: InsertTextArgs, _, cx| {
        this.insert(&args.text, LastEdit::None, cx);
        Ok(())
    });
    registry.editor("edit.type", |this, args: InsertTextArgs, _, cx| {
        this.type_text(&args.text, cx);
        Ok(())
    });
    registry.editor("edit.text-direction-rtl", |this, (), _, cx| {
        this.rtl = true;
        this.settings_changed(cx);
        Ok(())
    });
    registry.editor("edit.text-direction-ltr", |this, (), _, cx| {
        this.rtl = false;
        this.settings_changed(cx);
        Ok(())
    });
    registry.editor("edit.toggle-overwrite", |this, (), _, cx| {
        this.overwrite = !this.overwrite;
        this.pause_blink(cx);
        cx.notify();
        Ok(())
    });
    registry.editor("edit.convert-eol", |this, args: ConvertEolArgs, _, cx| {
        let target = match args.eol {
            EolName::Crlf => LineEnding::CrLf,
            EolName::Lf => LineEnding::Lf,
            EolName::Cr => LineEnding::Cr,
        };
        let transaction = this.buffer.read(cx).doc().convert_line_endings(target);
        this.apply(transaction, LastEdit::None, cx);
        Ok(())
    });
}

/// Which kind of edit was made last, to group consecutive typing into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastEdit {
    None,
    Typing,
    Deleting,
}

/// What a mouse drag extends by: characters, words (after a double click) or lines (triple).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone)]
enum Drag {
    /// Selecting text; `origin` is what the first click selected.
    Select {
        granularity: Granularity,
        origin: ByteRange<usize>,
    },
    /// Selecting a rectangle (Alt+drag) from `anchor`.
    Block {
        anchor: BlockPoint,
    },
    /// Dragging a scrollbar thumb; `grab` is where in the thumb it was grabbed.
    VerticalThumb {
        grab: Pixels,
    },
    HorizontalThumb {
        grab: Pixels,
    },
}

/// View menu switches kept in `state.toml`: command, current value, how to remember a new one.
pub type ViewSwitch = (
    &'static str,
    fn(&ViewSettings) -> bool,
    fn(&mut birchpad_config::UserState, bool),
);

pub const VIEW_SWITCHES: &[ViewSwitch] = &[
    (
        "view.word-wrap",
        |view| view.word_wrap,
        |state, on| state.word_wrap = Some(on),
    ),
    (
        "view.show-whitespace",
        |view| view.show_whitespace,
        |state, on| state.show_whitespace = Some(on),
    ),
    (
        "view.show-eol",
        |view| view.show_eol,
        |state, on| state.show_eol = Some(on),
    ),
    (
        "view.indent-guides",
        |view| view.indent_guides,
        |state, on| state.indent_guides = Some(on),
    ),
    (
        "view.wrap-symbol",
        |view| view.wrap_symbol,
        |state, on| state.wrap_symbol = Some(on),
    ),
];

/// View > Show Symbol > Show All Characters: whitespace and line ends both shown.
pub fn all_characters(view: &ViewSettings) -> bool {
    view.show_whitespace && view.show_eol
}

/// The view-related settings, read from the application state on every frame.
#[derive(Debug, Clone)]
pub struct ViewSettings {
    /// The theme's font, else the platform's monospace one.
    pub font_family: gpui_kit::SharedString,
    pub tab_width: usize,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub font_size: Pixels,
    pub line_numbers: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    /// The change history margin.
    pub change_history: bool,
    /// View > Show Symbol.
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub wrap_symbol: bool,
    pub current_line: CurrentLine,
    pub current_line_frame_width: Pixels,
    pub edge: Edge,
    pub edge_columns: Vec<usize>,
}

impl ViewSettings {
    pub fn read(cx: &App) -> Self {
        let app = AppState::global(cx);
        let editor = &app.settings.editor;
        let zoom = app.state.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        let theme_font = crate::theme::current().font.clone();
        Self {
            tab_width: usize::from(editor.tab_width.max(1)),
            insert_spaces: editor.insert_spaces,
            word_wrap: app.state.word_wrap.unwrap_or(editor.word_wrap),
            font_family: theme_font.family().map_or_else(
                || crate::MONOSPACE.into(),
                |family| family.to_owned().into(),
            ),
            font_size: px(theme_font.size().unwrap_or(BASE_FONT_SIZE) + zoom as f32),
            line_numbers: editor.line_numbers,
            bookmark_margin: editor.bookmark_margin,
            fold_margin: editor.fold_margin,
            change_history: editor.change_history == birchpad_config::ChangeHistory::Margin,
            show_whitespace: app.state.show_whitespace.unwrap_or(editor.show_whitespace),
            show_eol: app.state.show_eol.unwrap_or(editor.show_eol),
            indent_guides: app.state.indent_guides.unwrap_or(editor.indent_guides),
            wrap_symbol: app.state.wrap_symbol.unwrap_or(editor.wrap_symbol),
            current_line: editor.current_line,
            current_line_frame_width: px(f32::from(editor.current_line_frame_width.clamp(1, 6))),
            edge: if editor.edge_columns.is_empty() {
                Edge::Off
            } else {
                editor.edge
            },
            edge_columns: editor
                .edge_columns
                .iter()
                .map(|&c| usize::from(c))
                .collect(),
        }
    }

    /// The settings for a document in `language` (an id, `text`, or `udl:<name>`): its tab
    /// size and spaces from `languages.<id>` where set.
    pub fn for_language(language: &str, cx: &App) -> Self {
        let mut settings = Self::read(cx);
        if let Some(own) = AppState::global(cx).settings.languages.get(language) {
            if let Some(width) = own.tab_width {
                settings.tab_width = usize::from(width.max(1));
            }
            if let Some(spaces) = own.insert_spaces {
                settings.insert_spaces = spaces;
            }
        }
        settings
    }

    /// The settings for `buffer`'s document.
    pub fn for_buffer(buffer: &Buffer, cx: &App) -> Self {
        Self::for_language(&buffer.language_key(), cx)
    }

    /// The column where Split Lines breaks lines: the first edge column while the edge is
    /// shown, as in Notepad++.
    pub fn split_column(&self) -> Option<usize> {
        match self.edge {
            Edge::Off => None,
            Edge::Line | Edge::Background => self.edge_columns.first().copied(),
        }
    }
}

/// A word wrap layout being computed in the background for a large document.
struct WrapJob {
    config: LayoutConfig,
    revision: RevisionId,
    _task: Task<()>,
}

/// What a view tells the workspace.
pub enum EditorEvent {
    /// The view scrolled by this many rows and pixels since the last frame (for synchronized
    /// scrolling).
    Scrolled { rows: f64, x: Pixels },
    /// An item of the context menu was chosen: the command runs on this view's document.
    Command(birchpad_commands::Invocation),
}

/// Where a view is in its buffer: what Clone to Other View copies and sessions save.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub selection: Selection,
    pub scroll_top: f64,
    pub scroll_left: Pixels,
    /// First lines of the collapsed folds.
    pub collapsed: Vec<usize>,
    /// Edit > Text Direction RTL, remembered per tab in sessions.
    pub rtl: bool,
}

pub struct EditorView {
    pub focus_handle: FocusHandle,
    pub buffer: Entity<Buffer>,
    pub selection: Selection,
    /// Text being composed by an input method, as a byte range.
    pub marked: Option<ByteRange<usize>>,
    pub overwrite: bool,
    /// Edit > Text Direction RTL: rows run right to left and end at the right edge.
    pub rtl: bool,
    /// The completion list, while it shows.
    pub completion: Option<completion::Popup>,
    /// A function's parameters, while they show.
    pub call_tip: Option<completion::CallTip>,
    pub display: DisplayMap,
    wrap_job: Option<WrapJob>,
    /// Where to put the caret once the file has been read (`-n`, `-c`, `-p`).
    pending_caret: Option<CaretTarget>,
    /// A search result to select once the file has been read: a line, and the match's bytes
    /// from its start.
    pending_match: Option<(usize, ByteRange<usize>)>,
    /// Where to put the view once the file has been read (a restored session).
    pending_state: Option<ViewState>,
    /// Whether its tab is pinned, and its color.
    pub tab: crate::TabMarks,
    /// First visible row; fractional while scrolling smoothly.
    pub scroll_top: f64,
    /// Horizontal scroll offset of the text.
    pub scroll_left: Pixels,
    /// Widest content seen, for the horizontal scrollbar (as Scintilla tracks it).
    scroll_width: Pixels,
    /// Bring the primary caret into view on the next layout.
    pub autoscroll: bool,
    /// Column (in cells from the row start) that vertical movement aims for.
    goal_column: Option<usize>,
    last_edit: LastEdit,
    drag: Option<Drag>,
    caret_visible: bool,
    /// Whether the view has keyboard focus; the caret only blinks then.
    focused: bool,
    blink: Task<()>,
    /// Geometry of the last frame, for mouse and IME hit testing.
    pub layout: Option<Layout>,
    highlight_cache: Option<layout::HighlightCache>,
    /// Occurrences of the selected word around the visible text.
    pub smart_highlight: Option<smart_highlight::SmartHighlight>,
    /// Headers of this view's collapsed folds.
    collapsed: LineMarkers,
    /// View > Hide Lines: the lines hidden, each marked; shown again from the line above them.
    pub user_hidden: LineMarkers,
    /// The buffer's folds in lines.
    pub fold_cache: Option<folding::FoldCache>,
    /// The rectangular selection, valid while `selection` is still the one it made.
    block: Option<multi::BlockState>,
    /// The order in which Multi-select added ranges, valid while `selection` is still the one
    /// they make.
    multi_order: Option<(Vec<Range>, Selection)>,
    /// Begin/End Select: where the first invocation put the start, and in which mode.
    begin_select: Option<multi::BeginSelect>,
    /// The scroll position of the last frame, to report scrolling.
    last_scroll: Option<(f64, Pixels)>,
    /// The current scroll came from the other view: do not report it back.
    synced_scroll: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    pub fn new(buffer: Entity<Buffer>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&buffer, |this, buffer, event, cx| match event {
            BufferEvent::Edited {
                transaction,
                origin,
            } => {
                if *origin != Some(cx.entity_id()) {
                    let text = buffer.read(cx).doc().text().clone();
                    this.display.edit(&text, transaction.changes());
                    this.collapsed.map(transaction.changes(), &text);
                    this.follow_folds(transaction.changes(), cx);
                    this.sync_hidden(cx);
                    this.selection = this
                        .selection
                        .map(transaction.changes())
                        .clip_virtual(&text);
                    this.map_begin_select(transaction.changes());
                    this.marked = None;
                }
                cx.notify();
            }
            BufferEvent::Reloaded {
                previous: Some(previous),
            } => {
                this.follow_reload(previous, cx);
            }
            BufferEvent::FollowEnd => {
                let end = buffer.read(cx).doc().text().len();
                this.selection = Selection::point(end);
                this.request_autoscroll(cx);
            }
            BufferEvent::Reloaded { previous: None } => {
                let text = buffer.read(cx).doc().text().clone();
                this.display.reset(&text);
                this.collapsed.clear();
                this.fold_cache = None;
                // A new document starts its revisions over.
                this.smart_highlight = None;
                this.sync_hidden(cx);
                this.selection = Selection::point(0);
                this.begin_select = None;
                this.marked = None;
                this.goal_column = None;
                this.last_edit = LastEdit::None;
                this.scroll_top = 0.;
                this.scroll_left = px(0.);
                this.scroll_width = px(0.);
                if let Some(state) = this.pending_state.take() {
                    this.restore(state, cx);
                }
                if let Some(target) = this.pending_caret.take() {
                    this.place_caret(target, cx);
                }
                if let Some((line, target)) = this.pending_match.take() {
                    this.select_in_line(line, target, cx);
                }
                cx.notify();
            }
            BufferEvent::SyntaxChanged => {
                // New folds from the reparse: drop collapsed headers that no longer fold.
                this.prune_collapsed(cx);
                cx.notify();
            }
            BufferEvent::StateChanged | BufferEvent::MarksChanged | BufferEvent::LoadFailed(_) => {
                cx.notify()
            }
        });
        let focus_handle = cx.focus_handle();
        let text = buffer.read(cx).doc().text().clone();
        let settings = ViewSettings::for_buffer(buffer.read(cx), cx);
        let display = DisplayMap::new(
            &text,
            LayoutConfig {
                tab_width: settings.tab_width,
                wrap_width: None,
            },
        );
        Self {
            focus_handle,
            buffer,
            selection: Selection::point(0),
            marked: None,
            overwrite: false,
            rtl: false,
            completion: None,
            call_tip: None,
            display,
            wrap_job: None,
            pending_caret: None,
            pending_match: None,
            pending_state: None,
            tab: crate::TabMarks::default(),
            scroll_top: 0.,
            scroll_left: px(0.),
            scroll_width: px(0.),
            autoscroll: false,
            goal_column: None,
            last_edit: LastEdit::None,
            drag: None,
            caret_visible: true,
            focused: false,
            blink: Task::ready(()),
            layout: None,
            highlight_cache: None,
            smart_highlight: None,
            collapsed: LineMarkers::new(),
            user_hidden: LineMarkers::new(),
            fold_cache: None,
            block: None,
            multi_order: None,
            begin_select: None,
            last_scroll: None,
            synced_scroll: false,
            _subscriptions: vec![events],
        }
    }

    /// The file was read again: carets and selections stay at the same lines and columns,
    /// collapsed folds on the same lines, the scroll position as it was, as far as the new
    /// text has them.
    fn follow_reload(&mut self, previous: &Rope, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let last_line = motion::line_count(&text) - 1;
        let map = |pos: usize| {
            let line = line_of(previous, pos);
            let column = pos - line_range(previous, line).start;
            let range = line_range(&text, line.min(last_line));
            text.floor_char_boundary((range.start + column).min(range.end))
        };
        let ranges: Vec<Range> = self
            .selection
            .ranges()
            .iter()
            .map(|range| Range::new(map(range.anchor), map(range.head)))
            .collect();
        self.selection = Selection::new(ranges, self.selection.primary_index());
        let collapsed = self.collapsed.lines(previous);
        self.display.reset(&text);
        self.collapsed.set_lines(&text, collapsed);
        self.fold_cache = None;
        self.smart_highlight = None;
        self.sync_hidden(cx);
        self.block = None;
        self.multi_order = None;
        self.begin_select = None;
        self.marked = None;
        cx.notify();
    }

    /// Where the view is, or will be once its file has been read.
    pub fn view_state(&self, cx: &App) -> ViewState {
        if let Some(pending) = &self.pending_state {
            return pending.clone();
        }
        ViewState {
            selection: self.selection.clone(),
            scroll_top: self.scroll_top,
            scroll_left: self.scroll_left,
            collapsed: self.collapsed.lines(self.text(cx)),
            rtl: self.rtl,
        }
    }

    /// Puts the view where `state` says, now or, while the file is being read, once it has
    /// been. Positions past the end of the text (the file changed meanwhile) move to the end.
    pub fn restore(&mut self, state: ViewState, cx: &mut Context<Self>) {
        // The direction shows at once, the file read or not.
        self.rtl = state.rtl;
        if self.buffer.read(cx).read_only() == Some(ReadOnly::Loading) {
            self.pending_state = Some(state);
            return;
        }
        let text = self.text(cx).clone();
        let clamp = |pos: usize| {
            let pos = text.floor_char_boundary(pos.min(text.len()));
            // Not between the CR and the LF of a line break.
            if pos > 0 && text.get_byte(pos - 1) == Some(b'\r') && text.get_byte(pos) == Some(b'\n')
            {
                pos - 1
            } else {
                pos
            }
        };
        let ranges: Vec<Range> = state
            .selection
            .ranges()
            .iter()
            .map(|range| Range::new(clamp(range.anchor), clamp(range.head)))
            .collect();
        self.selection = Selection::new(ranges, state.selection.primary_index());
        self.scroll_top = state.scroll_top.max(0.);
        self.scroll_left = state.scroll_left.max(px(0.));
        self.collapsed.set_lines(&text, state.collapsed);
        self.autoscroll = false;
        cx.notify();
    }

    /// The last frame's layout.
    #[cfg(feature = "test-support")]
    pub fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    /// Where the text was drawn in the last frame.
    #[cfg(feature = "test-support")]
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.layout.as_ref().map(|layout| layout.text_bounds)
    }

    /// The first visible row and the horizontal scroll offset.
    #[cfg(feature = "test-support")]
    pub fn scroll_position(&self) -> (f64, Pixels) {
        (self.scroll_top, self.scroll_left)
    }

    /// Scrolls along with the other view (synchronized scrolling).
    pub fn scroll_by(&mut self, rows: f64, x: Pixels, cx: &mut Context<Self>) {
        self.scroll_top = (self.scroll_top + rows).max(0.);
        self.scroll_left = (self.scroll_left + x).max(px(0.));
        self.synced_scroll = true;
        cx.notify();
    }

    /// Reports how far the view scrolled since the last frame, unless the scroll came from the
    /// other view.
    fn report_scroll(&mut self, cx: &mut Context<Self>) {
        let position = (self.scroll_top, self.scroll_left);
        let synced = std::mem::take(&mut self.synced_scroll);
        if let Some(last) = self.last_scroll.replace(position)
            && last != position
            && !synced
        {
            cx.emit(EditorEvent::Scrolled {
                rows: position.0 - last.0,
                x: position.1 - last.1,
            });
        }
    }

    pub fn text<'a>(&self, cx: &'a App) -> &'a Rope {
        self.buffer.read(cx).doc().text()
    }

    pub fn run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invocation = &action.0;
        self.close_completion_for(&invocation.command);
        let handler = cx
            .global::<CommandRegistry>()
            .handler::<Self>(&invocation.command);
        match handler {
            Some(run) => match run(self, invocation, window, cx) {
                Ok(()) => (crate::EditorHooks::get(cx).ran)(invocation, cx),
                Err(error) => birchpad_ui::report_error(&error, window, cx),
            },
            None => cx.propagate(),
        }
    }

    /// Starts or stops blinking when the view gains or loses focus; called on every frame.
    fn track_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let focused = self.focus_handle.is_focused(window);
        if focused != self.focused {
            self.focused = focused;
            if focused {
                self.pause_blink(cx);
            } else {
                self.blink = Task::ready(());
            }
        }
    }

    /// Restarts the caret blink cycle with the caret visible, as after every key press.
    fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible = true;
        if !self.focused {
            return;
        }
        self.blink = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(CARET_BLINK).await;
                let blinking = this.update(cx, |this, cx| {
                    this.caret_visible = !this.caret_visible;
                    cx.notify();
                });
                if blinking.is_err() {
                    break;
                }
            }
        });
    }

    /// Zoom, word wrap or Show Symbol changed: keep the caret in view after the relayout.
    pub fn settings_changed(&mut self, cx: &mut Context<Self>) {
        self.scroll_width = px(0.);
        self.request_autoscroll(cx);
    }

    /// Brings the primary caret into view on the next frame, expanding folds that hide carets.
    fn request_autoscroll(&mut self, cx: &mut Context<Self>) {
        self.reveal_carets(cx);
        self.autoscroll = true;
        self.pause_blink(cx);
        cx.notify();
    }

    /// Rows that fit in the view, as of the last frame.
    fn page_rows(&self) -> isize {
        self.layout.as_ref().map_or(30, |layout| {
            layout.full_rows.saturating_sub(1).max(1) as isize
        })
    }

    /// Columns that fit across the text, as of the last frame.
    fn visible_columns(&self) -> Option<usize> {
        self.layout.as_ref().map(Layout::text_columns)
    }

    // --- Moving carets -------------------------------------------------------------------------

    fn move_carets(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let page = self.page_rows();
        let vertical = match motion {
            Motion::Up => Some(-1),
            Motion::Down => Some(1),
            Motion::PageUp => Some(-page),
            Motion::PageDown => Some(page),
            _ => None,
        };
        let primary_goal = match vertical {
            Some(_) => {
                let goal = self.goal_column.unwrap_or_else(|| {
                    self.display
                        .column_in_row(&text, self.selection.primary().head)
                });
                self.goal_column = Some(goal);
                Some(goal)
            }
            None => {
                self.goal_column = None;
                None
            }
        };
        if matches!(motion, Motion::DocumentStart | Motion::DocumentEnd) && !extend {
            self.selection = Selection::point(self.selection.primary().head);
        }
        let single = self.selection.ranges().len() == 1;
        let forward = matches!(motion, Motion::Right | Motion::WordRight);
        let horizontal = matches!(
            motion,
            Motion::Left | Motion::Right | Motion::WordLeft | Motion::WordRight
        );
        let display = &mut self.display;
        self.selection = self.selection.transform(|range| {
            let head = match (motion, vertical) {
                (_, Some(rows)) => {
                    let goal = match primary_goal {
                        Some(goal) if single => goal,
                        _ => display.column_in_row(&text, range.head),
                    };
                    display.move_by_rows(&text, range.head, rows, goal)
                }
                (Motion::Left, _) if !extend && !range.is_empty() => range.from(),
                (Motion::Right, _) if !extend && !range.is_empty() => range.to(),
                (Motion::Left, _) => motion::prev_boundary(&text, range.head),
                (Motion::Right, _) => motion::next_boundary(&text, range.head),
                (Motion::WordLeft, _) => motion::word_left(&text, range.head),
                (Motion::WordRight, _) => motion::word_right(&text, range.head),
                (Motion::Home, _) => smart_home(display, &text, range.head),
                (Motion::End, _) => line_range(&text, line_of(&text, range.head)).end,
                (Motion::DocumentStart, _) => 0,
                (Motion::DocumentEnd, _) => text.len(),
                _ => range.head,
            };
            if extend {
                Range::new(range.anchor, head)
            } else {
                Range::point(head)
            }
        });
        // Moving left or right into a collapsed fold steps over it.
        if horizontal {
            self.selection = self.selection.transform(|range| {
                let head = self.skip_hidden(&text, range.head, forward);
                if extend {
                    Range::new(range.anchor, head)
                } else {
                    Range::point(head)
                }
            });
        }
        // PageUp/PageDown scroll the view by the same amount, as in Notepad++.
        if let Some(rows) = vertical
            && rows.abs() > 1
        {
            self.scroll_top = (self.scroll_top + rows as f64).max(0.);
        }
        self.last_edit = LastEdit::None;
        self.request_autoscroll(cx);
    }

    /// Column of `pos` counted in cells from the line start (tabs to their stops).
    pub fn column_of(&mut self, pos: usize, cx: &App) -> usize {
        let text = self.text(cx).clone();
        self.display.column(&text, pos)
    }

    /// Puts the caret where the command line asked, now or once the file has been read.
    pub fn set_caret_target(&mut self, target: CaretTarget, cx: &mut Context<Self>) {
        if self.buffer.read(cx).read_only() == Some(ReadOnly::Loading) {
            self.pending_caret = Some(target);
        } else {
            self.place_caret(target, cx);
        }
    }

    fn place_caret(&mut self, target: CaretTarget, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let pos = match target {
            CaretTarget::Position(offset) => birchpad_core::motion::go_to_position(
                &text,
                birchpad_core::motion::GoToTarget::Offset(offset),
            ),
            CaretTarget::LineColumn { line, column } => {
                let start = birchpad_core::motion::go_to_position(
                    &text,
                    birchpad_core::motion::GoToTarget::Line(line),
                );
                let line_range = line_range(&text, line_of(&text, start));
                let tab_width = self.display.config().tab_width;
                birchpad_view::pos_at_column(&text, line_range, 0, column - 1, tab_width).0
            }
        };
        self.go_to(pos, cx);
    }

    /// Selects `range` (a search match) and scrolls it into view.
    pub fn select_range(&mut self, range: ByteRange<usize>, cx: &mut Context<Self>) {
        self.selection = Selection::single(Range::new(range.start, range.end));
        self.goal_column = None;
        self.last_edit = LastEdit::None;
        self.request_autoscroll(cx);
    }

    /// Selects a search result: `target` holds bytes from the start of the 0-based `line`. A
    /// line past the end is the last line, and a target past the line's end or inside a
    /// character is cut to fit, since the text may have changed since the search. Waits for a
    /// file that is still being read.
    pub fn select_in_line(
        &mut self,
        line: usize,
        target: ByteRange<usize>,
        cx: &mut Context<Self>,
    ) {
        if self.buffer.read(cx).read_only() == Some(ReadOnly::Loading) {
            self.pending_match = Some((line, target));
            return;
        }
        let text = self.text(cx).clone();
        let line = line.min(motion::line_count(&text).saturating_sub(1));
        let start = line_range(&text, line).start;
        let from = text.floor_char_boundary((start + target.start).min(text.len()));
        let to = text.floor_char_boundary((start + target.end).min(text.len()));
        self.select_range(from..to.max(from), cx);
    }

    /// The lines the last frame showed, the last one partly perhaps.
    pub fn visible_lines(&self) -> Option<std::ops::Range<usize>> {
        let layout = self.layout.as_ref()?;
        let first = layout.rows.first()?.row.line;
        let last = layout.rows.last()?.row.line;
        Some(first..last + 1)
    }

    /// Scrolls so that `line` is the first one shown, or the first shown of its fold (the
    /// Document Map).
    pub fn scroll_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let line = line.min(motion::line_count(&text).saturating_sub(1));
        let visible = self.display.visible_line(line);
        self.scroll_top = self.display.first_row_of_line(visible) as f64;
        cx.notify();
    }

    /// The view settings for this document's language.
    pub fn view_settings(&self, cx: &App) -> ViewSettings {
        ViewSettings::for_buffer(self.buffer.read(cx), cx)
    }

    pub fn tab_width(&self) -> usize {
        self.display.config().tab_width
    }

    /// Puts a single caret at `pos` and scrolls to it (Go To).
    pub fn go_to(&mut self, pos: usize, cx: &mut Context<Self>) {
        self.select_range(pos..pos, cx);
    }

    /// Applies an edit made on behalf of the user by a command (Replace All) as one undo step.
    /// Returns false if the document is read-only.
    pub fn apply_command_edit(&mut self, transaction: Transaction, cx: &mut Context<Self>) -> bool {
        if !self.is_editable(cx) {
            return false;
        }
        self.apply(transaction, LastEdit::None, cx);
        true
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::single(Range::new(0, self.text(cx).len()));
        self.last_edit = LastEdit::None;
        self.pause_blink(cx);
        cx.notify();
    }

    // --- Editing -------------------------------------------------------------------------------

    /// False while the buffer is loading, has a decoding problem or is a read-only file.
    pub fn is_editable(&self, cx: &App) -> bool {
        self.buffer.read(cx).read_only().is_none()
    }

    fn apply(&mut self, transaction: Transaction, kind: LastEdit, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let grouping = if kind != LastEdit::None && kind == self.last_edit {
            UndoGrouping::MergeWithPrevious
        } else {
            UndoGrouping::NewStep
        };
        let before = self.selection.clone();
        let selection = match transaction.selection() {
            Some(selection) => selection.clone(),
            None => before.map(transaction.changes()),
        };
        self.map_begin_select(transaction.changes());
        let changes = transaction.changes().clone();
        let origin = Some(cx.entity_id());
        self.buffer.update(cx, |buffer, cx| {
            buffer.apply(transaction, &before, grouping, origin, cx);
        });
        let text = self.text(cx).clone();
        self.display.edit(&text, &changes);
        self.collapsed.map(&changes, &text);
        self.user_hidden.map(&changes, &text);
        self.follow_folds(&changes, cx);
        self.sync_hidden(cx);
        self.selection = selection.clip_virtual(&text);
        self.last_edit = kind;
        self.goal_column = None;
        self.request_autoscroll(cx);
    }

    /// Replaces every selection range with `text`. In a rectangular selection, every line gets `text`, and the rectangle stays as a caret
    /// on each line unless `text` breaks lines.
    pub fn insert(&mut self, text: &str, kind: LastEdit, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let block = self.active_block();
        let transaction = Transaction::replace_selections(self.text(cx), &self.selection, text)
            .expect("the selection always lies on character boundaries of the document");
        self.apply(transaction, kind, cx);
        if let Some(block) = block
            && !text.contains(['\r', '\n'])
        {
            self.keep_thin_block(block, cx);
        }
    }

    /// Text typed by the user. In overwrite mode each typed character replaces the next one,
    /// up to the end of the line.
    fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) {
        if self.type_paired(typed, cx) {
            self.after_typing(typed, cx);
            return;
        }
        if self.overwrite && !typed.contains(['\r', '\n']) && self.is_editable(cx) {
            let text = self.text(cx).clone();
            let count = typed.chars().count();
            self.selection = self.selection.transform(|range| {
                if !range.is_empty() {
                    return range;
                }
                let end = line_range(&text, line_of(&text, range.head)).end;
                let mut to = range.head;
                for _ in 0..count {
                    if to >= end {
                        break;
                    }
                    to = motion::next_boundary(&text, to);
                }
                if to == range.head {
                    return range;
                }
                Range::new(range.head, to)
            });
        }
        self.insert(typed, LastEdit::Typing, cx);
        self.after_typing(typed, cx);
    }

    /// Enter: a line break, indented as `editor.auto-indent` says.
    fn newline(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let buffer = self.buffer.read(cx);
        let text = buffer.doc().text().clone();
        let eol = buffer.doc().line_ending().as_str();
        let python = buffer
            .language()
            .is_some_and(|language| language.id == "python");
        let mode = AppState::global(cx).settings.editor.auto_indent;
        let settings = self.view_settings(cx);
        let unit = if settings.insert_spaces {
            " ".repeat(settings.tab_width)
        } else {
            "\t".to_owned()
        };
        let mut edits = Vec::new();
        let mut carets = Vec::new();
        let mut shift: isize = 0;
        for range in self.selection.ranges() {
            let (from, to) = (range.from(), range.to());
            let line = line_of(&text, from);
            let line_start = line_range(&text, line).start;
            let indent = match mode {
                AutoIndent::Off => String::new(),
                _ => {
                    let end = motion::indent_end(&text, line).min(from);
                    text.slice(line_start..end).to_string()
                }
            };
            let mut inserted = format!("{eol}{indent}");
            let mut caret = inserted.len();
            if mode == AutoIndent::Advanced {
                let prefix = text.slice(line_start..from);
                let before = prefix
                    .chars_at(prefix.len())
                    .reversed()
                    .find(|c| !c.is_whitespace());
                let opens =
                    matches!(before, Some('{' | '[' | '(')) || (python && before == Some(':'));
                if opens {
                    let inner = format!("{eol}{indent}{unit}");
                    let after = text.slice(to..).chars().next();
                    let closes = matches!(
                        (before, after),
                        (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')'))
                    );
                    caret = inner.len();
                    // Enter between a pair of brackets: the closing one goes on its own line.
                    inserted = if closes {
                        format!("{inner}{eol}{indent}")
                    } else {
                        inner
                    };
                }
            }
            let start = (from as isize + shift) as usize;
            carets.push(Range::point(start + caret));
            shift += inserted.len() as isize - (to - from) as isize;
            edits.push(Edit::replace(from..to, inserted));
        }
        let selection = Selection::new(carets, self.selection.primary_index());
        let transaction = Transaction::from_edits(&text, edits)
            .expect("the selection lies on character boundaries")
            .with_selection(selection);
        self.apply(transaction, LastEdit::Typing, cx);
    }

    /// Ctrl+B: to the bracket matching the one at the caret; with `select`, selects both
    /// brackets and everything between them (Ctrl+Alt+B), as in Notepad++.
    fn go_to_matching_brace(&mut self, select: bool, cx: &mut Context<Self>) {
        let buffer = self.buffer.read(cx);
        let text = buffer.doc().text();
        let head = self.selection.primary().head;
        let Some(found) = birchpad_syntax::matching_bracket(text, buffer.syntax(), head) else {
            return;
        };
        let Some(partner) = found.partner else {
            return;
        };
        if select {
            let from = found.bracket.start.min(partner.start);
            let to = found.bracket.end.max(partner.end);
            self.select_range(from..to, cx);
        } else {
            self.go_to(partner.start, cx);
        }
    }

    /// Increase (`more`) or Decrease Line Indent: the lines of every selection, by a tab or by
    /// `editor.tab-width` spaces with `editor.insert-spaces`.
    fn indent(&mut self, more: bool, cx: &mut Context<Self>) {
        let settings = self.view_settings(cx);
        let unit = if settings.insert_spaces {
            " ".repeat(settings.tab_width.max(1))
        } else {
            "	".to_owned()
        };
        let width = settings.tab_width;
        self.run_op(cx, move |text, selection, _| {
            if more {
                birchpad_core::ops::indent_lines(text, selection, &unit)
            } else {
                birchpad_core::ops::outdent_lines(text, selection, width)
            }
        });
    }

    /// Tab: a tab character, or spaces up to the next tab stop with `editor.insert-spaces`.
    fn insert_tab(&mut self, cx: &mut Context<Self>) {
        let settings = self.view_settings(cx);
        if !settings.insert_spaces {
            self.insert("\t", LastEdit::Typing, cx);
            return;
        }
        if !self.is_editable(cx) {
            return;
        }
        let text = self.text(cx).clone();
        let display = &mut self.display;
        let transaction = Transaction::replace_selections_with(&text, &self.selection, |range| {
            // In virtual space, from the column past the line end.
            let column = display.column(&text, range.from()) + range.from_virtual();
            " ".repeat(tab_advance(column, settings.tab_width))
        })
        .expect("the selection lies on character boundaries");
        self.apply(transaction, LastEdit::Typing, cx);
    }

    /// Replaces one range (used by input methods, which only work at the primary caret) and
    /// puts the primary caret after it; the other carets follow the edit.
    fn replace_range(&mut self, range: ByteRange<usize>, text: &str, cx: &mut Context<Self>) {
        let caret = Range::point(range.start + text.len());
        let Ok(transaction) =
            Transaction::from_edits(self.text(cx), [Edit::replace(range.clone(), text)])
        else {
            return;
        };
        // The primary range, wherever mapping puts it, becomes the caret after the new text.
        let mapped = self.selection.map(transaction.changes());
        let primary = mapped.primary_index();
        let mut ranges = mapped.ranges().to_vec();
        ranges[primary] = caret;
        let selection = Selection::new(ranges, primary);
        self.apply(transaction.with_selection(selection), LastEdit::Typing, cx);
    }

    /// Deletes every selection; empty ones first extend to `target(head)` (the previous
    /// character, the next word, ...).
    fn delete_with(&mut self, cx: &mut Context<Self>, target: fn(&Rope, usize) -> usize) {
        if !self.is_editable(cx) {
            return;
        }
        if self.active_block().is_some() {
            self.delete_in_block(cx, target);
            return;
        }
        let text = self.text(cx).clone();
        self.selection = self.selection.transform(|range| {
            if range.is_empty() {
                Range::new(range.head, target(&text, range.head))
            } else {
                range
            }
        });
        self.insert("", LastEdit::Deleting, cx);
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let origin = Some(cx.entity_id());
        if let Some(transaction) = self.buffer.update(cx, |buffer, cx| buffer.undo(origin, cx)) {
            self.restore_from_history(&transaction, cx);
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let origin = Some(cx.entity_id());
        if let Some(transaction) = self.buffer.update(cx, |buffer, cx| buffer.redo(origin, cx)) {
            self.restore_from_history(&transaction, cx);
        }
    }

    fn restore_from_history(&mut self, transaction: &Transaction, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        self.display.edit(&text, transaction.changes());
        self.collapsed.map(transaction.changes(), &text);
        self.follow_folds(transaction.changes(), cx);
        self.sync_hidden(cx);
        self.selection = match transaction.selection() {
            Some(selection) => selection.clone(),
            None => self.selection.map(transaction.changes()),
        }
        .clip_virtual(&text);
        self.map_begin_select(transaction.changes());
        self.last_edit = LastEdit::None;
        self.marked = None;
        self.request_autoscroll(cx);
    }

    pub fn selected_text(&self, cx: &App) -> Option<String> {
        let doc = self.buffer.read(cx).doc();
        let parts: Vec<String> = self
            .selection
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| doc.text().slice(range.from()..range.to()).to_string())
            .collect();
        (!parts.is_empty()).then(|| parts.join(doc.line_ending().as_str()))
    }

    /// Copies the selection; a rectangle goes to the clipboard as its rows, each ended by a
    /// line break, marked as rectangular for pasting it back as a rectangle.
    fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        if self.active_block().is_some() {
            let item = self.block_clipboard_item(cx);
            cx.write_to_clipboard(item);
            return true;
        }
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return true;
        }
        false
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        if self.copy(cx) {
            self.insert("", LastEdit::None, cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else {
            return;
        };
        match multi::rectangle_rows(&item, &text) {
            Some(rows) => self.paste_rectangle(&rows, cx),
            None => self.insert(&text, LastEdit::None, cx),
        }
    }

    // --- Scrolling -----------------------------------------------------------------------------

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let line_height = layout.metrics.line_height;
        if event.modifiers.secondary() {
            // Ctrl+wheel zooms, as in Notepad++.
            let delta = event.delta.pixel_delta(line_height).y;
            if delta != px(0.) {
                let step = if delta > px(0.) { 1 } else { -1 };
                AppState::update_state(cx, |state, _| {
                    state.zoom = (state.zoom + step).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
                });
                self.settings_changed(cx);
                cx.refresh_windows();
            }
            return;
        }
        let mut delta = event.delta.pixel_delta(line_height);
        if event.modifiers.shift && delta.x == px(0.) {
            delta = point(delta.y, px(0.));
        }
        self.scroll_top = (self.scroll_top - f64::from(delta.y / line_height)).max(0.);
        self.scroll_left = (self.scroll_left - delta.x).max(px(0.));
        cx.notify();
    }

    // --- Mouse ---------------------------------------------------------------------------------

    fn with_primary(&self, range: Range) -> Selection {
        let mut ranges = self.selection.ranges().to_vec();
        let primary = self.selection.primary_index();
        ranges[primary] = range;
        Selection::new(ranges, primary)
    }

    /// The range a click at `pos` selects at `granularity`.
    fn unit_at(text: &Rope, pos: usize, granularity: Granularity) -> ByteRange<usize> {
        match granularity {
            Granularity::Char => pos..pos,
            Granularity::Word => motion::word_at(text, pos),
            Granularity::Line => line_range_with_break(text, line_of(text, pos)),
        }
    }

    /// A Ctrl+click on a link opens it, and a double click too with `links.double-click`;
    /// Ctrl+double click selects between the delimiters around the click. Whether the click was
    /// taken.
    fn link_or_delimiters(
        &mut self,
        event: &MouseDownEvent,
        pos: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let settings = AppState::global(cx).settings.clone();
        let secondary = event.modifiers.secondary();
        let text = self.text(cx).clone();
        let opens = (event.click_count == 1 && secondary)
            || (event.click_count == 2 && !secondary && settings.links.double_click);
        if settings.links.enabled
            && opens
            && let Some(link) =
                birchpad_core::links::link_at(&text, pos, &settings.links.extra_schemes())
        {
            let url = text.slice(link).to_string();
            (crate::EditorHooks::get(cx).open_link)(url, cx);
            self.drag = None;
            return true;
        }
        if secondary
            && event.click_count == 2
            && let Some((open, close)) = settings.delimiters.pair()
            && let Some(range) = birchpad_core::links::between_delimiters(
                &text,
                pos,
                open,
                close,
                settings.delimiters.multi_line,
            )
        {
            self.selection = Selection::single(Range::new(range.start, range.end));
            self.drag = None;
            self.goal_column = None;
            cx.notify();
            return true;
        }
        false
    }

    /// A right click: the caret goes there, unless the click is in a selection (the context
    /// menu then acts on it).
    fn right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.completion = None;
        self.call_tip = None;
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let in_selection = self
            .selection
            .iter()
            .any(|range| !range.is_empty() && range.from() <= pos && pos <= range.to());
        if !in_selection {
            self.go_to(pos, cx);
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.completion = None;
        self.call_tip = None;
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        if let Some(bar) = &layout.vertical_bar
            && bar.track.contains(&event.position)
        {
            let grab = if bar.thumb.contains(&event.position) {
                event.position.y - bar.thumb.top()
            } else {
                bar.thumb.size.height / 2.
            };
            self.drag = Some(Drag::VerticalThumb { grab });
            self.drag_scrollbar(event.position, cx);
            return;
        }
        if let Some(bar) = &layout.horizontal_bar
            && bar.track.contains(&event.position)
        {
            let grab = if bar.thumb.contains(&event.position) {
                event.position.x - bar.thumb.left()
            } else {
                bar.thumb.size.width / 2.
            };
            self.drag = Some(Drag::HorizontalThumb { grab });
            self.drag_scrollbar(event.position, cx);
            return;
        }
        // A click in the margins of a line with hidden lines below shows them again.
        if layout.margins.gutter.contains(&event.position)
            && let Some(line) = layout
                .row_at_y(event.position.y)
                .map(|row| row.row.line)
                .filter(|line| layout.hidden_marks.iter().any(|(anchor, _)| anchor == line))
        {
            self.show_hidden_below(line, cx);
            return;
        }
        if let Some(margin) = layout.margins.folding
            && margin.contains(&event.position)
        {
            // A click on a fold point toggles it.
            let header = layout
                .row_at_y(event.position.y)
                .filter(|row| row.row.index_in_line == 0)
                .map(|row| row.row.line);
            if let Some(line) = header {
                self.toggle_fold(line, cx);
            }
            return;
        }
        if let Some(margin) = layout.margins.symbols
            && margin.contains(&event.position)
        {
            // A click in the symbol margin toggles the bookmark, as in Notepad++.
            if let Some(line) = layout.row_at_y(event.position.y).map(|row| row.row.line) {
                self.buffer.update(cx, |buffer, cx| {
                    buffer.update_marks(cx, |marks, text| {
                        marks.bookmarks.toggle(text, line);
                    });
                });
            }
            return;
        }
        // In the line number margin, clicking and dragging selects whole lines; in the change
        // history margin too, as in Scintilla's margins that take no clicks of their own.
        let in_line_numbers = [layout.margins.line_numbers, layout.margins.changes]
            .into_iter()
            .flatten()
            .any(|margin| margin.contains(&event.position));
        let text = self.text(cx).clone();
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let granularity = match event.click_count {
            _ if in_line_numbers => Granularity::Line,
            0 | 1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        };
        if event.modifiers.alt
            && !event.modifiers.shift
            && !event.modifiers.secondary()
            && granularity == Granularity::Char
            && let Some(anchor) = self.block_point_for_point(event.position, cx)
        {
            // Alt+drag selects a rectangle.
            self.set_block(Block::new(anchor, anchor), cx);
            self.drag = Some(Drag::Block { anchor });
            return;
        }
        if !in_line_numbers && !event.modifiers.shift && self.link_or_delimiters(event, pos, cx) {
            return;
        }
        let unit = Self::unit_at(&text, pos, granularity);
        self.selection = if event.modifiers.shift {
            self.with_primary(Range::new(self.selection.primary().anchor, pos))
        } else if event.modifiers.secondary() && granularity == Granularity::Char {
            self.selection.clone().push(Range::point(pos))
        } else {
            Selection::single(Range::new(unit.start, unit.end))
        };
        self.drag = Some(Drag::Select {
            granularity,
            origin: unit,
        });
        self.goal_column = None;
        self.last_edit = LastEdit::None;
        self.pause_blink(cx);
        cx.notify();
    }

    pub fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        // Dragging past the top or bottom scrolls.
        if matches!(drag, Drag::Select { .. } | Drag::Block { .. })
            && let Some(layout) = &self.layout
        {
            if event.position.y < layout.text_bounds.top() {
                self.scroll_top = (self.scroll_top - 1.).max(0.);
            } else if event.position.y > layout.text_bounds.bottom() {
                self.scroll_top += 1.;
            }
        }
        if let Drag::Block { anchor } = drag {
            if let Some(head) = self.block_point_for_point(event.position, cx) {
                self.set_block(Block::new(anchor, head), cx);
            }
            return;
        }
        let Drag::Select {
            granularity,
            origin,
        } = drag
        else {
            self.drag_scrollbar(event.position, cx);
            return;
        };
        let text = self.text(cx).clone();
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let unit = Self::unit_at(&text, pos, granularity);
        let range = if pos < origin.start {
            Range::new(origin.end, unit.start)
        } else {
            Range::new(origin.start, unit.end.max(origin.end))
        };
        self.selection = self.with_primary(range);
        // Dragging past the left or right edge scrolls the caret into view, a few columns at a
        // time, so the selection goes on past the visible part of a long line.
        if let Some(layout) = &self.layout
            && (event.position.x < layout.text_bounds.left()
                || event.position.x > layout.text_bounds.right())
        {
            self.autoscroll = true;
        }
        self.pause_blink(cx);
        cx.notify();
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
    }

    fn drag_scrollbar(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        match self.drag {
            Some(Drag::VerticalThumb { grab }) => {
                if let Some(bar) = &layout.vertical_bar {
                    let free = bar.track.size.height - bar.thumb.size.height;
                    let fraction = if free > px(0.) {
                        ((position.y - grab - bar.track.top()) / free).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let max = layout.total_rows.saturating_sub(layout.full_rows) as f64;
                    self.scroll_top = f64::from(fraction) * max;
                }
            }
            Some(Drag::HorizontalThumb { grab }) => {
                if let Some(bar) = &layout.horizontal_bar {
                    let free = bar.track.size.width - bar.thumb.size.width;
                    let fraction = if free > px(0.) {
                        ((position.x - grab - bar.track.left()) / free).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let max = (self.scroll_width - layout.text_bounds.size.width).max(px(0.));
                    self.scroll_left = max * fraction;
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// The text position under a window point, using the last frame's layout.
    fn position_for_point(&mut self, point: Point<Pixels>, cx: &App) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let text = self.buffer.read(cx).doc().text();
        layout.position_for_point(point, text, &mut self.display)
    }

    /// The rectangle corner under a window point: past the end of a line, in virtual space.
    fn block_point_for_point(&mut self, point: Point<Pixels>, cx: &App) -> Option<BlockPoint> {
        let layout = self.layout.as_ref()?;
        let text = self.buffer.read(cx).doc().text();
        layout.block_point_for_point(point, text, &mut self.display)
    }
}

/// Home: to the first non-blank character of the line, or to the line start if already there
/// (Scintilla's VCHOMEWRAP); in a wrapped continuation row, first to the row start.
fn smart_home(display: &mut DisplayMap, text: &Rope, head: usize) -> usize {
    let (_, row) = display.row_of(text, head);
    if row.index_in_line > 0 && head != row.range.start {
        return row.range.start;
    }
    let line = line_of(text, head);
    let indent = motion::indent_end(text, line);
    if head == indent {
        line_range(text, line).start
    } else {
        indent
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let buffer = self.buffer.read(cx);
        let banner = match buffer.read_only() {
            Some(ReadOnly::Decoding(problem)) => {
                let encoding = birchpad_io::display_name(buffer.doc().format().encoding, false);
                Some(crate::banner::decoding_problem(
                    &encoding,
                    problem,
                    buffer.decode_changes().count,
                    self.focus_handle.clone(),
                ))
            }
            Some(ReadOnly::File) => Some(crate::banner::read_only_file()),
            Some(ReadOnly::Requested) => Some(crate::banner::read_only_requested()),
            Some(ReadOnly::Monitoring) => Some(crate::banner::monitoring()),
            Some(ReadOnly::Loading) | None => None,
        };
        let loading = buffer.loading_progress();

        div()
            .id("editor")
            .key_context(self.key_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paint(theme::editor().background))
            .text_color(theme::paint(theme::editor().text))
            .children(banner)
            .child(
                div()
                    .id("text")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::right_mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_scroll_wheel(cx.listener(Self::scroll_wheel))
                    .child(EditorElement::new(cx.entity()))
                    .children(self.render_completion(cx))
                    .children(loading.map(crate::banner::loading))
                    .context_menu({
                        let view = cx.entity().downgrade();
                        move |menu, window, cx| {
                            (crate::EditorHooks::get(cx).context_menu)(menu, &view, window, cx)
                        }
                    }),
            )
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: ByteRange<usize>,
        adjusted_range: &mut Option<ByteRange<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.bytes_from_utf16(&range_utf16, cx);
        adjusted_range.replace(self.utf16_from_bytes(&range, cx));
        Some(self.text(cx).slice(range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let primary = self.selection.primary();
        Some(UTF16Selection {
            range: self.utf16_from_bytes(&(primary.from()..primary.to()), cx),
            reversed: primary.is_backward(),
        })
    }

    fn marked_text_range(
        &self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ByteRange<usize>> {
        self.marked
            .as_ref()
            .map(|range| self.utf16_from_bytes(range, cx))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<ByteRange<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = range_utf16
            .map(|range| self.bytes_from_utf16(&range, cx))
            .or(self.marked.take());
        self.marked = None;
        match target {
            Some(range) => self.replace_range(range, text, cx),
            None => self.type_text(text, cx),
        }
        (crate::EditorHooks::get(cx).typed)(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<ByteRange<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<ByteRange<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_editable(cx) {
            return;
        }
        let target = range_utf16
            .map(|range| self.bytes_from_utf16(&range, cx))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| {
                let primary = self.selection.primary();
                primary.from()..primary.to()
            });
        self.replace_range(target.clone(), new_text, cx);
        self.marked = (!new_text.is_empty()).then(|| target.start..target.start + new_text.len());
        if let Some(selected) = new_selected_range_utf16 {
            let anchor = target.start + utf16_to_byte_offset(new_text, selected.start);
            let head = target.start + utf16_to_byte_offset(new_text, selected.end);
            self.selection = self.with_primary(Range::new(anchor, head));
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: ByteRange<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.bytes_from_utf16(&range_utf16, cx);
        let layout = self.layout.as_ref()?;
        layout.bounds_for_range(range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let pos = self.position_for_point(point, cx)?;
        Some(self.text(cx).byte_to_utf16_idx(pos))
    }
}

impl EditorView {
    fn utf16_from_bytes(&self, range: &ByteRange<usize>, cx: &App) -> ByteRange<usize> {
        let text = self.text(cx);
        text.byte_to_utf16_idx(range.start)..text.byte_to_utf16_idx(range.end)
    }

    fn bytes_from_utf16(&self, range: &ByteRange<usize>, cx: &App) -> ByteRange<usize> {
        let text = self.text(cx);
        let len = text.len_utf16();
        text.utf16_to_byte_idx(range.start.min(len))..text.utf16_to_byte_idx(range.end.min(len))
    }
}

/// Byte offset in `text` of the given UTF-16 code unit offset.
fn utf16_to_byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}
