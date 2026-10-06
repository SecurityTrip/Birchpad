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
use crate::app_state::AppState;
use crate::buffer::MARK_STYLES;
use crate::commands::CommandRegistry;

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
        bail!("there is no style {style}; styles are numbered 1 to {MARK_STYLES}");
    }
    Ok(style - 1)
}

fn any_style(args: Option<AnyStyleArgs>) -> Result<Option<usize>> {
    args.unwrap_or_default().style.map(style_index).transpose()
}

pub(super) fn register_commands(registry: &mut CommandRegistry) {
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
    fn update_marks(
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

    fn bookmarked_lines(&self, cx: &gpui_kit::App) -> Vec<usize> {
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

#[cfg(test)]
#[allow(
    clippy::single_range_in_vec_init,
    reason = "decorations are lists of byte ranges, some with one range"
)]
mod tests {
    use birchpad_commands::Invocation;
    use birchpad_core::{LineEnding, Range, Selection};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use serde_json::json;

    use super::*;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{
        active_text, document_start, open_workspace, secondary, toggle_bookmark,
    };

    fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
        workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
    }

    fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            })
            .unwrap();
        cx.run_until_parked();
    }

    fn with_style(id: &str, style: usize) -> Invocation {
        Invocation::with_args(id, json!({ "style": style }))
    }

    fn bookmarks(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<usize> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, cx| view.bookmarked_lines(cx))
    }

    fn set_bookmarks(workspace: &Entity<Workspace>, lines: &[usize], cx: &mut VisualTestContext) {
        let view = view(workspace, cx);
        view.update(cx, |view, cx| {
            view.update_marks(cx, |marks, text| {
                marks.bookmarks.set_lines(text, lines.iter().copied());
            });
        });
    }

    fn styled(
        workspace: &Entity<Workspace>,
        style: usize,
        cx: &mut VisualTestContext,
    ) -> Vec<ByteRange<usize>> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, cx| {
            let marks = view.buffer.read(cx).marks();
            marks.styles[style - 1]
                .iter()
                .map(|(range, _)| range)
                .collect()
        })
    }

    fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| view.selection.primary())
    }

    fn select(workspace: &Entity<Workspace>, selection: Selection, cx: &mut VisualTestContext) {
        let view = view(workspace, cx);
        view.update(cx, |view, cx| {
            view.selection = selection;
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }

    /// Jump Up for style `n`: Ctrl+Shift+N, or Cmd+Ctrl+N on macOS.
    fn jump_up_key(n: usize) -> String {
        if cfg!(target_os = "macos") {
            format!("cmd-ctrl-{n}")
        } else {
            format!("ctrl-shift-{n}")
        }
    }

    #[gpui_kit::test]
    fn bookmarks_toggle_on_caret_lines_and_navigation_wraps(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("zero\none\ntwo\nthree\nfour");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes(&format!("down {}", toggle_bookmark()));
        cx.simulate_keystrokes(&format!("down down {}", toggle_bookmark()));
        assert_eq!(bookmarks(&workspace, cx), [1, 3]);

        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes("f2");
        assert_eq!(selection(&workspace, cx), Range::point(5), "line 1");
        cx.simulate_keystrokes("f2");
        assert_eq!(selection(&workspace, cx), Range::point(13), "line 3");
        cx.simulate_keystrokes("f2");
        assert_eq!(
            selection(&workspace, cx),
            Range::point(5),
            "wrapped to line 1"
        );
        cx.simulate_keystrokes("shift-f2");
        assert_eq!(
            selection(&workspace, cx),
            Range::point(13),
            "wrapped to line 3"
        );
        cx.simulate_keystrokes("shift-f2");
        assert_eq!(selection(&workspace, cx), Range::point(5));

        // With several carets: marks all their lines unless all are marked, then unmarks them.
        let carets = Selection::new([Range::point(1), Range::point(6), Range::point(7)], 0);
        select(&workspace, carets.clone(), cx);
        cx.simulate_keystrokes(toggle_bookmark());
        assert_eq!(bookmarks(&workspace, cx), [0, 1, 3]);
        cx.simulate_keystrokes(toggle_bookmark());
        assert_eq!(bookmarks(&workspace, cx), [3]);

        run(&workspace, Invocation::new("bookmark.clear-all"), cx);
        assert!(bookmarks(&workspace, cx).is_empty());
        // Without bookmarks, F2 stays put.
        cx.simulate_keystrokes("f2");
        assert_eq!(selection(&workspace, cx), Range::point(1));
    }

    #[gpui_kit::test]
    fn next_bookmark_expands_the_fold_hiding_it(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("fn a() {\n    x();\n}\nfn b() {}\n");
        run(
            &workspace,
            Invocation::with_args("language.set", json!({ "language": "rust" })),
            cx,
        );
        set_bookmarks(&workspace, &[1], cx);
        cx.simulate_keystrokes(document_start());
        run(&workspace, Invocation::new("view.fold-all"), cx);
        let view = view(&workspace, cx);
        assert!(view.read_with(cx, |view, _| view.display.is_hidden(1)));
        cx.simulate_keystrokes("f2");
        assert_eq!(selection(&workspace, cx), Range::point(9));
        assert!(!view.read_with(cx, |view, _| view.display.is_hidden(1)));
    }

    #[gpui_kit::test]
    fn bookmarked_line_operations_are_one_undo_step(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let eol = LineEnding::native().as_str();
        cx.simulate_input("a\nb\nc\nd\ne");
        set_bookmarks(&workspace, &[1, 3], cx);

        run(&workspace, Invocation::new("bookmark.copy-lines"), cx);
        assert_eq!(clipboard(cx), Some(format!("b{eol}d{eol}")));

        run(&workspace, Invocation::new("bookmark.inverse"), cx);
        assert_eq!(bookmarks(&workspace, cx), [0, 2, 4]);
        run(&workspace, Invocation::new("bookmark.inverse"), cx);
        assert_eq!(bookmarks(&workspace, cx), [1, 3]);

        run(&workspace, Invocation::new("bookmark.remove-lines"), cx);
        assert_eq!(active_text(&workspace, cx), "a\nc\ne");
        assert!(
            bookmarks(&workspace, cx).is_empty(),
            "removed with their lines"
        );
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(
            active_text(&workspace, cx),
            "a\nb\nc\nd\ne",
            "one undo step"
        );

        set_bookmarks(&workspace, &[0, 4], cx);
        run(
            &workspace,
            Invocation::new("bookmark.remove-unmarked-lines"),
            cx,
        );
        assert_eq!(active_text(&workspace, cx), "a\ne");
        assert_eq!(
            bookmarks(&workspace, cx),
            [0, 1],
            "kept lines keep their bookmarks"
        );
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");

        set_bookmarks(&workspace, &[2, 3], cx);
        run(&workspace, Invocation::new("bookmark.cut-lines"), cx);
        assert_eq!(active_text(&workspace, cx), "a\nb\ne");
        assert_eq!(clipboard(cx), Some(format!("c{eol}d{eol}")));
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");

        set_bookmarks(&workspace, &[0, 2], cx);
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("X\nY".into())));
        run(&workspace, Invocation::new("bookmark.paste-to-lines"), cx);
        assert_eq!(active_text(&workspace, cx), "X\nY\nb\nX\nY\nd\ne");
        assert_eq!(
            bookmarks(&workspace, cx),
            [0, 3],
            "on the first pasted line"
        );
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");
    }

    #[gpui_kit::test]
    fn token_styles_mark_jump_copy_and_clear(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let eol = LineEnding::native().as_str();
        cx.simulate_input("cat dog cat\ncatalog cat");
        cx.simulate_keystrokes(document_start());
        // The word at the caret, whole words only by default.
        cx.simulate_keystrokes(&secondary("alt-1"));
        assert_eq!(styled(&workspace, 1, cx), [0..3, 8..11, 20..23]);
        // A selection, here part of a word: still only whole-word occurrences.
        select(&workspace, Selection::single(Range::new(12, 15)), cx);
        cx.simulate_keystrokes(&secondary("alt-3"));
        assert_eq!(styled(&workspace, 3, cx), [0..3, 8..11, 20..23]);
        cx.update(|_, cx| {
            cx.global_mut::<AppState>()
                .settings
                .highlighting
                .token_style
                .whole_word = false;
        });
        cx.simulate_keystrokes(&secondary("alt-3"));
        assert_eq!(styled(&workspace, 3, cx), [0..3, 8..11, 12..15, 20..23]);
        // Style One Token styles only the token at the caret.
        select(&workspace, Selection::point(5), cx);
        run(&workspace, with_style("mark.style-one", 2), cx);
        assert_eq!(styled(&workspace, 2, cx), [4..7]);

        // Jump Down / Up select the next or previous occurrence, wrapping around.
        select(&workspace, Selection::point(0), cx);
        cx.simulate_keystrokes(&secondary("1"));
        assert_eq!(selection(&workspace, cx), Range::new(8, 11));
        cx.simulate_keystrokes(&secondary("1"));
        assert_eq!(selection(&workspace, cx), Range::new(20, 23));
        cx.simulate_keystrokes(&secondary("1"));
        assert_eq!(selection(&workspace, cx), Range::new(0, 3), "wrapped");
        cx.simulate_keystrokes(&jump_up_key(1));
        assert_eq!(
            selection(&workspace, cx),
            Range::new(20, 23),
            "wrapped back"
        );
        run(&workspace, Invocation::new("mark.jump-up"), cx);
        assert_eq!(selection(&workspace, cx), Range::new(12, 15), "any style");
        run(&workspace, Invocation::new("mark.jump-up"), cx);
        assert_eq!(selection(&workspace, cx), Range::new(8, 11));
        run(&workspace, Invocation::new("mark.jump-up"), cx);
        assert_eq!(selection(&workspace, cx), Range::new(4, 7), "style 2");

        run(&workspace, with_style("mark.copy-styled-text", 2), cx);
        assert_eq!(clipboard(cx), Some(format!("dog{eol}")));
        run(&workspace, Invocation::new("mark.copy-styled-text"), cx);
        assert_eq!(
            clipboard(cx),
            Some(format!("cat{eol}dog{eol}cat{eol}cat{eol}cat{eol}"))
        );

        cx.simulate_keystrokes(&secondary("alt-shift-1"));
        assert!(styled(&workspace, 1, cx).is_empty());
        assert!(!styled(&workspace, 3, cx).is_empty());
        cx.simulate_keystrokes(&secondary("alt-shift-0"));
        assert!(styled(&workspace, 2, cx).is_empty());
        assert!(styled(&workspace, 3, cx).is_empty());
    }

    #[gpui_kit::test]
    fn styles_and_bookmarks_follow_edits_undo_and_redo(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("one two\nthree two");
        cx.simulate_keystrokes(document_start());
        select(&workspace, Selection::point(5), cx);
        cx.simulate_keystrokes(&secondary("alt-1"));
        assert_eq!(styled(&workspace, 1, cx), [4..7, 14..17]);
        set_bookmarks(&workspace, &[1], cx);

        // Typing right before a styled word does not extend it; Enter above moves both.
        cx.simulate_keystrokes(document_start());
        cx.simulate_input("x\n");
        assert_eq!(styled(&workspace, 1, cx), [6..9, 16..19]);
        assert_eq!(bookmarks(&workspace, cx), [2]);
        cx.simulate_keystrokes(&secondary("z"));
        assert_eq!(styled(&workspace, 1, cx), [4..7, 14..17]);
        assert_eq!(bookmarks(&workspace, cx), [1]);
        cx.simulate_keystrokes(&secondary("y"));
        assert_eq!(styled(&workspace, 1, cx), [6..9, 16..19]);
        assert_eq!(bookmarks(&workspace, cx), [2]);
    }

    /// X11 and Wayland report Ctrl+Alt+Shift+1 as `ctrl-alt-!` and Ctrl+Shift+1 as `ctrl-!`.
    #[cfg(target_os = "linux")]
    #[gpui_kit::test]
    fn shifted_digits_work_as_linux_reports_them(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("cat dog cat");
        cx.simulate_keystrokes(document_start());
        cx.simulate_keystrokes("ctrl-alt-1");
        assert_eq!(styled(&workspace, 1, cx), [0..3, 8..11]);
        cx.simulate_keystrokes("ctrl-!");
        assert_eq!(
            selection(&workspace, cx),
            Range::new(8, 11),
            "Jump Up wraps"
        );
        cx.simulate_keystrokes("ctrl-alt-!");
        assert!(styled(&workspace, 1, cx).is_empty());
    }

    #[gpui_kit::test]
    fn bad_style_numbers_are_reported(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&with_style("mark.style-all", 6), window, cx)
        });
        assert!(result.unwrap_err().to_string().contains("style 6"));
    }
}
