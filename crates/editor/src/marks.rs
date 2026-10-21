//! Search menu: token styles (Style All Occurrences of Token, Style One Token, Clear Style,
//! Jump Up/Down, Copy Styled Text) and Bookmark. The decorations live in the buffer
//! (`DocumentMarks`); the work on text is done by `birchpad_core`.

use std::ops::Range as ByteRange;

use anyhow::{Result, bail};
use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::search::{Query, Searcher, token_at};
use birchpad_core::{Assoc, Rope, ops};
use gpui_kit::{ClipboardItem, Context};
use serde::Deserialize;

use super::{EditorView, LastEdit};
use crate::buffer::MARK_STYLES;
use birchpad_ui::AppState;
use birchpad_ui::CommandRegistry;

use crate::EditorCommands as _;

/// `{ "style": 1..5 }`.
#[derive(Deserialize)]
struct StyleArgs {
    style: usize,
}

/// `{ "style": 1..5 }`, or no style for any (all) styles.
#[derive(Deserialize, Default)]
struct AnyStyleArgs {
    style: Option<usize>,
}

/// The index of style 1..5.
fn style_index(style: usize) -> Result<usize> {
    if !(1..=MARK_STYLES).contains(&style) {
        bail!(
            "{}",
            t!(
                "editor.marks.there-is-no-style-styles-are-numbered-1-to",
                style = style,
                MARK_STYLES = MARK_STYLES
            )
        );
    }
    Ok(style - 1)
}

fn any_style(args: Option<AnyStyleArgs>) -> Result<Option<usize>> {
    args.unwrap_or_default().style.map(style_index).transpose()
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("mark.style-all", |this, args: StyleArgs, _, cx| {
        let style = style_index(args.style)?;
        this.style_token(style, true, cx);
        Ok(())
    });
    registry.editor("mark.style-one", |this, args: StyleArgs, _, cx| {
        let style = style_index(args.style)?;
        this.style_token(style, false, cx);
        Ok(())
    });
    registry.editor("mark.clear", |this, args: StyleArgs, _, cx| {
        let style = style_index(args.style)?;
        this.update_marks(cx, |marks, _| marks.styles[style].clear());
        Ok(())
    });
    registry.editor("mark.clear-all", |this, (), _, cx| {
        this.update_marks(cx, |marks, _| {
            marks.styles.iter_mut().for_each(|s| s.clear())
        });
        Ok(())
    });
    registry.editor("mark.jump-up", |this, args: Option<AnyStyleArgs>, _, cx| {
        this.jump_to_style(any_style(args)?, false, cx);
        Ok(())
    });
    registry.editor(
        "mark.jump-down",
        |this, args: Option<AnyStyleArgs>, _, cx| {
            this.jump_to_style(any_style(args)?, true, cx);
            Ok(())
        },
    );
    registry.editor(
        "mark.copy-styled-text",
        |this, args: Option<AnyStyleArgs>, _, cx| {
            this.copy_styled_text(any_style(args)?, cx);
            Ok(())
        },
    );
    registry.editor("bookmark.toggle", |this, (), _, cx| {
        this.toggle_bookmarks(cx);
        Ok(())
    });
    registry.editor("bookmark.next", |this, (), _, cx| {
        this.go_to_bookmark(true, cx);
        Ok(())
    });
    registry.editor("bookmark.previous", |this, (), _, cx| {
        this.go_to_bookmark(false, cx);
        Ok(())
    });
    registry.editor("bookmark.clear-all", |this, (), _, cx| {
        this.update_marks(cx, |marks, _| marks.bookmarks.clear());
        Ok(())
    });
    registry.editor("bookmark.copy-lines", |this, (), _, cx| {
        this.copy_bookmarked_lines(cx);
        Ok(())
    });
    registry.editor("bookmark.cut-lines", |this, (), _, cx| {
        if this.is_editable(cx) && this.copy_bookmarked_lines(cx) {
            this.remove_bookmarked_lines(cx);
        }
        Ok(())
    });
    registry.editor("bookmark.paste-to-lines", |this, (), _, cx| {
        this.paste_to_bookmarked_lines(cx);
        Ok(())
    });
    registry.editor("bookmark.remove-lines", |this, (), _, cx| {
        this.remove_bookmarked_lines(cx);
        Ok(())
    });
    registry.editor("bookmark.remove-unmarked-lines", |this, (), _, cx| {
        let lines = this.bookmarked_lines(cx);
        this.edit_lines(cx, |text| ops::remove_other_lines(text, &lines));
        Ok(())
    });
    registry.editor("bookmark.inverse", |this, (), _, cx| {
        this.update_marks(cx, |marks, text| {
            let lines = marks.bookmarks.lines(text);
            marks
                .bookmarks
                .set_lines(text, ops::other_lines(line_count(text), &lines));
        });
        Ok(())
    });
}

