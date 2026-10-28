//! Editing through the view: typing, new lines, indentation, deleting, undo, the clipboard.

use std::ops::Range as ByteRange;

use birchpad_config::AutoIndent;
use birchpad_core::motion::{self, line_of, line_range};
use birchpad_core::{Edit, Range, Rope, Selection, Transaction, UndoGrouping};
use birchpad_ui::AppState;
use birchpad_view::tab_advance;
use gpui_kit::{App, ClipboardItem, Context};

use crate::multi;

use super::{EditorView, LastEdit};

impl EditorView {
    // --- Editing -------------------------------------------------------------------------------

    /// False while the buffer is loading, has a decoding problem or is a read-only file.
    pub fn is_editable(&self, cx: &App) -> bool {
        self.buffer.read(cx).read_only().is_none()
    }

    pub(crate) fn apply(
        &mut self,
        transaction: Transaction,
        kind: LastEdit,
        cx: &mut Context<Self>,
    ) {
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
        self.column_to_multi_editing(cx);
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
    pub(crate) fn type_text(&mut self, typed: &str, cx: &mut Context<Self>) {
        self.column_to_multi_editing(cx);
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
    pub(crate) fn newline(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn go_to_matching_brace(&mut self, select: bool, cx: &mut Context<Self>) {
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
    pub(crate) fn indent(&mut self, more: bool, cx: &mut Context<Self>) {
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
    pub(crate) fn insert_tab(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn replace_range(
        &mut self,
        range: ByteRange<usize>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn delete_with(
        &mut self,
        cx: &mut Context<Self>,
        target: fn(&Rope, usize) -> usize,
    ) {
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

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let origin = Some(cx.entity_id());
        if let Some(transaction) = self.buffer.update(cx, |buffer, cx| buffer.undo(origin, cx)) {
            self.restore_from_history(&transaction, cx);
        }
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let origin = Some(cx.entity_id());
        if let Some(transaction) = self.buffer.update(cx, |buffer, cx| buffer.redo(origin, cx)) {
            self.restore_from_history(&transaction, cx);
        }
    }

    pub(crate) fn restore_from_history(
        &mut self,
        transaction: &Transaction,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        if self.active_block().is_some() {
            let item = self.block_clipboard_item(cx);
            cx.write_to_clipboard(item);
            return true;
        }
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return true;
        }
        // Preferences > Editing > Copy/Cut Line without selection: the caret's line, with
        // its line break.
        if let Some(line) = self.unselected_line(cx) {
            let text = self.text(cx).slice(line).to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        false
    }

    /// Whether every caret is in its line's indentation, past its start, with nothing
    /// selected.
    pub fn in_indentation(&self, cx: &App) -> bool {
        let text = self.text(cx);
        self.selection.iter().all(|range| {
            let line_start = line_range(text, line_of(text, range.head)).start;
            range.is_empty()
                && range.head > line_start
                && text
                    .slice(line_start..range.head)
                    .chars()
                    .all(|ch| ch == ' ' || ch == '\t')
        })
    }

    /// The caret's whole line, when nothing is selected and Copy and Cut take it.
    pub(crate) fn unselected_line(&self, cx: &App) -> Option<ByteRange<usize>> {
        if !AppState::global(cx)
            .settings
            .editor
            .copy_line_without_selection
            || self.selection.iter().any(|range| !range.is_empty())
        {
            return None;
        }
        let text = self.text(cx);
        let line = line_of(text, self.selection.primary().head);
        Some(motion::line_range_with_break(text, line))
    }

    pub(crate) fn cut(&mut self, cx: &mut Context<Self>) {
        if self.copy(cx) {
            self.insert("", LastEdit::None, cx);
        } else if let Some(line) = self.unselected_line(cx)
            && !line.is_empty()
            && self.is_editable(cx)
        {
            let text = self.text(cx).clone();
            if let Ok(transaction) = Transaction::from_edits(&text, [Edit::replace(line, "")]) {
                self.apply(transaction, LastEdit::None, cx);
            }
        }
    }

    pub(crate) fn paste(&mut self, cx: &mut Context<Self>) {
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
}
