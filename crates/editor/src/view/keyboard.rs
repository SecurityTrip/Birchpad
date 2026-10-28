//! Text from the keyboard and input methods: the platform's input handler, in UTF-16 offsets.

use std::ops::Range as ByteRange;

use birchpad_core::Range;
use birchpad_ui::AppState;
use gpui_kit::{App, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use super::EditorView;

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
        // Keys such as Ctrl+E may type a control character: nothing, unless asked for.
        let filtered;
        let text = if AppState::global(cx).settings.editor.prevent_control_typing {
            filtered = without_control_characters(text);
            if filtered.is_empty() && !text.is_empty() {
                return;
            }
            filtered.as_str()
        } else {
            text
        };
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
    pub(crate) fn utf16_from_bytes(&self, range: &ByteRange<usize>, cx: &App) -> ByteRange<usize> {
        let text = self.text(cx);
        text.byte_to_utf16_idx(range.start)..text.byte_to_utf16_idx(range.end)
    }

    pub(crate) fn bytes_from_utf16(&self, range: &ByteRange<usize>, cx: &App) -> ByteRange<usize> {
        let text = self.text(cx);
        let len = text.len_utf16();
        text.utf16_to_byte_idx(range.start.min(len))..text.utf16_to_byte_idx(range.end.min(len))
    }
}

/// Byte offset in `text` of the given UTF-16 code unit offset.
pub(crate) fn utf16_to_byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// `text` without the control characters keys may type (C0, but tab and line breaks).
pub fn without_control_characters(text: &str) -> String {
    text.chars()
        .filter(|&ch| !(ch < ' ' && !matches!(ch, '\t' | '\n' | '\r')))
        .collect()
}
