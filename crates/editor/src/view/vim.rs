//! The editor's opt-in modal key handling (ADR 0072).

use std::ops::Range as ByteRange;

use birchpad_core::motion::{self, line_of, line_range, line_range_with_break};
use birchpad_core::{Rope, Selection};
use birchpad_ui::AppState;
use gpui_kit::{App, Context, Div, FocusHandle, Global, Keystroke, Subscription, div, prelude::*};

use super::{EditorView, LastEdit, commands::Motion};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Change,
    Delete,
    Yank,
}

#[derive(Default)]
pub(super) struct State {
    mode: Mode,
    count: usize,
    operator: Option<(Operator, usize)>,
    go: bool,
}

struct Register {
    text: String,
    linewise: bool,
}

impl Global for Register {}

pub(super) fn enabled(cx: &App) -> bool {
    AppState::global(cx).settings.editor.vim_mode
}

pub(super) fn intercept(focus: &FocusHandle, cx: &mut Context<EditorView>) -> Subscription {
    let editor = cx.entity().downgrade();
    let focus = focus.clone();
    cx.intercept_keystrokes(move |event, window, cx| {
        if enabled(cx)
            && focus.is_focused(window)
            && editor
                .update(cx, |editor, cx| editor.vim_key(&event.keystroke, cx))
                .unwrap_or(false)
        {
            cx.stop_propagation();
        }
    })
}

fn cursor_on_line(text: &Rope, position: usize) -> usize {
    let line = line_range(text, line_of(text, position));
    if position >= line.end && !line.is_empty() {
        motion::prev_boundary(text, line.end)
    } else {
        position
    }
}

fn motion_for(key: &str) -> Option<Motion> {
    Some(match key {
        "h" | "left" => Motion::Left,
        "j" | "down" => Motion::Down,
        "k" | "up" => Motion::Up,
        "l" | "right" => Motion::Right,
        "w" => Motion::WordRight,
        "b" => Motion::WordLeft,
        "$" | "end" => Motion::End,
        _ => return None,
    })
}

impl EditorView {
    pub(super) fn vim_accepts_text(&self, cx: &App) -> bool {
        !enabled(cx) || self.vim.mode == Mode::Insert
    }

    pub(super) fn vim_indicator(&self, cx: &App) -> Option<Div> {
        enabled(cx).then(|| {
            let mode = match self.vim.mode {
                Mode::Normal => t!("vim.normal"),
                Mode::Insert => t!("vim.insert"),
                Mode::Visual => t!("vim.visual"),
            };
            div().px_2().text_sm().child(mode)
        })
    }

