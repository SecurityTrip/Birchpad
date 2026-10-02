//! Several selections at once: rectangular (column) selections, Multi-select Next and All,
//! Begin/End Select, and inserting text or numbers into a column (the Column Editor).
//!
//! A rectangle is kept in lines and cells (`birchpad_view::Block`) and turned into one range
//! per line; the work on text is done by `birchpad_core`.

use birchpad_core::motion::{self, line_count, line_of, line_range};
use birchpad_core::ops::{self, Base, Leading, MatchOptions, NumberSequence};
use birchpad_core::{ChangeSet, Range, Rope, Selection, Transaction};
use birchpad_view::{Block, BlockPoint, DisplayMap, LayoutConfig};
use gpui_kit::{ClipboardItem, Context};
use serde::{Deserialize, Serialize};

use super::{EditorView, LastEdit};
use crate::commands::CommandRegistry;

/// A rectangle and the selection it made; the rectangle is active only while the view's
/// selection is still that one, so any other way of changing the selection leaves column
/// mode without having to say so.
#[derive(Debug, Clone)]
pub(super) struct BlockState {
    block: Block,
    selection: Selection,
}

/// The start that the first Begin/End Select remembered.
#[derive(Debug, Clone, Copy)]
pub(super) struct BeginSelect {
    pos: usize,
    virtual_cells: usize,
    column: bool,
}

#[derive(Debug, Clone, Copy)]
enum BlockMotion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
struct MultiSelectArgs {
    #[serde(default)]
    match_case: bool,
    #[serde(default)]
    whole_word: bool,
}

impl From<MultiSelectArgs> for MatchOptions {
    fn from(args: MultiSelectArgs) -> Self {
        Self {
            match_case: args.match_case,
            whole_word: args.whole_word,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LeadingName {
    #[default]
    None,
    Zeros,
    Spaces,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) enum FormatName {
    #[default]
    Dec,
    Hex,
    Oct,
    Bin,
}

impl From<FormatName> for Base {
    fn from(format: FormatName) -> Self {
        match format {
            FormatName::Dec => Base::Dec,
            FormatName::Hex => Base::Hex,
            FormatName::Oct => Base::Oct,
            FormatName::Bin => Base::Bin,
        }
    }
}

const fn one_i64() -> i64 {
    1
}

const fn one_usize() -> usize {
    1
}

const fn yes() -> bool {
    true
}

/// Number to Insert, as `edit.column-insert` takes it.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NumberArgs {
    #[serde(default = "one_i64")]
    pub(super) initial: i64,
    #[serde(default = "one_i64")]
    pub(super) step: i64,
    #[serde(default = "one_usize")]
    pub(super) repeat: usize,
    #[serde(default)]
    pub(super) leading: LeadingName,
    #[serde(default)]
    pub(super) format: FormatName,
    #[serde(default = "yes")]
    pub(super) uppercase: bool,
}

impl From<NumberArgs> for NumberSequence {
    fn from(args: NumberArgs) -> Self {
        Self {
            initial: args.initial,
            step: args.step,
            repeat: args.repeat,
            leading: match args.leading {
                LeadingName::None => Leading::None,
                LeadingName::Zeros => Leading::Zeros,
                LeadingName::Spaces => Leading::Spaces,
            },
            base: args.format.into(),
            uppercase: args.uppercase,
        }
    }
}

/// What `edit.column-insert` inserts: `{ "text": "..." }` or the numbers.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub(super) enum ColumnInsertArgs {
    Text { text: String },
    Numbers(NumberArgs),
}

/// Marks clipboard text copied from a rectangle.
#[derive(Serialize, Deserialize)]
struct RectangleClipboard {
    rectangular: bool,
    rows: usize,
}

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    let motions = [
        ("select.block-left", BlockMotion::Left),
        ("select.block-right", BlockMotion::Right),
        ("select.block-up", BlockMotion::Up),
        ("select.block-down", BlockMotion::Down),
        ("select.block-home", BlockMotion::Home),
        ("select.block-end", BlockMotion::End),
        ("select.block-page-up", BlockMotion::PageUp),
        ("select.block-page-down", BlockMotion::PageDown),
    ];
    for (id, motion) in motions {
        registry.editor(id, move |this, (), _, cx| {
            this.extend_block(motion, cx);
            Ok(())
        });
    }
    registry.editor("edit.cancel-selection", |this, (), _, cx| {
        this.cancel_selection(cx);
        Ok(())
    });
    registry.editor("edit.begin-end-select", |this, (), _, cx| {
        this.begin_end_select(false, cx);
        Ok(())
    });
    registry.editor("edit.begin-end-select-column", |this, (), _, cx| {
        this.begin_end_select(true, cx);
        Ok(())
    });
    registry.editor(
        "edit.multi-select-next",
        |this, args: Option<MultiSelectArgs>, _, cx| {
            let options = args.unwrap_or_default().into();
            this.multi_select(cx, |text, selection| {
                ops::select_next(text, selection, options)
            });
            Ok(())
        },
    );
    registry.editor(
        "edit.multi-select-all",
        |this, args: Option<MultiSelectArgs>, _, cx| {
            let options = args.unwrap_or_default().into();
            this.multi_select(cx, |text, selection| {
                ops::select_all(text, selection, options)
            });
            Ok(())
        },
    );
    registry.editor(
        "edit.multi-select-skip",
        |this, args: Option<MultiSelectArgs>, _, cx| {
            let options = args.unwrap_or_default().into();
            this.multi_select(cx, |text, selection| {
                ops::skip_to_next(text, selection, options)
            });
            Ok(())
        },
    );
    registry.editor("edit.multi-select-undo", |this, (), _, cx| {
        this.undo_multi_select(cx);
        Ok(())
    });
    registry.editor("edit.column-editor", |_, (), window, cx| {
        super::column_editor::open(cx.entity(), window, cx);
        Ok(())
    });
    registry.editor(
        "edit.column-insert",
        |this, args: ColumnInsertArgs, _, cx| {
            this.column_insert(args, cx);
            Ok(())
        },
    );
}

