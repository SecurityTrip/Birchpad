//! Plugins > Hex Editor > View in HEX, as Notepad++'s Hex Editor plugin: the document's own
//! bytes, in its encoding, shown and edited in place of its text (ADR 0049).
//!
//! Rows of `hex.columns` bytes, grouped by `hex.group`, with their offset and the bytes as text.
//! The caret is a byte; it is the text's caret too, so View in HEX again shows the text where
//! the bytes were. Hex digits type a byte in two halves, over the byte at the caret or before it
//! (Insert switches); Tab moves to the text column, where characters type their bytes. The
//! document stays text: bytes that are not text in its encoding are held back, shown as typed,
//! until the bytes that complete a character come, and refused past that; each edit is one undo
//! step of the text. Find in HEX finds bytes written in hexadecimal, or text in quotes.

use std::ops::Range;
use std::rc::Rc;

use anyhow::{Result, anyhow, bail};
use birchpad_core::Encoding;
use birchpad_core::{Format, Range as Span, RevisionId, Rope, Selection};
use birchpad_tools::hex;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    MouseDownEvent, ScrollStrategy, Subscription, UniformListScrollHandle, Window, div, prelude::*,
    px, uniform_list,
};

use crate::app_state::AppState;
use crate::editor::EditorView;
use crate::workspace::{Workspace, report_error};

/// The largest document shown in hex, in bytes.
pub const MAX_BYTES: usize = 64 << 20;
/// Characters between two points of the map from text to bytes, for encodings other than UTF-8.
const CHECKPOINT: usize = 256;
/// The most bytes held back while they are not text yet: the longest character of an encoding.
const MAX_PENDING: usize = 4;
const ROW_HEIGHT: f32 = 20.;

/// The bytes of one state of a document, and how they map to its text.
pub struct Bytes {
    revision: RevisionId,
    format: Format,
    pub bytes: Rc<Vec<u8>>,
    /// The byte order mark's length, at the start.
    pub bom: usize,
    /// For encodings other than UTF-8: positions in the text every [`CHECKPOINT`] characters,
    /// with their byte offsets.
    checkpoints: Vec<(usize, usize)>,
}

fn encoded_len(text: &str, encoding: Encoding) -> usize {
    birchpad_io::encode(&Rope::from(text), encoding, false).map_or(text.len(), |bytes| bytes.len())
}

impl Bytes {
    fn of(text: &Rope, format: Format, revision: RevisionId) -> Result<Self> {
        let bytes =
            birchpad_io::encode_document(text, format).map_err(|error| anyhow!("{error}"))?;
        if bytes.len() > MAX_BYTES {
            bail!(
                "{}",
                t!(
                    "hex-editor.too-large",
                    size = bytes.len(),
                    limit = MAX_BYTES
                )
            );
        }
        let bom = if format.bom {
            birchpad_io::bom_bytes(format.encoding).len()
        } else {
            0
        };
        let mut checkpoints = Vec::new();
        if format.encoding != Encoding::Utf8 {
            let whole = text.to_string();
            let (mut pos, mut byte) = (0, bom);
            let mut chars = whole.char_indices().peekable();
            while chars.peek().is_some() {
                checkpoints.push((pos, byte));
                let end = chars
                    .by_ref()
                    .take(CHECKPOINT)
                    .last()
                    .map_or(pos, |(at, c)| at + c.len_utf8());
                byte += encoded_len(&whole[pos..end], format.encoding);
                pos = end;
            }
            checkpoints.push((pos, byte));
        }
        Ok(Self {
            revision,
            format,
            bytes: Rc::new(bytes),
            bom,
            checkpoints,
        })
    }

    /// The byte where the character at text position `pos` starts.
    pub fn byte_of(&self, text: &Rope, pos: usize) -> usize {
        let pos = pos.min(text.len());
        if self.format.encoding == Encoding::Utf8 {
            return self.bom + pos;
        }
        let index = self.checkpoints.partition_point(|(at, _)| *at <= pos);
        let (from, byte) = self.checkpoints[index.saturating_sub(1)];
        byte + encoded_len(&text.slice(from..pos).to_string(), self.format.encoding)
    }

