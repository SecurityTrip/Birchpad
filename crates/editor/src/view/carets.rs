//! Moving the carets: motions, placing the caret, selecting, scrolling to a place.

use std::ops::Range as ByteRange;

use birchpad_cli::CaretTarget;
use birchpad_core::motion::{self, line_of, line_range};
use birchpad_core::{Range, Rope, Selection, Transaction};
use birchpad_ui::AppState;
use birchpad_view::DisplayMap;
use gpui_kit::{App, Context, px};

use crate::ZOOM_RANGE;
use crate::buffer::ReadOnly;

use super::commands::Motion;
use super::settings::ViewSettings;
use super::{EditorView, LastEdit};

impl EditorView {
    // --- Moving carets -------------------------------------------------------------------------

    pub(crate) fn move_carets(&mut self, motion: Motion, extend: bool, cx: &mut Context<Self>) {
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

    pub(crate) fn place_caret(&mut self, target: CaretTarget, cx: &mut Context<Self>) {
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
                let grid = self.display.config().grid();
                birchpad_view::pos_at_column(&text, line_range, 0, column - 1, grid).0
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
        let mut settings = ViewSettings::for_buffer(self.buffer.read(cx), cx);
        if let Some(own) = self.zoom {
            let shared = AppState::global(cx)
                .state
                .zoom
                .clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
            let own = own.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
            settings.font_size += px((own - shared) as f32);
        }
        settings
    }

    pub fn tab_width(&self) -> usize {
        self.display.config().tab_width
    }

    /// Shows `rows` blank rows above each `line` of `padding` (a line past the last: below the
    /// text), as Compare aligns two documents; empty shows none.
    pub fn set_padding(&mut self, padding: Vec<(usize, u32)>, cx: &mut Context<Self>) {
        let text = self.buffer.read(cx).doc().text().clone();
        self.display.set_padding(&text, padding);
        cx.notify();
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

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::single(Range::new(0, self.text(cx).len()));
        self.last_edit = LastEdit::None;
        self.pause_blink(cx);
        cx.notify();
    }
}

/// Home: to the first non-blank character of the line, or to the line start if already there
/// (Scintilla's VCHOMEWRAP); in a wrapped continuation row, first to the row start.
pub(crate) fn smart_home(display: &mut DisplayMap, text: &Rope, head: usize) -> usize {
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