/// The rows of a rectangle copied to the clipboard, if `item` is one. The marker is checked
/// against the text, in case another program replaced the text but not the marker.
pub(super) fn rectangle_rows(item: &ClipboardItem, text: &str) -> Option<Vec<String>> {
    let marker: RectangleClipboard = serde_json::from_str(item.metadata()?).ok()?;
    if !marker.rectangular {
        return None;
    }
    let mut rows = split_lines(text);
    if rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    (rows.len() == marker.rows).then_some(rows)
}

/// Lines of `text` split at CRLF, LF or CR.
fn split_lines(text: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['\r', '\n']) {
        rows.push(rest[..at].to_owned());
        let skip = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        rest = &rest[at + skip..];
    }
    rows.push(rest.to_owned());
    rows
}

impl EditorView {
    /// The rectangle, if the selection is still the one it made.
    pub(super) fn active_block(&self) -> Option<Block> {
        self.block
            .as_ref()
            .filter(|state| state.selection == self.selection)
            .map(|state| state.block)
    }

    /// Selects `block`: one range per line, the caret's line primary.
    pub(super) fn set_block(&mut self, block: Block, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        self.selection = block.selection(&text, &mut self.display);
        self.block = Some(BlockState {
            block,
            selection: self.selection.clone(),
        });
        self.goal_column = None;
        self.last_edit = LastEdit::None;
        self.request_autoscroll(cx);
    }

    /// After typing or deleting in `block`: a zero-width rectangle at the primary caret's
    /// column, over the carets the edit left (which may not all be at that column when tabs
    /// or wide characters are involved).
    pub(super) fn keep_thin_block(&mut self, block: Block, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let primary = self.selection.primary();
        let column = self.display.column(&text, primary.head) + primary.head_virtual;
        let block = Block::new(
            BlockPoint::new(block.anchor.line, column),
            BlockPoint::new(block.head.line, column),
        );
        self.block = Some(BlockState {
            block,
            selection: self.selection.clone(),
        });
    }

    /// The rectangle that Alt+Shift+arrows extend: the active one, or one from the primary
    /// range's anchor to its head.
    fn current_block(&mut self, cx: &Context<Self>) -> Block {
        if let Some(block) = self.active_block() {
            return block;
        }
        let text = self.text(cx).clone();
        let primary = self.selection.primary();
        Block::new(
            self.display
                .block_point(&text, primary.anchor, primary.anchor_virtual),
            self.display
                .block_point(&text, primary.head, primary.head_virtual),
        )
    }

    /// The visible line `steps` lines above (negative) or below `line`.
    fn visible_line_from(&self, text: &Rope, line: usize, steps: isize) -> usize {
        let last = line_count(text) - 1;
        let mut line = line;
        for _ in 0..steps.unsigned_abs() {
            let next = if steps < 0 {
                (0..line).rev().find(|&l| !self.display.is_hidden(l))
            } else {
                (line + 1..=last).find(|&l| !self.display.is_hidden(l))
            };
            match next {
                Some(next) => line = next,
                None => break,
            }
        }
        line
    }