    /// The text position of the character that byte `byte` belongs to.
    pub fn pos_of(&self, text: &Rope, byte: usize) -> usize {
        if self.format.encoding == Encoding::Utf8 {
            let mut pos = byte.saturating_sub(self.bom).min(text.len());
            while !text.is_char_boundary(pos) {
                pos -= 1;
            }
            return pos;
        }
        let index = self.checkpoints.partition_point(|(_, at)| *at <= byte);
        let (mut pos, mut at) = self.checkpoints[index.saturating_sub(1)];
        let rest = text
            .slice(pos..text.len().min(pos + CHECKPOINT * 4))
            .to_string();
        for c in rest.chars() {
            let width = encoded_len(c.encode_utf8(&mut [0; 4]), self.format.encoding);
            if at + width > byte {
                break;
            }
            at += width;
            pos += c.len_utf8();
        }
        pos
    }
}

pub struct HexEditor {
    pub view: Entity<EditorView>,
    focus_handle: FocusHandle,
    /// The caret, a byte offset (up to the length, after the last byte).
    pub cursor: usize,
    /// The other end of the selection.
    pub anchor: usize,
    /// Typing replaces the bytes at the caret rather than going before them.
    pub overwrite: bool,
    /// The text column has the keys rather than the hexadecimal one.
    pub in_text: bool,
    /// Bytes typed that are not text yet, held back.
    pub pending: Vec<u8>,
    /// The high half of a byte being typed.
    pub nibble: Option<u8>,
    pub find: Entity<InputState>,
    bytes: Option<Bytes>,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for HexEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl HexEditor {
    pub fn new(view: Entity<EditorView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("hex-editor.find-placeholder")));
        let enter = cx.subscribe_in(
            &find,
            window,
            |this, find, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, .. } = event {
                    let pattern = find.read(cx).value().to_string();
                    if let Err(error) = this.find_next(&pattern, *secondary, cx) {
                        report_error(&error, window, cx);
                    }
                }
            },
        );
        let changes = cx.observe(&view, |_, _, cx| cx.notify());
        let mut editor = Self {
            view,
            focus_handle: cx.focus_handle(),
            cursor: 0,
            anchor: 0,
            overwrite: true,
            in_text: false,
            pending: Vec::new(),
            nibble: None,
            find,
            bytes: None,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: vec![enter, changes],
        };
        // The text's caret becomes the bytes' caret.
        if editor.refresh(cx).is_ok() {
            let (anchor, head) = {
                let selection = editor.view.read(cx).selection.primary();
                (selection.anchor, selection.head)
            };
            let text = editor.text(cx);
            let bytes = editor.bytes.as_ref().expect("just read");
            editor.anchor = bytes.byte_of(&text, anchor);
            editor.cursor = bytes.byte_of(&text, head);
        }
        editor
    }

    fn text(&self, cx: &App) -> Rope {
        self.view.read(cx).buffer.read(cx).doc().text().clone()
    }

    /// Reads the document's bytes again if it changed.
    pub fn refresh(&mut self, cx: &App) -> Result<()> {
        let doc = self.view.read(cx).buffer.read(cx).doc();
        let (revision, format) = (doc.revision(), doc.format());
        if self
            .bytes
            .as_ref()
            .is_some_and(|bytes| bytes.revision == revision && bytes.format == format)
        {
            return Ok(());
        }
        let text = doc.text().clone();
        self.bytes = Some(Bytes::of(&text, format, revision)?);
        let len = self.len();
        self.cursor = self.cursor.min(len);
        self.anchor = self.anchor.min(len);
        Ok(())
    }

    /// The document's bytes.
    pub fn bytes(&mut self, cx: &App) -> Rc<Vec<u8>> {
        let _ = self.refresh(cx);
        self.bytes
            .as_ref()
            .map(|bytes| bytes.bytes.clone())
            .unwrap_or_default()
    }

    fn len(&self) -> usize {
        self.bytes.as_ref().map_or(0, |bytes| bytes.bytes.len())
    }

    fn layout(cx: &App) -> (usize, usize) {
        AppState::global(cx).settings.hex.layout()
    }

    /// Moves the caret to byte `to`, extending the selection or not, and puts the text's caret
    /// on the character there.
    pub fn move_to(&mut self, to: usize, extend: bool, cx: &mut Context<Self>) {
        let _ = self.refresh(cx);
        self.cursor = to.min(self.len());
        if !extend {
            self.anchor = self.cursor;
        }
        self.pending.clear();
        self.nibble = None;
        self.sync_text(cx);
        let (columns, _) = Self::layout(cx);
        self.scroll
            .scroll_to_item(self.cursor / columns, ScrollStrategy::Top);
        cx.notify();
    }

    /// The text's selection follows the bytes'.
    fn sync_text(&mut self, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let Some(bytes) = &self.bytes else {
            return;
        };
        let (anchor, head) = (
            bytes.pos_of(&text, self.anchor),
            bytes.pos_of(&text, self.cursor),
        );
        self.view.update(cx, |view, cx| {
            view.selection = Selection::single(Span::new(anchor, head));
            cx.notify();
        });
    }

    /// The bytes selected, or none.
    pub fn selected(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    /// Replaces the bytes `range` with `with`, if the result is text in the document's
    /// encoding; the caret goes to `caret` then.
    pub fn replace(
        &mut self,
        range: Range<usize>,
        with: &[u8],
        caret: usize,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.refresh(cx)?;
        let bytes = self.bytes.as_ref().expect("just read");
        let old = bytes.bytes.clone();
        let range = range.start.min(old.len())..range.end.min(old.len());
        if range.start < bytes.bom || (range.is_empty() && range.start < bytes.bom) {
            bail!("{}", t!("hex-editor.byte-order-mark"));
        }
        let encoding = bytes.format.encoding;
        let mut new = Vec::with_capacity(old.len() + with.len());
        new.extend_from_slice(&old[..range.start]);
        new.extend_from_slice(with);
        new.extend_from_slice(&old[range.end..]);
        let decoded = birchpad_io::decode(new, encoding);
        if let Some(problem) = decoded.problem {
            bail!(
                "{}",
                t!(
                    "hex-view.the-bytes-are-not-text-at-byte",
                    encoding = encoding,
                    problem = problem.offset()
                )
            );
        }
        self.view.update(cx, |view, cx| -> Result<()> {
            let transaction = hex::replace_differing(view.text(cx), &decoded.text)?;
            if transaction.changes().is_identity() {
                return Ok(());
            }
            if !view.apply_command_edit(transaction, cx) {
                bail!("{}", t!("hex-view.the-document-cannot-be-changed"));
            }
            Ok(())
        })?;
        self.refresh(cx)?;
        self.move_to(caret, false, cx);
        Ok(())
    }

    /// A hex digit typed in the hexadecimal column.
    pub fn type_digit(&mut self, digit: u8, cx: &mut Context<Self>) -> Result<()> {
        match self.nibble.take() {
            None => {
                self.nibble = Some(digit);
                cx.notify();
                Ok(())
            }
            Some(high) => {
                self.pending.push(high << 4 | digit);
                self.flush(cx)
            }
        }
    }

    /// Characters typed in the text column: their bytes in the document's encoding.
    pub fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) -> Result<()> {
        self.refresh(cx)?;
        let encoding = self.bytes.as_ref().expect("just read").format.encoding;
        let bytes = birchpad_io::encode(&Rope::from(typed), encoding, false)
            .map_err(|error| anyhow!("{error}"))?;
        self.nibble = None;
        self.pending.extend(bytes);
        self.flush(cx)
    }

    /// Puts the bytes held back into the document once they are text; past
    /// [`MAX_PENDING`] bytes that are not, gives up on them.
    fn flush(&mut self, cx: &mut Context<Self>) -> Result<()> {
        self.refresh(cx)?;
        let pending = std::mem::take(&mut self.pending);
        let selected = self.selected();
        let range = if !selected.is_empty() {
            selected
        } else if self.overwrite {
            self.cursor..self.cursor + pending.len()
        } else {
            self.cursor..self.cursor
        };
        let caret = range.start + pending.len();
        let bom = self.bytes.as_ref().map_or(0, |bytes| bytes.bom);
        if range.start >= bom
            && !self.is_text(range.clone(), &pending)
            && pending.len() < MAX_PENDING
        {
            // Perhaps the next bytes complete a character.
            self.pending = pending;
            cx.notify();
            return Ok(());
        }
        let result = self.replace(range, &pending, caret, cx);
        cx.notify();
        result
    }

    /// Whether the bytes with `range` replaced by `with` are text in the document's encoding.
    fn is_text(&self, range: Range<usize>, with: &[u8]) -> bool {
        let Some(bytes) = &self.bytes else {
            return false;
        };
        let old = &bytes.bytes;
        let range = range.start.min(old.len())..range.end.min(old.len());
        let mut new = Vec::with_capacity(old.len() + with.len());
        new.extend_from_slice(&old[..range.start]);
        new.extend_from_slice(with);
        new.extend_from_slice(&old[range.end..]);
        birchpad_io::decode(new, bytes.format.encoding)
            .problem
            .is_none()
    }

    /// Backspace or Delete: the selection, or the character before or at the caret.
    pub fn delete(&mut self, forward: bool, cx: &mut Context<Self>) -> Result<()> {
        if !self.pending.is_empty() || self.nibble.is_some() {
            self.pending.clear();
            self.nibble = None;
            cx.notify();
            return Ok(());
        }
        self.refresh(cx)?;
        let selected = self.selected();
        if !selected.is_empty() {
            return self.replace(selected.clone(), &[], selected.start, cx);
        }
        let text = self.text(cx);
        let bytes = self.bytes.as_ref().expect("just read");
        let pos = bytes.pos_of(&text, self.cursor);
        // The whole character: its bytes alone would not be text.
        let (start, end) = if forward {
            if self.cursor >= self.len() {
                return Ok(());
            }
            let next = birchpad_core::motion::next_boundary(&text, pos);
            (
                bytes.byte_of(&text, pos),
                bytes.byte_of(&text, next).max(self.cursor + 1),
            )
        } else {
            if self.cursor <= bytes.bom {
                return Ok(());
            }
            let before = bytes.pos_of(&text, self.cursor - 1);
            (
                bytes.byte_of(&text, before),
                self.cursor.max(bytes.byte_of(&text, before) + 1),
            )
        };
        if self.overwrite && !forward {
            self.move_to(start, false, cx);
            return Ok(());
        }
        self.replace(start..end, &[], start, cx)
    }

    /// Finds the next (or the previous) bytes `pattern` writes: hexadecimal digits, or text in
    /// quotes, in the document's encoding. They are selected.
    pub fn find_next(
        &mut self,
        pattern: &str,
        backward: bool,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.refresh(cx)?;
        let encoding = self.bytes.as_ref().expect("just read").format.encoding;
        let trimmed = pattern.trim();
        let needle = match trimmed
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            Some(text) if !text.is_empty() => {
                birchpad_io::encode(&Rope::from(text), encoding, false)
                    .map_err(|error| anyhow!("{error}"))?
            }
            _ => hex::parse_pattern(trimmed)?,
        };
        let haystack = self.bytes(cx);
        let selected = self.selected();
        // Found already: the next one starts after the start of this one.
        let from = if backward {
            selected.start
        } else if selected.is_empty() {
            self.cursor
        } else {
            selected.start + 1
        };
        let found = hex::find(&haystack, &needle, from, backward)
            .ok_or_else(|| anyhow!("{}", t!("hex-editor.not-found", pattern = trimmed)))?;
        self.move_to(found, false, cx);
        self.cursor = found + needle.len();
        self.sync_text(cx);
        cx.notify();
        Ok(())
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt {
            return;
        }
        let (columns, _) = Self::layout(cx);
        let extend = modifiers.shift;
        let page = columns * 16;
        let row_start = self.cursor - self.cursor % columns;
        let result = match keystroke.key.as_str() {
            "left" => {
                self.move_to(self.cursor.saturating_sub(1), extend, cx);
                Ok(())
            }
            "right" => {
                self.move_to(self.cursor + 1, extend, cx);
                Ok(())
            }
            "up" => {
                self.move_to(self.cursor.saturating_sub(columns), extend, cx);
                Ok(())
            }
            "down" => {
                self.move_to(self.cursor + columns, extend, cx);
                Ok(())
            }
            "pageup" => {
                self.move_to(self.cursor.saturating_sub(page), extend, cx);
                Ok(())
            }
            "pagedown" => {
                self.move_to(self.cursor + page, extend, cx);
                Ok(())
            }
            "home" => {
                self.move_to(row_start, extend, cx);
                Ok(())
            }
            "end" => {
                self.move_to(row_start + columns - 1, extend, cx);
                Ok(())
            }
            "tab" => {
                self.in_text = !self.in_text;
                self.nibble = None;
                cx.notify();
                Ok(())
            }
            "insert" => {
                self.overwrite = !self.overwrite;
                cx.notify();
                Ok(())
            }
            "escape" => {
                self.pending.clear();
                self.nibble = None;
                cx.notify();
                Ok(())
            }
            "backspace" => self.delete(false, cx),
            "delete" => self.delete(true, cx),
            _ => {
                // A printable key without its character (as some platforms send it) types
                // itself.
                let typed = keystroke.key_char.clone().or_else(|| {
                    (keystroke.key.chars().count() == 1).then(|| keystroke.key.clone())
                });
                let Some(typed) = typed.filter(|typed| !typed.is_empty()) else {
                    return;
                };
                let typed = typed.as_str();
                if self.in_text {
                    self.type_text(typed, cx)
                } else {
                    match typed.chars().next().and_then(|c| c.to_digit(16)) {
                        Some(digit) if typed.chars().count() == 1 => {
                            self.type_digit(digit as u8, cx)
                        }
                        _ => return,
                    }
                }
            }
        };
        if let Err(error) = result {
            report_error(&error, window, cx);
        }
        cx.stop_propagation();
    }

    /// A click on byte `byte`, in the text column or not.
    fn clicked(
        &mut self,
        byte: usize,
        in_text: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.in_text = in_text;
        window.focus(&self.focus_handle, cx);
        self.move_to(byte, extend, cx);
    }

    fn render_row(&mut self, row: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (columns, group) = Self::layout(cx);
        let uppercase = AppState::global(cx).settings.hex.uppercase;
        let bytes = self.bytes(cx);
        let colors = crate::theme::ui();
        let start = row * columns;
        let selected = self.selected();
        let caret = self.cursor;
        let pending = (self.nibble.is_some() || !self.pending.is_empty()).then(|| {
            let mut shown: String = self
                .pending
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect();
            if let Some(high) = self.nibble {
                shown.push_str(&format!("{high:X}_"));
            }
            shown
        });
        let in_text = self.in_text;
        let cell = |byte: usize, text: bool, label: String, cx: &mut Context<Self>| {
            let is_caret = byte == caret;
            let active = is_caret && text == in_text;
            div()
                .id((if text { "hex-text" } else { "hex-byte" }, byte))
                .debug_selector(move || {
                    format!("{}-{byte}", if text { "hex-text" } else { "hex-byte" })
                })
                .when(selected.contains(&byte), |cell| {
                    cell.bg(crate::theme::paint(colors.selected))
                })
                .when(is_caret, |cell| {
                    cell.border_1().border_color(crate::theme::paint(if active {
                        colors.accent
                    } else {
                        colors.border
                    }))
                })
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.clicked(byte, text, event.modifiers.shift, window, cx);
                    }),
                )
        };
        let mut hex_cells = Vec::with_capacity(columns);
        let mut text_cells = Vec::with_capacity(columns);
        for index in 0..columns {
            let byte = start + index;
            let label = match bytes.get(byte) {
                Some(_) if byte == caret && pending.is_some() && !in_text => {
                    pending.clone().unwrap_or_default()
                }
                Some(value) if uppercase => format!("{value:02X}"),
                Some(value) => format!("{value:02x}"),
                // The place after the last byte, where bytes are added.
                None if byte == bytes.len() => pending.clone().unwrap_or_else(|| "  ".to_owned()),
                None => "  ".to_owned(),
            };
            let gap = index > 0 && index % group == 0;
            hex_cells.push(
                div()
                    .flex()
                    .when(gap, |cell| cell.ml(px(6.)))
                    .child(cell(byte, false, label, cx)),
            );
            let shown = bytes.get(byte).map_or(' ', |&value| hex::printable(value));
            text_cells.push(cell(byte, true, shown.to_string(), cx));
        }
        div()
            .id(("hex-row", row))
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_2()
            .whitespace_nowrap()
            .child(
                div()
                    .text_color(crate::theme::paint(colors.muted))
                    .child(format!("{start:08X}")),
            )
            .child(div().flex().flex_row().gap_1().children(hex_cells))
            .child(div().flex().flex_row().children(text_cells))
    }
}

