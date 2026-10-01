//! The editor view: one document, its selection, and the glue between them and GPUI.
//!
//! Phase 0 spike limits: one document per window, no horizontal scrolling, tabs drawn as single
//! spaces, UTF-8 only, no saving.

use std::collections::HashMap;
use std::ops::Range as ByteRange;
use std::path::PathBuf;
use std::time::Duration;

use birchpad_core::{
    Document, Edit, LineEnding, LineType, Range, Selection, Transaction, UndoGrouping,
};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollStrategy, ShapedLine, UTF16Selection, UniformListScrollHandle, Window,
    canvas, div, point, prelude::*, px, rgb, uniform_list,
};

use crate::line_element::LineElement;

/// Line breaks recognized by the editor: CRLF, LF and CR, as in Notepad++.
const LINES: LineType = LineType::LF_CR;
const LINE_HEIGHT: f32 = 20.;
/// Lines moved by PageUp/PageDown until the view reports its real height.
const PAGE_LINES: isize = 30;

const MONOSPACE: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

gpui_kit::actions!(
    editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        Home,
        End,
        SelectHome,
        SelectEnd,
        DocumentStart,
        DocumentEnd,
        PageUp,
        PageDown,
        SelectAll,
        Newline,
        InsertTab,
        Undo,
        Redo,
        Copy,
        Cut,
        Paste,
        Quit,
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some("Editor");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("shift-home", SelectHome, context),
        KeyBinding::new("shift-end", SelectEnd, context),
        KeyBinding::new("secondary-home", DocumentStart, context),
        KeyBinding::new("secondary-end", DocumentEnd, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("secondary-a", SelectAll, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("tab", InsertTab, context),
        KeyBinding::new("secondary-z", Undo, context),
        KeyBinding::new("secondary-y", Redo, context),
        KeyBinding::new("secondary-shift-z", Redo, context),
        KeyBinding::new("secondary-c", Copy, context),
        KeyBinding::new("secondary-x", Cut, context),
        KeyBinding::new("secondary-v", Paste, context),
        KeyBinding::new("secondary-q", Quit, None),
    ]);
}

/// How the document was opened, for the title and status bar.
pub(crate) struct LoadInfo {
    pub(crate) path: Option<PathBuf>,
    pub(crate) load_time: Duration,
    /// The file was not valid UTF-8 and invalid bytes were replaced.
    pub(crate) lossy: bool,
}

/// Which kind of edit was made last, to group consecutive typing into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastEdit {
    None,
    Typing,
    Deleting,
}

/// A line painted in the current frame, kept for mouse and IME hit testing.
pub(crate) struct VisibleLine {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) shaped: ShapedLine,
    pub(crate) start: usize,
}

pub(crate) struct Editor {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) doc: Document,
    pub(crate) selection: Selection,
    /// Text being composed by an input method, as a byte range.
    pub(crate) marked: Option<ByteRange<usize>>,
    pub(crate) visible_lines: HashMap<usize, VisibleLine>,
    scroll: UniformListScrollHandle,
    info: LoadInfo,
    preferred_column: Option<usize>,
    last_edit: LastEdit,
    mouse_selecting: bool,
    title: String,
}