impl EditorView {
    pub fn update_marks(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut crate::buffer::DocumentMarks, &Rope),
    ) {
        self.buffer
            .update(cx, |buffer, cx| buffer.update_marks(cx, change));
    }

    /// Style All Occurrences of Token (`all`) or Style One Token: the selection, or the word
    /// at the caret, gets `style`.
    fn style_token(&mut self, style: usize, all: bool, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let primary = self.selection.primary();
        let Some(token) = token_at(&text, primary.from()..primary.to()) else {
            return;
        };
        let ranges = if all {
            let matching = &AppState::global(cx).settings.highlighting.token_style;
            let query = Query {
                pattern: text.slice(token).to_string(),
                match_case: matching.match_case,
                whole_word: matching.whole_word,
                ..Query::default()
            };
            let Ok(searcher) = Searcher::new(&query) else {
                return;
            };
            searcher.find_all(&text)
        } else {
            vec![token]
        };
        self.update_marks(cx, |marks, _| marks.styles[style].insert_all(ranges, ()));
    }

    /// Jump Down (`down`) or Up to the next occurrence of `style` (of any style for `None`),
    /// wrapping around, and selects it.
    fn jump_to_style(&mut self, style: Option<usize>, down: bool, cx: &mut Context<Self>) {
        let from = self.selection.primary().from();
        let marks = self.buffer.read(cx).marks();
        let target = match style {
            Some(style) if down => marks.styles[style].next_after(from),
            Some(style) => marks.styles[style].previous_before(from),
            None => {
                let sets: Vec<_> = marks.styles.iter().collect();
                if down {
                    birchpad_core::next_in_any(&sets, from)
                } else {
                    birchpad_core::previous_in_any(&sets, from)
                }
            }
        };
        if let Some(range) = target {
            self.select_range(range, cx);
        }
    }

    /// Copy Styled Text: the text of every occurrence of `style` (of all styles for `None`),
    /// one per line.
    fn copy_styled_text(&mut self, style: Option<usize>, cx: &mut Context<Self>) {
        let buffer = self.buffer.read(cx);
        let marks = buffer.marks();
        let ranges: Vec<ByteRange<usize>> = marks
            .styles
            .iter()
            .enumerate()
            .filter(|(index, _)| style.is_none_or(|style| style == *index))
            .flat_map(|(_, set)| set.iter().map(|(range, _)| range))
            .collect();
        if ranges.is_empty() {
            return;
        }
        let copied = ops::copy_ranges(buffer.doc().text(), ranges, buffer.doc().line_ending());
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
    }

    pub fn bookmarked_lines(&self, cx: &gpui_kit::App) -> Vec<usize> {
        let buffer = self.buffer.read(cx);
        buffer.marks().bookmarks.lines(buffer.doc().text())
    }

    /// Ctrl+F2: bookmarks the lines of all carets, or removes their bookmarks if they all
    /// have one.
    fn toggle_bookmarks(&mut self, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let mut lines: Vec<usize> = self
            .selection
            .iter()
            .map(|range| line_of(&text, range.head))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        self.update_marks(cx, |marks, text| {
            let all_marked = lines
                .iter()
                .all(|&line| marks.bookmarks.contains(text, line));
            for &line in &lines {
                if all_marked {
                    marks.bookmarks.remove(text, line);
                } else {
                    marks.bookmarks.add(text, line);
                }
            }
        });
    }

    /// F2 / Shift+F2: to the start of the next or previous bookmarked line, wrapping around
    /// and expanding folds that hide it.
    fn go_to_bookmark(&mut self, next: bool, cx: &mut Context<Self>) {
        let buffer = self.buffer.read(cx);
        let text = buffer.doc().text();
        let line = line_of(text, self.selection.primary().head);
        let bookmarks = &buffer.marks().bookmarks;
        let target = if next {
            bookmarks.next_after(text, line)
        } else {
            bookmarks.previous_before(text, line)
        };
        if let Some(target) = target {
            let pos = line_range(text, target).start;
            self.go_to(pos, cx);
        }
    }

    /// Copy Bookmarked Lines; returns false if no line is bookmarked.
    fn copy_bookmarked_lines(&mut self, cx: &mut Context<Self>) -> bool {
        let lines = self.bookmarked_lines(cx);
        if lines.is_empty() {
            return false;
        }
        let doc = self.buffer.read(cx).doc();
        let copied = ops::copy_lines(doc.text(), &lines, doc.line_ending());
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        true
    }

    /// Remove Bookmarked Lines: the lines go, and so do their bookmarks (Notepad++ does not
    /// move them to the next line).
    fn remove_bookmarked_lines(&mut self, cx: &mut Context<Self>) {
        let lines = self.bookmarked_lines(cx);
        if lines.is_empty() || !self.is_editable(cx) {
            return;
        }
        self.update_marks(cx, |marks, _| marks.bookmarks.clear());
        self.edit_lines(cx, |text| ops::remove_lines(text, &lines));
    }

    /// Paste to (Replace) Bookmarked Lines: each bookmarked line's content becomes the
    /// clipboard text; the bookmarks stay on the first line of each pasted block.
    fn paste_to_bookmarked_lines(&mut self, cx: &mut Context<Self>) {
        let Some(clipboard) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let lines = self.bookmarked_lines(cx);
        if lines.is_empty() || !self.is_editable(cx) {
            return;
        }
        let text = self.text(cx).clone();
        let Some(transaction) = ops::replace_lines(&text, &lines, &clipboard) else {
            return;
        };
        let starts: Vec<usize> = lines
            .iter()
            .map(|&line| {
                let start = line_range(&text, line).start;
                transaction.changes().map_pos(start, Assoc::Before)
            })
            .collect();
        self.apply(transaction, LastEdit::None, cx);
        self.update_marks(cx, |marks, text| {
            marks
                .bookmarks
                .set_lines(text, starts.iter().map(|&start| line_of(text, start)));
        });
    }

    /// Applies an operation on whole lines as one undo step.
    fn edit_lines(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&Rope) -> Option<birchpad_core::Transaction>,
    ) {
        if !self.is_editable(cx) {
            return;
        }
        let text = self.text(cx).clone();
        if let Some(transaction) = op(&text) {
            self.apply(transaction, LastEdit::None, cx);
        }
    }
}