    /// Alt+Shift+arrows, Home, End, Page Up and Page Down: moves the rectangle's caret
    /// corner. Left and Right go by characters inside the text and by cells in virtual space.
    fn extend_block(&mut self, motion: BlockMotion, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let block = self.current_block(cx);
        let mut head = block.head;
        let page = self.page_rows();
        let range = line_range(&text, head.line);
        let column_of = |display: &mut DisplayMap, pos: usize| display.column(&text, pos);
        match motion {
            BlockMotion::Up => head.line = self.visible_line_from(&text, head.line, -1),
            BlockMotion::Down => head.line = self.visible_line_from(&text, head.line, 1),
            BlockMotion::PageUp => head.line = self.visible_line_from(&text, head.line, -page),
            BlockMotion::PageDown => head.line = self.visible_line_from(&text, head.line, page),
            BlockMotion::Left => {
                let (mut pos, _) = self
                    .display
                    .pos_at_line_column(&text, head.line, head.column);
                let end_column = column_of(&mut self.display, range.end);
                if head.column > end_column {
                    head.column -= 1;
                } else {
                    while pos > range.start && column_of(&mut self.display, pos) >= head.column {
                        pos = motion::prev_boundary(&text, pos);
                    }
                    head.column = head.column.min(column_of(&mut self.display, pos));
                }
            }
            BlockMotion::Right => {
                let (mut pos, _) = self
                    .display
                    .pos_at_line_column(&text, head.line, head.column);
                while pos < range.end && column_of(&mut self.display, pos) <= head.column {
                    pos = motion::next_boundary(&text, pos);
                }
                let column = column_of(&mut self.display, pos);
                head.column = if column > head.column {
                    column
                } else {
                    head.column + 1
                };
            }
            BlockMotion::Home => {
                let indent = column_of(&mut self.display, motion::indent_end(&text, head.line));
                head.column = if head.column == indent { 0 } else { indent };
            }
            BlockMotion::End => head.column = column_of(&mut self.display, range.end),
        }
        self.set_block(Block::new(block.anchor, head), cx);
    }

    /// Esc: a rectangle becomes a caret at its caret corner; several selections become the
    /// primary one, as Scintilla's `SCI_CANCEL`.
    fn cancel_selection(&mut self, cx: &mut Context<Self>) {
        if self.active_block().is_some() {
            let head = self.selection.primary().head;
            self.selection = Selection::point(head);
        } else if self.selection.ranges().len() > 1 {
            self.selection = Selection::single(self.selection.primary());
        } else {
            return;
        }
        self.block = None;
        self.request_autoscroll(cx);
    }

    /// Backspace and Delete in a rectangle. Deleting the rectangle's text leaves a caret on
    /// each line; with carets, each deletes one character on its line (never the line break).
    /// A caret in virtual space moves one cell left on Backspace and stays on Delete.
    pub(super) fn delete_in_block(
        &mut self,
        cx: &mut Context<Self>,
        target: fn(&Rope, usize) -> usize,
    ) {
        let block = self.active_block().expect("called in column mode");
        let text = self.text(cx).clone();
        let wide = self.selection.iter().any(|range| !range.is_empty());
        if !wide {
            self.selection = self.selection.transform(|range| {
                let to = target(&text, range.head);
                let backward = to < range.head;
                if range.head_virtual > 0 {
                    return if backward {
                        Range::new(range.head, range.head)
                            .with_virtual(range.head_virtual - 1, range.head_virtual)
                    } else {
                        range
                    };
                }
                let line = line_range(&text, line_of(&text, range.head));
                Range::new(range.head, to.clamp(line.start, line.end))
            });
        }
        let transaction = Transaction::replace_selections(&text, &self.selection, "")
            .expect("the selection lies on character boundaries");
        if transaction.changes().is_identity() {
            // Only virtual space moved: no undo step.
            if let Some(carets) = transaction.selection() {
                self.selection = carets.clone();
            }
            self.pause_blink(cx);
        } else {
            self.apply(transaction, LastEdit::Deleting, cx);
        }
        self.keep_thin_block(block, cx);
        cx.notify();
    }

    /// The rectangle as clipboard text (each row ended by a line break, as Scintilla copies
    /// it), marked so that pasting it back inserts a rectangle.
    pub(super) fn block_clipboard_item(&self, cx: &Context<Self>) -> ClipboardItem {
        let doc = self.buffer.read(cx).doc();
        let eol = doc.line_ending().as_str();
        let mut text = String::new();
        for range in self.selection.iter() {
            text.push_str(&doc.text().slice(range.from()..range.to()).to_string());
            text.push_str(eol);
        }
        ClipboardItem::new_string_with_json_metadata(
            text,
            RectangleClipboard {
                rectangular: true,
                rows: self.selection.ranges().len(),
            },
        )
    }