impl Editor {
    pub(crate) fn new(
        doc: Document,
        info: LoadInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            focus_handle,
            doc,
            selection: Selection::point(0),
            marked: None,
            visible_lines: HashMap::new(),
            scroll: UniformListScrollHandle::new(),
            info,
            preferred_column: None,
            last_edit: LastEdit::None,
            mouse_selecting: false,
            title: String::new(),
        }
    }

    // --- Text geometry -------------------------------------------------------------------------

    fn line_count(&self) -> usize {
        self.doc.text().len_lines(LINES)
    }

    pub(crate) fn line_of(&self, pos: usize) -> usize {
        self.doc.text().byte_to_line_idx(pos, LINES)
    }

    /// Byte range of a line, without its line break.
    pub(crate) fn line_range(&self, line: usize) -> ByteRange<usize> {
        let text = self.doc.text();
        let start = text.line_to_byte_idx(line, LINES);
        let slice = text.line(line, LINES);
        start..start + slice.trailing_line_break_idx(LINES).unwrap_or(slice.len())
    }

    fn column_of(&self, pos: usize) -> usize {
        let start = self.line_range(self.line_of(pos)).start;
        self.doc.text().slice(start..pos).chars().count()
    }

    fn pos_at_column(&self, line: usize, column: usize) -> usize {
        let range = self.line_range(line);
        let slice = self.doc.text().slice(range.clone());
        range.start
            + slice
                .chars()
                .take(column)
                .map(char::len_utf8)
                .sum::<usize>()
    }

    /// The previous caret position; a CRLF pair is a single step.
    fn prev_boundary(&self, pos: usize) -> usize {
        let text = self.doc.text();
        match pos {
            0 => 0,
            _ if pos >= 2 && text.byte(pos - 2) == b'\r' && text.byte(pos - 1) == b'\n' => pos - 2,
            _ => text.floor_char_boundary(pos - 1),
        }
    }

    /// The next caret position; a CRLF pair is a single step.
    fn next_boundary(&self, pos: usize) -> usize {
        let text = self.doc.text();
        if pos >= text.len() {
            text.len()
        } else if text.byte(pos) == b'\r' && text.get_byte(pos + 1) == Some(b'\n') {
            pos + 2
        } else {
            text.ceil_char_boundary(pos + 1)
        }
    }

    /// Moves `pos` by `lines` lines, keeping the preferred column with a single caret.
    fn vertical(&self, pos: usize, lines: isize) -> usize {
        let column = match self.preferred_column {
            Some(column) if self.selection.ranges().len() == 1 => column,
            _ => self.column_of(pos),
        };
        let target = self.line_of(pos) as isize + lines;
        if target < 0 {
            0
        } else if target as usize >= self.line_count() {
            self.doc.text().len()
        } else {
            self.pos_at_column(target as usize, column)
        }
    }

    // --- Moving carets -------------------------------------------------------------------------

    fn move_carets(
        &mut self,
        extend: bool,
        cx: &mut Context<Self>,
        head: impl Fn(&Self, Range) -> usize,
    ) {
        self.selection = self.selection.transform(|range| {
            let head = head(self, range);
            if extend {
                Range::new(range.anchor, head)
            } else {
                Range::point(head)
            }
        });
        self.last_edit = LastEdit::None;
        self.scroll_to_primary();
        cx.notify();
    }

    fn move_vertically(&mut self, lines: isize, extend: bool, cx: &mut Context<Self>) {
        if self.preferred_column.is_none() {
            self.preferred_column = Some(self.column_of(self.selection.primary().head));
        }
        self.move_carets(extend, cx, |this, range| this.vertical(range.head, lines));
    }

    fn move_horizontally(
        &mut self,
        extend: bool,
        cx: &mut Context<Self>,
        head: impl Fn(&Self, Range) -> usize,
    ) {
        self.preferred_column = None;
        self.move_carets(extend, cx, head);
    }

    fn scroll_to_primary(&self) {
        let line = self.line_of(self.selection.primary().head);
        self.scroll.scroll_to_item(line, ScrollStrategy::Nearest);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(false, cx, |this, range| {
            if range.is_empty() {
                this.prev_boundary(range.head)
            } else {
                range.from()
            }
        });
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(false, cx, |this, range| {
            if range.is_empty() {
                this.next_boundary(range.head)
            } else {
                range.to()
            }
        });
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(true, cx, |this, range| this.prev_boundary(range.head));
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(true, cx, |this, range| this.next_boundary(range.head));
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, false, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, true, cx);
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-PAGE_LINES, false, cx);
    }

    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(PAGE_LINES, false, cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(false, cx, |this, range| {
            this.line_range(this.line_of(range.head)).start
        });
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(false, cx, |this, range| {
            this.line_range(this.line_of(range.head)).end
        });
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(true, cx, |this, range| {
            this.line_range(this.line_of(range.head)).start
        });
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_horizontally(true, cx, |this, range| {
            this.line_range(this.line_of(range.head)).end
        });
    }

    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::point(0);
        self.move_horizontally(false, cx, |_, _| 0);
    }

    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::point(0);
        self.move_horizontally(false, cx, |this, _| this.doc.text().len());
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::single(Range::new(0, self.doc.text().len()));
        self.last_edit = LastEdit::None;
        cx.notify();
    }

    // --- Editing -------------------------------------------------------------------------------

    fn apply(&mut self, transaction: Transaction, kind: LastEdit, cx: &mut Context<Self>) {
        let grouping = if kind != LastEdit::None && kind == self.last_edit {
            UndoGrouping::MergeWithPrevious
        } else {
            UndoGrouping::NewStep
        };
        let before = self.selection.clone();
        self.doc.apply(&transaction, &before, grouping);
        self.selection = match transaction.selection() {
            Some(selection) => selection.clone(),
            None => before.map(transaction.changes()),
        };
        self.last_edit = kind;
        self.preferred_column = None;
        self.scroll_to_primary();
        cx.notify();
    }

    /// Replaces every selection range with `text`.
    fn insert(&mut self, text: &str, kind: LastEdit, cx: &mut Context<Self>) {
        let transaction = Transaction::replace_selections(self.doc.text(), &self.selection, text)
            .expect("the selection always lies on character boundaries of the document");
        self.apply(transaction, kind, cx);
    }

    /// Replaces one range (used by input methods) and leaves a single caret after it.
    fn replace_range(&mut self, range: ByteRange<usize>, text: &str, cx: &mut Context<Self>) {
        let caret = Selection::point(range.start + text.len());
        let Ok(transaction) =
            Transaction::from_edits(self.doc.text(), [Edit::replace(range, text)])
        else {
            return;
        };
        self.apply(transaction.with_selection(caret), LastEdit::Typing, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = self.selection.transform(|range| {
            if range.is_empty() {
                Range::new(range.head, self.prev_boundary(range.head))
            } else {
                range
            }
        });
        self.insert("", LastEdit::Deleting, cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = self.selection.transform(|range| {
            if range.is_empty() {
                Range::new(range.head, self.next_boundary(range.head))
            } else {
                range
            }
        });
        self.insert("", LastEdit::Deleting, cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.insert(self.doc.line_ending().as_str(), LastEdit::Typing, cx);
    }

    fn insert_tab(&mut self, _: &InsertTab, _: &mut Window, cx: &mut Context<Self>) {
        self.insert("\t", LastEdit::Typing, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(transaction) = self.doc.undo() {
            self.restore_from_history(&transaction, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(transaction) = self.doc.redo() {
            self.restore_from_history(&transaction, cx);
        }
    }

    fn restore_from_history(&mut self, transaction: &Transaction, cx: &mut Context<Self>) {
        self.selection = match transaction.selection() {
            Some(selection) => selection.clone(),
            None => self.selection.map(transaction.changes()),
        };
        self.last_edit = LastEdit::None;
        self.marked = None;
        self.scroll_to_primary();
        cx.notify();
    }

    fn selected_text(&self) -> Option<String> {
        let text = self.doc.text();
        let parts: Vec<String> = self
            .selection
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| text.slice(range.from()..range.to()).to_string())
            .collect();
        (!parts.is_empty()).then(|| parts.join(self.doc.line_ending().as_str()))
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.insert("", LastEdit::None, cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, LastEdit::None, cx);
        }
    }

    // --- Mouse ---------------------------------------------------------------------------------

    /// The text position under a window point, using the lines painted in the last frame.
    fn position_for_point(&self, point: Point<Pixels>) -> Option<usize> {
        let containing = self
            .visible_lines
            .iter()
            .find(|(_, line)| line.bounds.top() <= point.y && point.y < line.bounds.bottom());
        let (&line, visible) = match containing {
            Some(found) => found,
            None => {
                let first = self.visible_lines.iter().min_by_key(|(line, _)| **line)?;
                let last = self.visible_lines.iter().max_by_key(|(line, _)| **line)?;
                if point.y < first.1.bounds.top() {
                    first
                } else {
                    last
                }
            }
        };
        let local = visible
            .shaped
            .closest_index_for_x(point.x - visible.bounds.left());
        Some((visible.start + local).min(self.line_range(line).end))
    }

    fn with_primary(&self, range: Range) -> Selection {
        let mut ranges = self.selection.ranges().to_vec();
        let primary = self.selection.primary_index();
        ranges[primary] = range;
        Selection::new(ranges, primary)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let Some(pos) = self.position_for_point(event.position) else {
            return;
        };
        self.selection = if event.modifiers.shift {
            self.with_primary(Range::new(self.selection.primary().anchor, pos))
        } else if event.modifiers.secondary() {
            self.selection.clone().push(Range::point(pos))
        } else {
            Selection::point(pos)
        };
        self.mouse_selecting = true;
        self.preferred_column = None;
        self.last_edit = LastEdit::None;
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.mouse_selecting {
            return;
        }
        if let Some(pos) = self.position_for_point(event.position) {
            self.selection = self.with_primary(Range::new(self.selection.primary().anchor, pos));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.mouse_selecting = false;
    }

    // --- Rendering -----------------------------------------------------------------------------

    fn update_title(&mut self, window: &mut Window) {
        let name = self
            .info
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "new 1".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
        let modified = if self.doc.is_modified() { "*" } else { "" };
        let title = format!("{modified}{name} - Birchpad");
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
    }

    fn status_bar(&self) -> impl IntoElement {
        let primary = self.selection.primary();
        let selected: usize = self.selection.iter().map(Range::len).sum();
        let line_ending = match self.doc.line_ending() {
            LineEnding::CrLf => "Windows (CR LF)",
            LineEnding::Lf => "Unix (LF)",
            LineEnding::Cr => "Macintosh (CR)",
        };
        let encoding = if self.info.lossy {
            "UTF-8 (invalid bytes replaced)"
        } else {
            "UTF-8"
        };
        let items = [
            format!(
                "Ln {}, Col {}",
                self.line_of(primary.head) + 1,
                self.column_of(primary.head) + 1
            ),
            format!("Sel {selected} | {} carets", self.selection.ranges().len()),
            format!(
                "{} lines, {} bytes",
                self.line_count(),
                self.doc.text().len()
            ),
            line_ending.to_owned(),
            encoding.to_owned(),
            format!("opened in {} ms", self.info.load_time.as_millis()),
        ];
        div()
            .flex()
            .flex_row()
            .gap_6()
            .px_3()
            .h(px(24.))
            .flex_none()
            .items_center()
            .border_t_1()
            .border_color(rgb(0xd0d7de))
            .bg(rgb(0xf6f8fa))
            .text_size(px(12.))
            .children(items)
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.visible_lines.clear();
        self.update_title(window);

        let line_count = self.line_count();
        let digits = line_count.to_string().len().max(3);
        let gutter_width = px(digits as f32 * 8. + 24.);
        let editor = cx.entity();
        let focus_handle = self.focus_handle.clone();

        let lines = uniform_list(
            "lines",
            line_count,
            cx.processor(move |_, visible: ByteRange<usize>, _, cx| {
                visible
                    .map(|line| {
                        div()
                            .flex()
                            .flex_row()
                            .h(px(LINE_HEIGHT))
                            .child(
                                div()
                                    .w(gutter_width)
                                    .flex_none()
                                    .flex()
                                    .justify_end()
                                    .pr_3()
                                    .bg(rgb(0xf6f8fa))
                                    .text_color(rgb(0x8c959f))
                                    .child((line + 1).to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .pl_1()
                                    .child(LineElement::new(cx.entity(), line)),
                            )
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();

        let input_handler = canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                window.handle_input(&focus_handle, ElementInputHandler::new(bounds, editor), cx);
            },
        )
        .absolute()
        .size_full();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2328))
            .font_family(MONOSPACE)
            .text_size(px(14.))
            .line_height(px(LINE_HEIGHT))
            .child(
                div()
                    .id("editor")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .key_context("Editor")
                    .track_focus(&self.focus_handle)
                    .cursor(CursorStyle::IBeam)
                    .on_action(cx.listener(Self::backspace))
                    .on_action(cx.listener(Self::delete))
                    .on_action(cx.listener(Self::left))
                    .on_action(cx.listener(Self::right))
                    .on_action(cx.listener(Self::up))
                    .on_action(cx.listener(Self::down))
                    .on_action(cx.listener(Self::select_left))
                    .on_action(cx.listener(Self::select_right))
                    .on_action(cx.listener(Self::select_up))
                    .on_action(cx.listener(Self::select_down))
                    .on_action(cx.listener(Self::home))
                    .on_action(cx.listener(Self::end))
                    .on_action(cx.listener(Self::select_home))
                    .on_action(cx.listener(Self::select_end))
                    .on_action(cx.listener(Self::document_start))
                    .on_action(cx.listener(Self::document_end))
                    .on_action(cx.listener(Self::page_up))
                    .on_action(cx.listener(Self::page_down))
                    .on_action(cx.listener(Self::select_all))
                    .on_action(cx.listener(Self::newline))
                    .on_action(cx.listener(Self::insert_tab))
                    .on_action(cx.listener(Self::undo))
                    .on_action(cx.listener(Self::redo))
                    .on_action(cx.listener(Self::copy))
                    .on_action(cx.listener(Self::cut))
                    .on_action(cx.listener(Self::paste))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .child(lines)
                    .child(input_handler),
            )
            .child(self.status_bar())
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: ByteRange<usize>,
        adjusted_range: &mut Option<ByteRange<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.bytes_from_utf16(&range_utf16);
        adjusted_range.replace(self.utf16_from_bytes(&range));
        Some(self.doc.text().slice(range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let primary = self.selection.primary();
        Some(UTF16Selection {
            range: self.utf16_from_bytes(&(primary.from()..primary.to())),
            reversed: primary.is_backward(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<ByteRange<usize>> {
        self.marked
            .as_ref()
            .map(|range| self.utf16_from_bytes(range))
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
            .map(|range| self.bytes_from_utf16(&range))
            .or(self.marked.take());
        self.marked = None;
        match target {
            Some(range) => self.replace_range(range, text, cx),
            None => self.insert(text, LastEdit::Typing, cx),
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<ByteRange<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<ByteRange<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = range_utf16
            .map(|range| self.bytes_from_utf16(&range))
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
            self.selection = Selection::single(Range::new(anchor, head));
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: ByteRange<usize>,
        _element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.bytes_from_utf16(&range_utf16);
        let line = self.line_of(range.start);
        let visible = self.visible_lines.get(&line)?;
        let end = range.end.min(self.line_range(line).end);
        let x =
            |pos: usize| visible.bounds.left() + visible.shaped.x_for_index(pos - visible.start);
        Some(Bounds::from_corners(
            point(x(range.start), visible.bounds.top()),
            point(x(end), visible.bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let pos = self.position_for_point(point)?;
        Some(self.doc.text().byte_to_utf16_idx(pos))
    }
}

impl Editor {
    fn utf16_from_bytes(&self, range: &ByteRange<usize>) -> ByteRange<usize> {
        let text = self.doc.text();
        text.byte_to_utf16_idx(range.start)..text.byte_to_utf16_idx(range.end)
    }

    fn bytes_from_utf16(&self, range: &ByteRange<usize>) -> ByteRange<usize> {
        let text = self.doc.text();
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