    fn vim_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let held = &key.modifiers;
        if held.control
            && !held.alt
            && !held.platform
            && !held.shift
            && key.key == "r"
            && self.vim.mode == Mode::Normal
        {
            self.vim = State::default();
            self.redo(cx);
            return true;
        }
        if held.control || held.alt || held.platform {
            self.vim.operator = None;
            self.vim.count = 0;
            self.vim.go = false;
            return false;
        }
        if key.key == "escape" {
            if self.vim.mode == Mode::Insert && (self.completion.is_some() || self.marked.is_some())
            {
                return false;
            }
            let cursor = self.vim_cursor(cx);
            let text = self.text(cx);
            let line = line_range(text, line_of(text, cursor));
            let position = if self.vim.mode == Mode::Insert && cursor > line.start {
                motion::prev_boundary(text, cursor)
            } else {
                cursor_on_line(text, cursor)
            };
            self.vim = State::default();
            self.go_to(position, cx);
            return true;
        }
        if self.vim.mode == Mode::Insert {
            return false;
        }
        let shifted;
        let typed = match key.key_char.as_deref().filter(|s| !s.is_empty()) {
            Some(typed) => typed,
            None if held.shift && key.key.len() == 1 => {
                shifted = if key.key == "4" {
                    "$".to_owned()
                } else {
                    key.key.to_ascii_uppercase()
                };
                &shifted
            }
            None => &key.key,
        };
        if let [digit @ b'0'..=b'9'] = typed.as_bytes()
            && (*digit != b'0' || self.vim.count > 0)
        {
            self.vim.count = (self.vim.count * 10 + usize::from(*digit - b'0')).min(1000);
            return true;
        }
        let entered_count = std::mem::take(&mut self.vim.count);
        let count = entered_count.max(1);
        let operator = self.vim.operator.take();
        let go = std::mem::take(&mut self.vim.go);
        let start = self.vim_cursor(cx);
        if go && typed != "g" {
            return true;
        }
        let next_operator = match typed {
            "c" => Some(Operator::Change),
            "d" => Some(Operator::Delete),
            "y" => Some(Operator::Yank),
            _ => None,
        };
        if let Some(next) = next_operator {
            match (self.vim.mode, operator) {
                (Mode::Visual, _) => {
                    let range = self.selection.primary();
                    self.vim_operate(next, range.from()..range.to(), false, cx);
                }
                (_, Some((pending, prefix))) if pending == next => {
                    let text = self.text(cx);
                    let first = line_of(text, start);
                    let last =
                        (first + (count * prefix).min(1000) - 1).min(motion::line_count(text) - 1);
                    let mut range = line_range_with_break(text, first).start
                        ..line_range_with_break(text, last).end;
                    if next == Operator::Change {
                        range.end = line_range(text, last).end;
                    }
                    self.vim_operate(next, range, true, cx);
                }
                (_, None) => self.vim.operator = Some((next, count)),
                _ => {}
            }
            return true;
        }
        if typed == "g" && !go {
            self.vim.go = true;
            self.vim.operator = operator;
            self.vim.count = count;
            return true;
        }
        let count = operator.map_or(count, |(_, prefix)| (prefix * count).min(1000));
        let motion_count = if typed == "G" && entered_count == 0 && operator.is_none() {
            0
        } else {
            count
        };
        if let Some(end) = self.vim_motion(typed, start, motion_count, cx) {
            if let Some((operator, _)) = operator {
                let text = self.text(cx);
                let from_line = line_of(text, start);
                let to_line = line_of(text, end);
                let linewise = matches!(typed, "j" | "k" | "up" | "down" | "G" | "g");
                let range = if linewise {
                    let last = from_line.max(to_line);
                    let end = if operator == Operator::Change {
                        line_range(text, last).end
                    } else {
                        line_range_with_break(text, last).end
                    };
                    line_range_with_break(text, from_line.min(to_line)).start..end
                } else {
                    start.min(end)..start.max(end)
                };
                self.vim_operate(operator, range, linewise, cx);
            } else if self.vim.mode == Mode::Visual {
                let range = self.selection.primary();
                let text = self.text(cx);
                let anchor = if range.is_backward() {
                    motion::prev_boundary(text, range.anchor)
                } else {
                    range.anchor
                };
                self.vim_select(anchor, cursor_on_line(text, end), cx);
            } else {
                let end = cursor_on_line(self.text(cx), end);
                self.selection = Selection::point(end);
                self.request_autoscroll(cx);
            }
            return true;
        }
        if operator.is_some() {
            return true;
        }
        match typed {
            "v" if self.vim.mode == Mode::Visual => {
                self.vim.mode = Mode::Normal;
                self.go_to(start, cx);
            }
            "v" => {
                self.vim.mode = Mode::Visual;
                self.vim_select(start, start, cx);
            }
            "x" if self.vim.mode == Mode::Visual => {
                let range = self.selection.primary();
                self.vim_operate(Operator::Delete, range.from()..range.to(), false, cx);
            }
            _ if self.vim.mode == Mode::Visual => {}
            "i" | "a" | "I" | "A" if self.is_editable(cx) => {
                let text = self.text(cx);
                let line = line_range(text, line_of(text, start));
                let position = match typed {
                    "a" => motion::next_boundary(text, start).min(line.end),
                    "I" => {
                        line.start
                            + text
                                .slice(line.clone())
                                .chars()
                                .take_while(|ch| *ch == ' ' || *ch == '\t')
                                .map(char::len_utf8)
                                .sum::<usize>()
                    }
                    "A" => line.end,
                    _ => start,
                };
                self.go_to(position, cx);
                self.vim.mode = Mode::Insert;
            }
            "x" => {
                let text = self.text(cx);
                let line_end = line_range(text, line_of(text, start)).end;
                let mut end = start;
                for _ in 0..count {
                    end = motion::next_boundary(text, end).min(line_end);
                }
                self.vim_operate(Operator::Delete, start..end, false, cx);
            }
            "p" | "P" => self.vim_put(start, typed == "P", cx),
            "u" => {
                for _ in 0..count {
                    self.undo(cx);
                }
                let position = cursor_on_line(self.text(cx), self.selection.primary().head);
                self.go_to(position, cx);
            }
            _ => {
                return typed.chars().count() == 1
                    || matches!(typed, "space" | "tab" | "enter" | "backspace" | "delete");
            }
        }
        cx.notify();
        true
    }

    fn vim_cursor(&self, cx: &App) -> usize {
        let range = self.selection.primary();
        if self.vim.mode == Mode::Visual && !range.is_empty() && !range.is_backward() {
            motion::prev_boundary(self.text(cx), range.head)
        } else {
            range.head
        }
    }

    fn vim_select(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let (from, to) = if head < anchor {
            (motion::next_boundary(text, anchor), head)
        } else {
            (anchor, motion::next_boundary(text, head))
        };
        self.select_range(from..to, cx);
    }

    fn vim_motion(
        &mut self,
        key: &str,
        from: usize,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.text(cx).clone();
        match key {
            "0" => return Some(line_range(&text, line_of(&text, from)).start),
            "g" => {
                return Some(
                    line_range(
                        &text,
                        count.saturating_sub(1).min(motion::line_count(&text) - 1),
                    )
                    .start,
                );
            }
            "G" => {
                let last = motion::line_count(&text) - 1;
                let line = if count == 0 {
                    last
                } else {
                    (count - 1).min(last)
                };
                return Some(line_range(&text, line).start);
            }
            _ => {}
        }
        let motion = motion_for(key)?;
        let previous = std::mem::replace(&mut self.selection, Selection::point(from));
        let line = line_range(&text, line_of(&text, from));
        for _ in 0..count {
            self.move_carets(motion, false, cx);
            if matches!(motion, Motion::Left | Motion::Right) {
                let head = self.selection.primary().head.clamp(line.start, line.end);
                self.selection = Selection::point(head);
            }
        }
        let end = self.selection.primary().head;
        self.selection = previous;
        Some(end)
    }

    fn vim_operate(
        &mut self,
        operator: Operator,
        range: ByteRange<usize>,
        linewise: bool,
        cx: &mut Context<Self>,
    ) {
        if operator != Operator::Yank && !self.is_editable(cx) {
            return;
        }
        if range.is_empty() {
            if operator == Operator::Change && linewise {
                self.vim.mode = Mode::Insert;
                self.go_to(range.start, cx);
            }
            return;
        }
        let text = self.text(cx);
        let eol = self.buffer.read(cx).doc().line_ending().as_str();
        let mut contents = text.slice(range.clone()).to_string();
        if linewise && !contents.ends_with(['\r', '\n']) {
            contents.push_str(eol);
        }
        let mut start = range.start;
        if linewise
            && operator == Operator::Delete
            && range.end == text.len()
            && start > 0
            && !text
                .slice(range.clone())
                .chars()
                .last()
                .is_some_and(|ch| ch == '\r' || ch == '\n')
        {
            start = motion::prev_boundary(text, start);
        }
        cx.set_global(Register {
            text: contents,
            linewise,
        });
        self.vim.mode = if operator == Operator::Change {
            Mode::Insert
        } else {
            Mode::Normal
        };
        if operator != Operator::Yank {
            self.select_range(start..range.end, cx);
            self.insert("", LastEdit::None, cx);
        }
        if operator != Operator::Change {
            let cursor = cursor_on_line(self.text(cx), start);
            self.go_to(cursor, cx);
        }
    }

    fn vim_put(&mut self, start: usize, before: bool, cx: &mut Context<Self>) {
        if !self.is_editable(cx) {
            return;
        }
        let Some(register) = cx.try_global::<Register>() else {
            return;
        };
        let mut contents = register.text.clone();
        let text = self.text(cx);
        let line = line_of(text, start);
        let position = if register.linewise {
            let range = line_range_with_break(text, line);
            if text.len() == 0 {
                0
            } else if before {
                range.start
            } else {
                if range.end == line_range(text, line).end {
                    contents.insert_str(0, self.buffer.read(cx).doc().line_ending().as_str());
                }
                range.end
            }
        } else if before {
            start
        } else {
            motion::next_boundary(text, start).min(line_range(text, line).end)
        };
        self.go_to(position, cx);
        self.insert(&contents, LastEdit::None, cx);
        let cursor = if self.text(cx).len() == position {
            position
        } else {
            cursor_on_line(self.text(cx), self.selection.primary().head)
        };
        self.go_to(cursor, cx);
    }
}