    /// Pastes `rows` as a rectangle: the selection is deleted, then each row goes into the
    /// next line at the caret's column, filling short lines with spaces and adding lines at
    /// the end of the document if needed. One undo step.
    pub(super) fn paste_rectangle(&mut self, rows: &[String], cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let doc = self.buffer.read(cx).doc();
        let (text, eol) = (doc.text().clone(), doc.line_ending());
        let block = self.active_block();
        let has_selection = self.selection.iter().any(|range| !range.is_empty());
        // Delete what is selected first; the paste goes where the rectangle (or the primary
        // selection) started.
        let (deletion, middle, start) = if has_selection {
            let deletion = Transaction::replace_selections(&text, &self.selection, "")
                .expect("the selection lies on character boundaries");
            let mut middle = text.clone();
            deletion.changes().apply(&mut middle);
            let carets = deletion.selection().expect("replacing sets carets");
            let start = if block.is_some() {
                carets.ranges()[0]
            } else {
                carets.primary()
            };
            (deletion.changes().clone(), middle, start)
        } else {
            let start = if block.is_some() {
                self.selection.ranges()[0]
            } else {
                self.selection.primary()
            };
            (ChangeSet::identity(text.len()), text.clone(), start)
        };
        let tab_width = self.display.config().tab_width;
        let mut display = DisplayMap::new(
            &middle,
            LayoutConfig {
                tab_width,
                wrap_width: None,
            },
        );
        let first_line = line_of(&middle, start.from());
        let column = display.column(&middle, start.from()) + start.from_virtual();
        let lines = line_count(&middle);
        let targets: Vec<Range> = (first_line..(first_line + rows.len()).min(lines))
            .map(|line| {
                let (pos, virtual_cells) = display.pos_at_line_column(&middle, line, column);
                Range::virtual_point(pos, virtual_cells)
            })
            .collect();
        let paste = ops::insert_rows(&middle, &targets, rows, column, eol);
        let caret = paste
            .selection()
            .map_or(Range::point(0), |carets| carets.primary());
        let transaction = Transaction::new(deletion.compose(paste.changes().clone()))
            .with_selection(Selection::single(caret));
        self.apply(transaction, LastEdit::None, cx);
    }

    /// Begin/End Select (and its column mode twin): the first call remembers the caret, the
    /// second selects from there to the caret. A call of the other kind in between does
    /// nothing, as in Notepad++.
    fn begin_end_select(&mut self, column: bool, cx: &mut Context<Self>) {
        let primary = self.selection.primary();
        match self.begin_select {
            None => {
                self.begin_select = Some(BeginSelect {
                    pos: primary.head,
                    virtual_cells: primary.head_virtual,
                    column,
                });
            }
            Some(start) if start.column == column => {
                self.begin_select = None;
                let text = self.text(cx).clone();
                if column {
                    let anchor = self
                        .display
                        .block_point(&text, start.pos, start.virtual_cells);
                    let head = self
                        .display
                        .block_point(&text, primary.head, primary.head_virtual);
                    self.set_block(Block::new(anchor, head), cx);
                } else {
                    self.selection = Selection::single(Range::new(start.pos, primary.head));
                    self.goal_column = None;
                    self.request_autoscroll(cx);
                }
            }
            Some(_) => {}
        }
    }

    /// Whether Begin/End Select is waiting for its second call (for a check mark in menus).
    #[cfg_attr(not(test), expect(dead_code, reason = "menus have no check marks yet"))]
    pub(super) fn begin_select_pending(&self) -> Option<bool> {
        self.begin_select.map(|start| start.column)
    }

    /// Keeps Begin/End Select's start on its text through an edit.
    pub(super) fn map_begin_select(&mut self, changes: &ChangeSet) {
        if let Some(start) = &mut self.begin_select {
            let range = Range::virtual_point(start.pos, start.virtual_cells).map(changes);
            start.pos = range.head;
            start.virtual_cells = range.head_virtual;
        }
    }

