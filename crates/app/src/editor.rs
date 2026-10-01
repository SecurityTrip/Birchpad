//! The editor view: one view of a buffer, with its own selection and scroll position.
//!
//! Several views may show the same buffer (split view, "Clone to Other View"); each maps its
//! selection through the changes the others make.

use std::collections::HashMap;
use std::ops::Range as ByteRange;

use birchpad_core::motion::{self, line_count, line_of, line_range};
use birchpad_core::{Edit, LineEnding, Range, Rope, Selection, Transaction, UndoGrouping};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollStrategy, ShapedLine, Subscription, UTF16Selection,
    UniformListScrollHandle, Window, canvas, div, point, prelude::*, px, rgb, uniform_list,
};
use serde::Deserialize;

use crate::buffer::{Buffer, BufferEvent, ReadOnly};
use crate::commands::{CommandRegistry, Handler, RunCommand};
use crate::line_element::LineElement;

pub(crate) const LINE_HEIGHT: f32 = 20.;
/// Lines moved by PageUp/PageDown until the view reports its real height.
const PAGE_LINES: isize = 30;

/// Caret motions; each is registered as `cursor.<name>` and `select.<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

const MOTIONS: [(&str, &str, Motion); 10] = [
    ("cursor.left", "select.left", Motion::Left),
    ("cursor.right", "select.right", Motion::Right),
    ("cursor.up", "select.up", Motion::Up),
    ("cursor.down", "select.down", Motion::Down),
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

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
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
        this.copy(cx);
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
        this.delete(cx);
        Ok(())
    });
    registry.editor("edit.backspace", |this, (), _, cx| {
        this.backspace(cx);
        Ok(())
    });
    registry.editor("edit.select-all", |this, (), _, cx| {
        this.select_all(cx);
        Ok(())
    });
    registry.editor("edit.newline", |this, (), _, cx| {
        let line_ending = this.buffer.read(cx).doc().line_ending();
        this.insert(line_ending.as_str(), LastEdit::Typing, cx);
        Ok(())
    });
    registry.editor("edit.tab", |this, (), _, cx| {
        this.insert("\t", LastEdit::Typing, cx);
        Ok(())
    });
    registry.editor("edit.insert-text", |this, args: InsertTextArgs, _, cx| {
        this.insert(&args.text, LastEdit::None, cx);
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
    for id in [
        "cursor.word-left",
        "cursor.word-right",
        "select.word-left",
        "select.word-right",
        "edit.delete-word-left",
        "edit.delete-word-right",
        "edit.toggle-overwrite",
    ] {
        registry.pending(id);
    }
}

/// Which kind of edit was made last, to group consecutive typing into one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LastEdit {
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

pub(crate) struct EditorView {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) buffer: Entity<Buffer>,
    pub(crate) selection: Selection,
    /// Text being composed by an input method, as a byte range.
    pub(crate) marked: Option<ByteRange<usize>>,
    pub(crate) visible_lines: HashMap<usize, VisibleLine>,
    scroll: UniformListScrollHandle,
    preferred_column: Option<usize>,
    last_edit: LastEdit,
    mouse_selecting: bool,
    _buffer_events: Subscription,
}

