//! Edit > Expand Selection and Shrink Selection: each selection grows to the smallest piece of
//! syntax around it (`birchpad_syntax::expand_selection`), and shrinks back the way it grew.
//! Without a syntax tree (plain text, a user defined language) a selection grows to its word,
//! then its line, then the whole text.

use std::ops::Range as ByteRange;

use birchpad_core::motion::{line_of, line_range, word_at};
use birchpad_core::{Range, Rope};
use birchpad_syntax::Syntax;
use gpui_kit::Context;

use super::EditorView;

/// What `range` grows to: the syntax around it, else its word, its line, the whole text; the
/// range itself once it is the whole text.
pub fn expanded(syntax: Option<&Syntax>, text: &Rope, range: ByteRange<usize>) -> ByteRange<usize> {
    let len = text.len();
    let fits = |found: &ByteRange<usize>| {
        found.end <= len
            && text.is_char_boundary(found.start)
            && text.is_char_boundary(found.end)
            && found.start <= range.start
            && range.end <= found.end
            && found.len() > range.len()
    };
    // A tree a reparse has not caught up with may not fit the text.
    if let Some(found) =
        syntax.and_then(|syntax| birchpad_syntax::expand_selection(syntax, range.clone()))
        && fits(&found)
    {
        return found;
    }
    if range.is_empty() {
        let word = word_at(text, range.start);
        if !word.is_empty() {
            return word;
        }
    }
    let first = line_of(text, range.start);
    if first == line_of(text, range.end) {
        let line = line_range(text, first);
        if fits(&line) {
            return line;
        }
    }
    0..len
}

impl EditorView {
    /// Edit > Expand Selection.
    pub fn expand_selection(&mut self, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let before = self.selection.clone();
        let after = {
            let syntax = self.buffer.read(cx).syntax();
            before.transform(|range| {
                let grown = expanded(syntax, &text, range.from()..range.to());
                if range.is_backward() {
                    Range::new(grown.end, grown.start)
                } else {
                    Range::new(grown.start, grown.end)
                }
            })
        };
        if after == before {
            return;
        }
        let mut steps = match self.expansions.take() {
            Some((steps, last)) if last == before => steps,
            _ => Vec::new(),
        };
        steps.push(before);
        self.selection = after.clone();
        self.expansions = Some((steps, after));
        self.request_autoscroll(cx);
    }

    /// Edit > Shrink Selection: the selection before the last Expand Selection; nothing once
    /// the selection changed otherwise.
    pub fn shrink_selection(&mut self, cx: &mut Context<Self>) {
        let Some((mut steps, last)) = self.expansions.take() else {
            return;
        };
        if last != self.selection {
            return;
        }
        let Some(previous) = steps.pop() else {
            return;
        };
        self.selection = previous.clone();
        if !steps.is_empty() {
            self.expansions = Some((steps, previous));
        }
        self.request_autoscroll(cx);
    }
}