impl Render for HexEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let problem = self.refresh(cx).err().map(|error| error.to_string());
        let colors = crate::theme::ui();
        let (columns, _) = Self::layout(cx);
        let len = self.len();
        let rows = len / columns + 1;
        let status = format!(
            "{}  {}  {}",
            t!(
                "hex-editor.offset",
                offset = format!("{:X}", self.cursor),
                decimal = self.cursor,
                length = len
            ),
            if self.overwrite {
                t!("hex-editor.overwrite")
            } else {
                t!("hex-editor.insert")
            },
            if self.in_text {
                t!("hex-editor.text-column")
            } else {
                t!("hex-editor.hex-column")
            }
        );
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .p_1()
            .flex_none()
            .border_b_1()
            .border_color(crate::theme::paint(colors.border))
            .child(
                div()
                    .w(px(240.))
                    .debug_selector(|| "hex-find".into())
                    .child(Input::new(&self.find).id("hex-find")),
            )
            .child(
                div()
                    .debug_selector(|| "hex-status".into())
                    .text_color(crate::theme::paint(colors.muted))
                    .child(status),
            );
        let body = match problem {
            Some(problem) => div()
                .p_2()
                .text_color(crate::theme::paint(colors.error))
                .child(problem)
                .into_any_element(),
            None => uniform_list(
                "hex-rows",
                rows,
                cx.processor(|this, range: Range<usize>, _, cx| {
                    range
                        .map(|row| this.render_row(row, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element(),
        };
        div()
            .id("hex-editor")
            .key_context("HexEditor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .font_family(crate::MONOSPACE)
            .bg(crate::theme::paint(crate::theme::editor().background))
            .text_color(crate::theme::paint(crate::theme::editor().text))
            .child(header)
            .child(div().flex_1().min_h(px(0.)).child(body))
    }
}

/// Whether View in HEX shows `view`'s bytes now.
pub fn is_shown(workspace: &Workspace, view: &Entity<EditorView>, cx: &App) -> bool {
    workspace
        .panes()
        .iter()
        .any(|pane| pane.read(cx).hex_editor(view).is_some())
}