    /// Multi-select Next, All and Skip: applies `op` and remembers the order ranges were
    /// added in, for Undo the Latest Added Multi-Select.
    fn multi_select(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&Rope, &Selection) -> Option<Selection>,
    ) {
        let text = self.text(cx).clone();
        let Some(selection) = op(&text, &self.selection) else {
            return;
        };
        let mut order = self.multi_order();
        order.retain(|range| selection.ranges().contains(range));
        for range in selection.iter() {
            if !order.contains(range) {
                order.push(*range);
            }
        }
        // The primary range counts as the latest.
        let primary = selection.primary();
        order.retain(|range| *range != primary);
        order.push(primary);
        self.selection = selection.clone();
        self.multi_order = Some((order, selection));
        self.goal_column = None;
        self.last_edit = LastEdit::None;
        self.request_autoscroll(cx);
    }

    /// The ranges of the selection in the order they were added, the primary one last.
    fn multi_order(&self) -> Vec<Range> {
        if let Some((order, selection)) = &self.multi_order
            && *selection == self.selection
        {
            return order.clone();
        }
        let primary = self.selection.primary();
        let mut order: Vec<Range> = self
            .selection
            .iter()
            .copied()
            .filter(|range| *range != primary)
            .collect();
        order.push(primary);
        order
    }

    /// Undo the Latest Added Multi-Select: drops the primary range; the one added before it
    /// becomes primary.
    fn undo_multi_select(&mut self, cx: &mut Context<Self>) {
        let mut order = self.multi_order();
        let Some(selection) = self.selection.remove(self.selection.primary_index()) else {
            return;
        };
        order.pop();
        let primary = order
            .last()
            .and_then(|latest| selection.iter().position(|range| range == latest));
        let selection = match primary {
            Some(index) => selection.with_primary_index(index),
            None => selection,
        };
        self.selection = selection.clone();
        self.multi_order = Some((order, selection));
        self.request_autoscroll(cx);
    }

    /// Where the Column Editor inserts: each line of the rectangle, each caret or selection,
    /// or with one caret, every line from the caret's to the last at the caret's column.
    fn column_targets(&mut self, cx: &Context<Self>) -> Selection {
        if self.active_block().is_some() || self.selection.ranges().len() > 1 {
            return self.selection.clone();
        }
        let text = self.text(cx).clone();
        let primary = self.selection.primary();
        let start = self
            .display
            .block_point(&text, primary.from(), primary.from_virtual());
        let last = line_count(&text) - 1;
        Block::new(start, BlockPoint::new(last, start.column)).selection(&text, &mut self.display)
    }

    /// `edit.column-insert`: the Column Editor's text or numbers into every target row, as
    /// one undo step.
    pub(super) fn column_insert(&mut self, args: ColumnInsertArgs, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let targets = self.column_targets(cx);
        let count = targets.ranges().len();
        let rows = match args {
            ColumnInsertArgs::Text { text } => vec![text; count],
            ColumnInsertArgs::Numbers(numbers) => NumberSequence::from(numbers).generate(count),
        };
        let doc = self.buffer.read(cx).doc();
        let (text, eol) = (doc.text().clone(), doc.line_ending());
        let transaction = ops::insert_rows(&text, targets.ranges(), &rows, 0, eol);
        self.apply(transaction, LastEdit::None, cx);
    }

    /// Sorting with a rectangle keys lines by its columns; the rectangle stays over the same
    /// lines and columns.
    pub(super) fn sort_by_block(
        &mut self,
        key: ops::SortKey,
        descending: bool,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<bool> {
        let Some(block) = self.active_block() else {
            return Ok(false);
        };
        if !self.is_editable(cx) {
            return Ok(true);
        }
        let doc = self.buffer.read(cx).doc();
        let (text, eol) = (doc.text().clone(), doc.line_ending());
        if let Some(transaction) =
            ops::sort_lines_by_columns(&text, &self.selection, key, descending, eol)?
        {
            self.apply(transaction, LastEdit::None, cx);
        }
        self.set_block(block, cx);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_rows_split_at_any_line_break() {
        assert_eq!(split_lines("a\r\nb\nc\rd"), ["a", "b", "c", "d"]);
        let item = ClipboardItem::new_string_with_json_metadata(
            "ab\r\ncd\r\n".to_owned(),
            RectangleClipboard {
                rectangular: true,
                rows: 2,
            },
        );
        assert_eq!(
            rectangle_rows(&item, "ab\r\ncd\r\n"),
            Some(vec!["ab".to_owned(), "cd".to_owned()])
        );
        assert_eq!(rectangle_rows(&item, "other text\n"), None, "stale marker");
        let plain = ClipboardItem::new_string("ab\ncd\n".to_owned());
        assert_eq!(rectangle_rows(&plain, "ab\ncd\n"), None);
    }

    #[test]
    fn column_insert_arguments() {
        let text: ColumnInsertArgs = serde_json::from_str(r#"{ "text": "x" }"#).unwrap();
        assert_eq!(text, ColumnInsertArgs::Text { text: "x".into() });
        let numbers: ColumnInsertArgs =
            serde_json::from_str(r#"{ "initial": 5, "format": "hex", "leading": "zeros" }"#)
                .unwrap();
        let ColumnInsertArgs::Numbers(numbers) = numbers else {
            panic!("numbers expected");
        };
        assert_eq!(
            (
                numbers.initial,
                numbers.step,
                numbers.repeat,
                numbers.format
            ),
            (5, 1, 1, FormatName::Hex)
        );
        assert_eq!(numbers.leading, LeadingName::Zeros);
    }
}

#[cfg(test)]
mod gpui_tests {
    use birchpad_commands::Invocation;
    use birchpad_core::LineEnding;
    use gpui_kit::{
        AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
        VisualTestContext, point,
    };
    use serde_json::json;

    use super::*;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{
        active_text, begin_end_column, block, column_editor, document_start, open_workspace,
        secondary,
    };

    fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
        workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
    }

    fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx).unwrap();
        });
        cx.run_until_parked();
    }

    fn ranges(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Range> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| view.selection.ranges().to_vec())
    }

    fn primary(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| view.selection.primary())
    }

    fn in_block(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Block> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| view.active_block())
    }

    /// The window point at `column` cells of `line` (an unwrapped, visible line).
    fn cell_point(
        workspace: &Entity<Workspace>,
        line: usize,
        column: usize,
        cx: &mut VisualTestContext,
    ) -> Point<Pixels> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("a frame was drawn");
            let row = layout.rows.iter().find(|row| row.row.line == line).unwrap();
            point(
                layout.column_zero + layout.metrics.cell * column as f32,
                row.y + layout.metrics.line_height / 2.,
            )
        })
    }

    fn set_text(text: &str, cx: &mut VisualTestContext) {
        cx.simulate_input(text);
        cx.simulate_keystrokes(document_start());
    }

    #[gpui_kit::test]
    fn alt_drag_selects_a_rectangle_into_virtual_space(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("abcdef\nab\nabcdef", cx);
        let from = cell_point(&workspace, 0, 1, cx);
        let to = cell_point(&workspace, 2, 4, cx);
        let middle = cell_point(&workspace, 1, 4, cx);
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::alt());
        cx.simulate_mouse_move(middle, MouseButton::Left, Modifiers::alt());
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::alt());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::alt());
        assert_eq!(
            ranges(&workspace, cx),
            [
                Range::new(1, 4),
                Range::new(8, 9).with_virtual(0, 2),
                Range::new(11, 14),
            ]
        );
        assert_eq!(
            primary(&workspace, cx),
            Range::new(11, 14),
            "the caret's line"
        );

        // Typing replaces the rectangle on every line and leaves a caret on each.
        cx.simulate_input("X");
        assert_eq!(active_text(&workspace, cx), "aXef\naX\naXef");
        cx.simulate_input("Y");
        assert_eq!(active_text(&workspace, cx), "aXYef\naXY\naXYef");
        assert!(in_block(&workspace, cx).is_some(), "still in column mode");
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(
            active_text(&workspace, cx),
            "abcdef\nab\nabcdef",
            "typing in a rectangle is one undo step"
        );
    }

    #[gpui_kit::test]
    fn keys_extend_a_rectangle_and_typing_fills_virtual_space(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("abcd\n\nab", cx);
        cx.simulate_keystrokes(&format!(
            "right right right {} {}",
            block("down"),
            block("down")
        ));
        assert_eq!(
            ranges(&workspace, cx),
            [
                Range::point(3),
                Range::virtual_point(5, 3),
                Range::virtual_point(8, 1),
            ]
        );
        cx.simulate_input("|");
        assert_eq!(active_text(&workspace, cx), "abc|d\n   |\nab |");
        // Backspace deletes the typed bar on every line.
        cx.simulate_keystrokes("backspace");
        assert_eq!(active_text(&workspace, cx), "abcd\n   \nab ");
        cx.simulate_keystrokes(&secondary("z"));
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "abcd\n\nab");

        // Right goes on into virtual space, Left comes back; Up shrinks the rectangle.
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&format!(
            "end {} {} {}",
            block("right"),
            block("right"),
            block("down")
        ));
        assert_eq!(
            ranges(&workspace, cx),
            [
                Range::new(4, 4).with_virtual(0, 2),
                Range::new(5, 5).with_virtual(4, 6),
            ]
        );
        cx.simulate_keystrokes(&format!("{} {}", block("left"), block("up")));
        assert_eq!(
            ranges(&workspace, cx),
            [Range::new(4, 4).with_virtual(0, 1)]
        );

        // Esc leaves column mode with a caret at the caret corner.
        cx.simulate_keystrokes(&format!("{} escape", block("down")));
        assert_eq!(ranges(&workspace, cx), [Range::point(5)]);
        assert!(in_block(&workspace, cx).is_none());
    }

    #[gpui_kit::test]
    fn backspace_in_virtual_space_moves_the_carets(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("ab\nabcd", cx);
        cx.simulate_keystrokes(&format!("down end {}", block("up")));
        assert_eq!(
            ranges(&workspace, cx),
            [Range::virtual_point(2, 2), Range::point(7)]
        );
        cx.simulate_keystrokes("backspace");
        assert_eq!(active_text(&workspace, cx), "ab\nabc");
        assert_eq!(
            ranges(&workspace, cx),
            [Range::virtual_point(2, 1), Range::point(6)]
        );
        // Delete never joins lines.
        cx.simulate_keystrokes("delete");
        assert_eq!(active_text(&workspace, cx), "ab\nabc");
    }

    #[gpui_kit::test]
    fn copying_and_pasting_a_rectangle(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("ab12\ncd34\nef", cx);
        cx.simulate_keystrokes(&format!(
            "right right {} {} {}",
            block("down"),
            block("right"),
            block("right")
        ));
        cx.simulate_keystrokes(&secondary("c"));
        let eol = LineEnding::native().as_str();
        let clipboard = cx.read_from_clipboard().unwrap();
        assert_eq!(
            clipboard.text().unwrap(),
            format!("12{eol}34{eol}"),
            "other programs get the rows as text"
        );

        // At the end of "ef": the first row goes there, the second on a new line.
        cx.simulate_keystrokes(&secondary("end"));
        let end = if cfg!(target_os = "macos") {
            "cmd-down"
        } else {
            "ctrl-end"
        };
        cx.simulate_keystrokes(end);
        cx.simulate_keystrokes(&secondary("v"));
        assert_eq!(
            active_text(&workspace, cx),
            format!("ab12\ncd34\nef12{eol}  34")
        );
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "ab12\ncd34\nef");

        // Over a rectangle of the same size: replaces it.
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&format!(
            "{} {} {}",
            block("down"),
            block("right"),
            block("right")
        ));
        cx.simulate_keystrokes(&secondary("v"));
        assert_eq!(active_text(&workspace, cx), "1212\n3434\nef");

        // Text copied as a stream pastes into every line of a rectangle.
        cx.write_to_clipboard(ClipboardItem::new_string("-".to_owned()));
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&format!("{} {}", block("down"), block("down")));
        cx.simulate_keystrokes(&secondary("v"));
        assert_eq!(active_text(&workspace, cx), "-1212\n-3434\n-ef");
    }

    #[gpui_kit::test]
    fn multi_select_all_renames_every_occurrence(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("let foo = foo_bar + Foo;\nfoo();", cx);
        cx.simulate_keystrokes("right right right right right");
        let all = Invocation::with_args(
            "edit.multi-select-all",
            json!({ "match-case": true, "whole-word": true }),
        );
        run(&workspace, all, cx);
        assert_eq!(
            ranges(&workspace, cx),
            [Range::new(4, 7), Range::new(25, 28)]
        );
        cx.simulate_input("value");
        assert_eq!(
            active_text(&workspace, cx),
            "let value = foo_bar + Foo;\nvalue();"
        );
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(
            active_text(&workspace, cx),
            "let foo = foo_bar + Foo;\nfoo();"
        );
    }

    #[gpui_kit::test]
    fn multi_select_next_undo_latest_and_escape(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("ab x ab x ab", cx);
        let next = Invocation::new("edit.multi-select-next");
        run(&workspace, next.clone(), cx);
        assert_eq!(ranges(&workspace, cx), [Range::new(0, 2)], "the word first");
        run(&workspace, next.clone(), cx);
        run(&workspace, next, cx);
        assert_eq!(
            ranges(&workspace, cx),
            [Range::new(0, 2), Range::new(5, 7), Range::new(10, 12)]
        );
        assert_eq!(primary(&workspace, cx), Range::new(10, 12));
        run(&workspace, Invocation::new("edit.multi-select-undo"), cx);
        assert_eq!(ranges(&workspace, cx), [Range::new(0, 2), Range::new(5, 7)]);
        assert_eq!(
            primary(&workspace, cx),
            Range::new(5, 7),
            "the one added before"
        );
        run(&workspace, Invocation::new("edit.multi-select-skip"), cx);
        assert_eq!(
            ranges(&workspace, cx),
            [Range::new(0, 2), Range::new(10, 12)]
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(ranges(&workspace, cx), [Range::new(10, 12)]);
    }

    #[gpui_kit::test]
    fn begin_end_select_in_both_modes(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("abcd\nefgh\nijkl", cx);
        cx.simulate_keystrokes(&format!("right {}", secondary("shift-b")));
        let view = view(&workspace, cx);
        assert_eq!(
            view.read_with(cx, |view, _| view.begin_select_pending()),
            Some(false)
        );
        cx.simulate_keystrokes(&format!("down down {}", secondary("shift-b")));
        assert_eq!(ranges(&workspace, cx), [Range::new(1, 11)]);

        // Right first collapses the selection to its end (line 2, column 1).
        cx.simulate_keystrokes(&format!(
            "right {} right right up {}",
            begin_end_column(),
            begin_end_column()
        ));
        assert_eq!(
            ranges(&workspace, cx),
            [Range::new(6, 8), Range::new(11, 13)]
        );
        assert!(in_block(&workspace, cx).is_some());
        assert_eq!(
            view.read_with(cx, |view, _| view.begin_select_pending()),
            None
        );
    }

    #[gpui_kit::test]
    fn sorting_a_csv_by_a_rectangle(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("name,age\nbob,30\namy,04\ncat,15", cx);
        // The age column of the three records.
        cx.simulate_keystrokes("down right right right right");
        cx.simulate_keystrokes(&format!(
            "{} {} {} {}",
            block("down"),
            block("down"),
            block("right"),
            block("right")
        ));
        let sort = Invocation::with_args("edit.sort-lines", json!({ "by": "integer" }));
        run(&workspace, sort, cx);
        let eol = LineEnding::native().as_str();
        assert_eq!(
            active_text(&workspace, cx),
            format!("name,age\namy,04{eol}cat,15{eol}bob,30")
        );
        assert!(in_block(&workspace, cx).is_some(), "the rectangle stays");
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(
            active_text(&workspace, cx),
            "name,age\nbob,30\namy,04\ncat,15"
        );
    }

    #[gpui_kit::test]
    fn column_editor_numbers_a_thousand_lines_in_one_step(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let original = vec!["x"; 1000].join("\n");
        set_text(&original, cx);
        let view = view(&workspace, cx);
        // The dialog's fields, filled and confirmed.
        cx.update(|window, cx| {
            let editor = cx
                .new(|cx| super::super::column_editor::ColumnEditor::new(view.clone(), window, cx));
            editor.update(cx, |editor, cx| {
                editor.fill_numbers("1", "1", "", window, cx);
                editor.insert(cx).unwrap();
            });
        });
        cx.run_until_parked();
        let text = active_text(&workspace, cx);
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 1000);
        assert_eq!(lines[0], "1   x", "left-aligned to the widest number");
        assert_eq!(lines[99], "100 x");
        assert_eq!(lines[999], "1000x");
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), original, "one undo step");

        // Alt+C (Cmd+Alt+C on macOS) opens the dialog.
        cx.simulate_keystrokes(column_editor());
        let open = cx.update(|window, cx| {
            use gpui_kit::component::WindowExt as _;
            window.has_active_dialog(cx)
        });
        assert!(open);
    }

    #[gpui_kit::test]
    fn column_insert_into_a_rectangle_and_carets(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("a\nbbb\nc", cx);
        cx.simulate_keystrokes(&format!("right {} {}", block("down"), block("down")));
        let insert = Invocation::with_args(
            "edit.column-insert",
            json!({ "initial": 9, "leading": "zeros", "format": "dec" }),
        );
        run(&workspace, insert, cx);
        assert_eq!(active_text(&workspace, cx), "a09\nb10bb\nc11");
        let text = Invocation::with_args("edit.column-insert", json!({ "text": "|" }));
        cx.simulate_keystrokes(document_start());
        run(&workspace, text, cx);
        assert_eq!(active_text(&workspace, cx), "|a09\n|b10bb\n|c11");
    }

    #[gpui_kit::test]
    fn input_methods_compose_at_the_primary_caret_only(cx: &mut TestAppContext) {
        use gpui_kit::EntityInputHandler as _;
        let (workspace, cx) = open_workspace(cx);
        set_text("a\nb", cx);
        cx.simulate_keystrokes(&format!("end {}", block("down")));
        let view = view(&workspace, cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.replace_and_mark_text_in_range(None, "か", None, window, cx);
                view.replace_text_in_range(None, "漢", window, cx);
            });
        });
        assert_eq!(active_text(&workspace, cx), "a\nb漢");
        assert_eq!(ranges(&workspace, cx).len(), 2, "the other caret stays");
        cx.simulate_input("!");
        assert_eq!(active_text(&workspace, cx), "a!\nb漢!");
    }

    #[gpui_kit::test]
    #[ignore = "a timing; run in release: cargo test --release -- --ignored --nocapture"]
    fn typing_with_a_thousand_carets(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text(&vec!["some text on a line"; 1000].join("\n"), cx);
        let view = view(&workspace, cx);
        view.update(cx, |view, cx| {
            view.set_block(
                Block::new(BlockPoint::new(0, 4), BlockPoint::new(999, 4)),
                cx,
            );
        });
        cx.run_until_parked();
        let started = std::time::Instant::now();
        for _ in 0..20 {
            cx.simulate_input("x");
        }
        let per_key = started.elapsed() / 20;
        eprintln!("typing with 1000 carets: {per_key:?} per keystroke, with a frame");
        assert!(active_text(&workspace, cx).starts_with("somexxxxxxxxxxxxxxxxxxxx text"));
    }
}