impl EditorView {
    pub(crate) fn new(buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Self {
        let events = cx.subscribe(&buffer, |this, _, event, cx| match event {
            BufferEvent::Edited {
                transaction,
                origin,
            } => {
                if *origin != Some(cx.entity_id()) {
                    this.selection = this.selection.map(transaction.changes());
                    this.marked = None;
                }
                cx.notify();
            }
            BufferEvent::Reloaded => {
                this.selection = Selection::point(0);
                this.marked = None;
                this.preferred_column = None;
                this.last_edit = LastEdit::None;
                this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            }
            BufferEvent::StateChanged | BufferEvent::LoadFailed(_) => cx.notify(),
        });
        Self {
            focus_handle: cx.focus_handle(),
            buffer,
            selection: Selection::point(0),
            marked: None,
            visible_lines: HashMap::new(),
            scroll: UniformListScrollHandle::new(),
            preferred_column: None,
            last_edit: LastEdit::None,
            mouse_selecting: false,
            _buffer_events: events,
        }
    }

    pub(crate) fn text<'a>(&self, cx: &'a App) -> &'a Rope {
        self.buffer.read(cx).doc().text()
    }

    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        let invocation = &action.0;
        let handler = cx.global::<CommandRegistry>().get(&invocation.command);
        match handler {
            Some(Handler::Editor(run)) => {
                if let Err(error) = run(self, invocation, window, cx) {
                    crate::workspace::report_error(&error, window, cx);
                }
            }
            _ => cx.propagate(),
        }
    }

    // --- Text geometry -------------------------------------------------------------------------

    fn column_of(text: &Rope, pos: usize) -> usize {
        let start = line_range(text, line_of(text, pos)).start;
        text.slice(start..pos).chars().count()
    }

    fn pos_at_column(text: &Rope, line: usize, column: usize) -> usize {
        let range = line_range(text, line);
        range.start
            + text
                .slice(range)
                .chars()
                .take(column)
                .map(char::len_utf8)
                .sum::<usize>()
    }

    /// Moves `pos` by `lines` lines, keeping the preferred column.
    fn vertical(text: &Rope, pos: usize, lines: isize, column: usize) -> usize {
        let target = line_of(text, pos) as isize + lines;
        if target < 0 {
            0
        } else if target as usize >= line_count(text) {
            text.len()
        } else {
            Self::pos_at_column(text, target as usize, column)
        }
    }

    // --- Moving carets -------------------------------------------------------------------------

    fn move_carets(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let vertical = match motion {
            Motion::Up => Some(-1),
            Motion::Down => Some(1),
            Motion::PageUp => Some(-PAGE_LINES),
            Motion::PageDown => Some(PAGE_LINES),
            _ => None,
        };
        if vertical.is_some() {
            if self.preferred_column.is_none() {
                self.preferred_column = Some(Self::column_of(&text, self.selection.primary().head));
            }
        } else {
            self.preferred_column = None;
        }
        let single = self.selection.ranges().len() == 1;
        let preferred = self.preferred_column;
        if matches!(motion, Motion::DocumentStart | Motion::DocumentEnd) && !extend {
            self.selection = Selection::point(self.selection.primary().head);
        }
        self.selection = self.selection.transform(|range| {
            let head = match (motion, vertical) {
                (_, Some(lines)) => {
                    let column = match preferred {
                        Some(column) if single => column,
                        _ => Self::column_of(&text, range.head),
                    };
                    Self::vertical(&text, range.head, lines, column)
                }
                (Motion::Left, _) if !extend && !range.is_empty() => range.from(),
                (Motion::Right, _) if !extend && !range.is_empty() => range.to(),
                (Motion::Left, _) => motion::prev_boundary(&text, range.head),
                (Motion::Right, _) => motion::next_boundary(&text, range.head),
                (Motion::Home, _) => line_range(&text, line_of(&text, range.head)).start,
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
        self.last_edit = LastEdit::None;
        self.scroll_to_primary(cx);
        cx.notify();
    }

    fn scroll_to_primary(&self, cx: &App) {
        let line = line_of(self.text(cx), self.selection.primary().head);
        self.scroll.scroll_to_item(line, ScrollStrategy::Nearest);
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::single(Range::new(0, self.text(cx).len()));
        self.last_edit = LastEdit::None;
        cx.notify();
    }

    // --- Editing -------------------------------------------------------------------------------

    /// False while the buffer is loading, has a decoding problem or is a read-only file.
    pub(crate) fn is_editable(&self, cx: &App) -> bool {
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
        let origin = Some(cx.entity_id());
        self.buffer.update(cx, |buffer, cx| {
            buffer.apply(transaction, &before, grouping, origin, cx);
        });
        self.selection = selection;
        self.last_edit = kind;
        self.preferred_column = None;
        self.scroll_to_primary(cx);
        cx.notify();
    }

    /// Replaces every selection range with `text`.
    pub(crate) fn insert(&mut self, text: &str, kind: LastEdit, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let transaction = Transaction::replace_selections(self.text(cx), &self.selection, text)
            .expect("the selection always lies on character boundaries of the document");
        self.apply(transaction, kind, cx);
    }

    /// Replaces one range (used by input methods) and leaves a single caret after it.
    fn replace_range(&mut self, range: ByteRange<usize>, text: &str, cx: &mut Context<Self>) {
        let caret = Selection::point(range.start + text.len());
        let Ok(transaction) = Transaction::from_edits(self.text(cx), [Edit::replace(range, text)])
        else {
            return;
        };
        self.apply(transaction.with_selection(caret), LastEdit::Typing, cx);
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let text = self.text(cx).clone();
        self.selection = self.selection.transform(|range| {
            if range.is_empty() {
                Range::new(range.head, motion::prev_boundary(&text, range.head))
            } else {
                range
            }
        });
        self.insert("", LastEdit::Deleting, cx);
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let text = self.text(cx).clone();
        self.selection = self.selection.transform(|range| {
            if range.is_empty() {
                Range::new(range.head, motion::next_boundary(&text, range.head))
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
        self.selection = match transaction.selection() {
            Some(selection) => selection.clone(),
            None => self.selection.map(transaction.changes()),
        };
        self.last_edit = LastEdit::None;
        self.marked = None;
        self.scroll_to_primary(cx);
        cx.notify();
    }

    fn selected_text(&self, cx: &App) -> Option<String> {
        let doc = self.buffer.read(cx).doc();
        let parts: Vec<String> = self
            .selection
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| doc.text().slice(range.from()..range.to()).to_string())
            .collect();
        (!parts.is_empty()).then(|| parts.join(doc.line_ending().as_str()))
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.insert("", LastEdit::None, cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, LastEdit::None, cx);
        }
    }

    // --- Mouse ---------------------------------------------------------------------------------

    /// The text position under a window point, using the lines painted in the last frame.
    fn position_for_point(&self, point: Point<Pixels>, cx: &App) -> Option<usize> {
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
        Some((visible.start + local).min(line_range(self.text(cx), line).end))
    }

    fn with_primary(&self, range: Range) -> Selection {
        let mut ranges = self.selection.ranges().to_vec();
        let primary = self.selection.primary_index();
        ranges[primary] = range;
        Selection::new(ranges, primary)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        let Some(pos) = self.position_for_point(event.position, cx) else {
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
        if let Some(pos) = self.position_for_point(event.position, cx) {
            self.selection = self.with_primary(Range::new(self.selection.primary().anchor, pos));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.mouse_selecting = false;
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.visible_lines.clear();

        let line_count = line_count(self.text(cx));
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

        let buffer = self.buffer.read(cx);
        let banner = match buffer.read_only() {
            Some(ReadOnly::Decoding(problem)) => {
                let encoding = birchpad_io::display_name(buffer.doc().format().encoding, false);
                Some(crate::banner::decoding_problem(&encoding, problem))
            }
            Some(ReadOnly::File) => Some(crate::banner::read_only_file()),
            Some(ReadOnly::Loading) | None => None,
        };
        let loading = buffer.loading_progress();

        div()
            .id("editor")
            .key_context("Editor")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1f2328))
            .children(banner)
            .child(
                div()
                    .id("text")
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .font_family(crate::MONOSPACE)
                    .text_size(px(14.))
                    .line_height(px(LINE_HEIGHT))
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
                    .child(lines)
                    .child(input_handler)
                    .children(loading.map(crate::banner::loading)),
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
            self.selection = Selection::single(Range::new(anchor, head));
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
        let text = self.text(cx);
        let line = line_of(text, range.start);
        let visible = self.visible_lines.get(&line)?;
        let end = range.end.min(line_range(text, line).end);
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
