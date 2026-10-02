//! Folding in a view: which folds are collapsed, the commands of View > Fold, and keeping
//! carets out of hidden lines.
//!
//! Fold points belong to the buffer (from its syntax tree or indentation); which of them are
//! collapsed belongs to each view, as in Scintilla, so a clone in the other pane can show the
//! same document unfolded.

use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::{ChangeSet, Range};
use birchpad_view::{
    Fold, fold_at, folds_from_ranges, hidden_lines, innermost_containing, shift_folds,
};
use gpui_kit::{App, Context};
use serde::Deserialize;

use super::EditorView;
use crate::commands::CommandRegistry;

#[derive(Deserialize)]
struct LevelArgs {
    level: usize,
}

pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("view.fold-all", |this, (), _, cx| {
        this.fold_all(true, cx);
        Ok(())
    });
    registry.editor("view.unfold-all", |this, (), _, cx| {
        this.fold_all(false, cx);
        Ok(())
    });
    registry.editor("view.fold-level", |this, args: LevelArgs, _, cx| {
        this.fold_level(args.level, true, cx);
        Ok(())
    });
    registry.editor("view.unfold-level", |this, args: LevelArgs, _, cx| {
        this.fold_level(args.level, false, cx);
        Ok(())
    });
    registry.editor("view.fold-current", |this, (), _, cx| {
        this.fold_current(true, cx);
        Ok(())
    });
    registry.editor("view.unfold-current", |this, (), _, cx| {
        this.fold_current(false, cx);
        Ok(())
    });
}

/// The buffer's folds in lines, for one version of them.
pub(super) struct FoldCache {
    version: u64,
    pub(super) folds: Vec<Fold>,
    /// Lines of the text the folds are for.
    lines: usize,
}

impl EditorView {
    /// The buffer's folds in lines, recomputed when the buffer's folds change.
    pub(super) fn folds(&mut self, cx: &App) -> &[Fold] {
        let buffer = self.buffer.read(cx);
        let (ranges, version, _) = buffer.folds();
        if self
            .fold_cache
            .as_ref()
            .is_none_or(|cache| cache.version != version)
        {
            let text = buffer.doc().text();
            self.fold_cache = Some(FoldCache {
                version,
                folds: folds_from_ranges(text, ranges),
                lines: line_count(text),
            });
        }
        self.fold_cache
            .as_ref()
            .map_or(&[][..], |cache| cache.folds.as_slice())
    }

    /// Follows an edit by shifting the cached folds when that is exact, which saves converting
    /// every fold of a large file to lines on each keystroke.
    pub(super) fn follow_folds(&mut self, changes: &ChangeSet, cx: &App) {
        let buffer = self.buffer.read(cx);
        let (_, version, mapped_from) = buffer.folds();
        let text = buffer.doc().text();
        if let Some(cache) = &mut self.fold_cache
            && mapped_from == Some(cache.version)
            && shift_folds(&mut cache.folds, text, changes, cache.lines)
        {
            cache.version = version;
            cache.lines = line_count(text);
        }
    }

    pub(super) fn is_collapsed(&self, line: usize, cx: &App) -> bool {
        self.collapsed.contains(self.text(cx), line)
    }

    /// Hides the lines of the collapsed folds in the display map.
    pub(super) fn sync_hidden(&mut self, cx: &App) {
        let text = self.text(cx).clone();
        let collapsed = self.collapsed.lines(&text);
        let hidden = hidden_lines(self.folds(cx), &collapsed);
        self.display.set_hidden(hidden);
    }

    /// Forgets collapsed headers that are no longer fold points (after a reparse).
    pub(super) fn prune_collapsed(&mut self, cx: &App) {
        let text = self.text(cx).clone();
        let collapsed = self.collapsed.lines(&text);
        let folds = self.folds(cx).to_vec();
        for line in collapsed {
            if fold_at(&folds, line).is_none() {
                self.collapsed.remove(&text, line);
            }
        }
        self.sync_hidden(cx);
    }

    /// Collapses (or expands) the folds with headers `lines`, then moves carets out of the
    /// hidden lines to the end of their fold's header.
    fn set_collapsed(&mut self, lines: Vec<usize>, collapse: bool, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        for line in lines {
            if collapse {
                self.collapsed.add(&text, line);
            } else {
                self.collapsed.remove(&text, line);
            }
        }
        self.sync_hidden(cx);
        let display = &self.display;
        self.selection = self.selection.transform(|range| {
            let line = line_of(&text, range.head);
            if display.is_hidden(line) {
                Range::point(line_range(&text, display.visible_line(line)).end)
            } else {
                range
            }
        });
        self.request_autoscroll(cx);
    }

    /// Fold All (Alt+0) and Unfold All (Alt+Shift+0).
    fn fold_all(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let headers = self.folds(cx).iter().map(|fold| fold.header).collect();
        self.set_collapsed(headers, collapse, cx);
    }

    /// Collapse or uncollapse level N (Alt+N, Alt+Shift+N): the folds nested N deep.
    fn fold_level(&mut self, level: usize, collapse: bool, cx: &mut Context<Self>) {
        let headers = self
            .folds(cx)
            .iter()
            .filter(|fold| fold.level == level)
            .map(|fold| fold.header)
            .collect();
        self.set_collapsed(headers, collapse, cx);
    }

    /// Collapse the innermost fold around the caret (Ctrl+Alt+F), or expand the fold at the
    /// caret's line (Ctrl+Alt+Shift+F).
    fn fold_current(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let line = line_of(self.text(cx), self.selection.primary().head);
        let folds = self.folds(cx);
        let target = if collapse {
            innermost_containing(folds, line)
        } else {
            fold_at(folds, line).or_else(|| innermost_containing(folds, line))
        };
        if let Some(header) = target.map(|fold| fold.header) {
            self.set_collapsed(vec![header], collapse, cx);
        }
    }

    /// A click on a fold point in the folding margin.
    pub(super) fn toggle_fold(&mut self, line: usize, cx: &mut Context<Self>) {
        if fold_at(self.folds(cx), line).is_none() {
            return;
        }
        let collapse = !self.is_collapsed(line, cx);
        self.set_collapsed(vec![line], collapse, cx);
    }

    /// Expands the folds that hide a caret: edits, search results and Go To land in view.
    pub(super) fn reveal_carets(&mut self, cx: &App) {
        let text = self.text(cx).clone();
        for range in self.selection.ranges().to_vec() {
            let line = line_of(&text, range.head);
            while self.display.is_hidden(line) {
                // The outermost collapsed fold hiding the line is its visible header.
                let header = self.display.visible_line(line);
                if !self.collapsed.remove(&text, header) {
                    break;
                }
                self.sync_hidden(cx);
            }
        }
    }

    /// Where a horizontal caret movement that landed in hidden lines goes instead: past the
    /// fold when moving forward, to the end of its header when moving back.
    pub(super) fn skip_hidden(
        &self,
        text: &birchpad_core::Rope,
        head: usize,
        forward: bool,
    ) -> usize {
        let line = line_of(text, head);
        if !self.display.is_hidden(line) {
            return head;
        }
        let header = self.display.visible_line(line);
        if forward {
            let after = (header + 1..birchpad_core::motion::line_count(text))
                .find(|&line| !self.display.is_hidden(line));
            match after {
                Some(line) => line_range(text, line).start,
                None => line_range(text, header).end,
            }
        } else {
            line_range(text, header).end
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::single_range_in_vec_init,
    reason = "hidden lines are lists of line ranges, some with one range"
)]
mod tests {
    use birchpad_commands::Invocation;
    use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext, point};
    use serde_json::json;

    use super::*;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{document_start, open_workspace};

    /// Folds: 0..4 (level 1), 1..3 (level 2), 5..7 (level 1).
    const SAMPLE: &str = "fn a() {\n    if x {\n        y();\n    }\n}\nfn b() {\n    z();\n}\n";

    fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
        workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
    }

    fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx).unwrap();
        });
        cx.run_until_parked();
    }

    /// A Rust document with SAMPLE, parsed.
    fn open_sample(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input(SAMPLE);
        let rust = Invocation::with_args("language.set", json!({ "language": "rust" }));
        run(&workspace, rust, cx);
        cx.simulate_keystrokes(document_start());
        (workspace, cx)
    }

    fn hidden(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Vec<std::ops::Range<usize>> {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| view.display.hidden().to_vec())
    }

    fn caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
        let view = view(workspace, cx);
        view.read_with(cx, |view, cx| {
            let text = view.text(cx);
            let head = view.selection.primary().head;
            let line = line_of(text, head);
            (line, head - line_range(text, line).start)
        })
    }

    #[gpui_kit::test]
    fn fold_commands_hide_and_show_lines(cx: &mut TestAppContext) {
        let (workspace, cx) = open_sample(cx);
        cx.simulate_keystrokes("down down");
        run(&workspace, Invocation::new("view.fold-all"), cx);
        assert_eq!(hidden(&workspace, cx), [1..5, 6..8]);
        assert_eq!(
            caret(&workspace, cx),
            (0, 8),
            "the caret leaves the hidden lines"
        );

        run(&workspace, Invocation::new("view.unfold-all"), cx);
        assert!(hidden(&workspace, cx).is_empty());
        run(
            &workspace,
            Invocation::with_args("view.fold-level", json!({ "level": 2 })),
            cx,
        );
        assert_eq!(hidden(&workspace, cx), [2..4]);

        // The keys of Notepad++.
        cx.simulate_keystrokes("alt-shift-2");
        assert!(hidden(&workspace, cx).is_empty());
        cx.simulate_keystrokes("alt-0");
        assert_eq!(hidden(&workspace, cx), [1..5, 6..8]);
        cx.simulate_keystrokes("alt-shift-0");
        assert!(hidden(&workspace, cx).is_empty());

        // Collapse the innermost fold around the caret, then expand it from its header.
        cx.simulate_keystrokes("down down ctrl-alt-f");
        assert_eq!(hidden(&workspace, cx), [2..4]);
        assert_eq!(caret(&workspace, cx).0, 1);
        cx.simulate_keystrokes("ctrl-alt-shift-f");
        assert!(hidden(&workspace, cx).is_empty());
    }

    #[gpui_kit::test]
    fn carets_step_over_collapsed_folds_and_reveal_them(cx: &mut TestAppContext) {
        let (workspace, cx) = open_sample(cx);
        cx.simulate_keystrokes("ctrl-alt-f end");
        assert_eq!(hidden(&workspace, cx), [1..5]);
        // Right at the end of the header goes past the fold, Left comes back.
        cx.simulate_keystrokes("right");
        assert_eq!(caret(&workspace, cx), (5, 0));
        cx.simulate_keystrokes("left");
        assert_eq!(caret(&workspace, cx), (0, 8));
        // Down goes to the next visible line.
        cx.simulate_keystrokes("down");
        assert_eq!(caret(&workspace, cx).0, 5);
        assert_eq!(hidden(&workspace, cx), [1..5], "still folded");

        // Going to a hidden line (Go To, a search result) expands the fold.
        let view = view(&workspace, cx);
        view.update(cx, |view, cx| {
            let pos = line_range(view.text(cx), 2).start;
            view.go_to(pos, cx);
        });
        cx.run_until_parked();
        assert!(hidden(&workspace, cx).is_empty());
        assert_eq!(caret(&workspace, cx), (2, 0));
    }

    #[gpui_kit::test]
    fn the_folding_margin_toggles_and_folds_follow_edits(cx: &mut TestAppContext) {
        let (workspace, cx) = open_sample(cx);
        let view = view(&workspace, cx);
        // The fold box of `fn b` (row 5).
        let click = view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("a frame was drawn");
            let margin = layout.margins.folding.expect("folding margin");
            let row = layout.rows.iter().find(|row| row.row.line == 5).unwrap();
            point(
                margin.left() + margin.size.width / 2.,
                row.y + layout.metrics.line_height / 2.,
            )
        });
        cx.simulate_click(click, Modifiers::none());
        assert_eq!(hidden(&workspace, cx), [6..8]);

        // A line typed above moves the collapsed fold down with its text, and it stays
        // collapsed after the reparse.
        cx.simulate_keystrokes(document_start());
        cx.simulate_input("// top\n");
        cx.run_until_parked();
        assert_eq!(hidden(&workspace, cx), [7..9]);

        cx.simulate_click(click, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            hidden(&workspace, cx),
            [7..9],
            "row 5 is no longer a fold point"
        );
    }

    #[gpui_kit::test]
    fn shifted_folds_equal_recomputed_ones(cx: &mut TestAppContext) {
        let (workspace, cx) = open_sample(cx);
        // Inside `if x { ... }`: every fold is before, around or after the edited line.
        cx.simulate_keystrokes("down down end");
        cx.simulate_input(
            "
        w();
        v();",
        );
        let view = view(&workspace, cx);
        view.update(cx, |view, cx| {
            let shifted = view.fold_cache.as_ref().unwrap();
            let buffer = view.buffer.read(cx);
            let (ranges, version, _) = buffer.folds();
            assert_eq!(shifted.version, version, "shifted, not recomputed");
            let recomputed = folds_from_ranges(buffer.doc().text(), ranges);
            assert_eq!(shifted.folds, recomputed);
        });
    }

    #[gpui_kit::test]
    fn rectangles_and_multi_selections_stay_out_of_collapsed_folds(cx: &mut TestAppContext) {
        let (workspace, cx) = open_sample(cx);
        // Collapse `fn a`, then extend a rectangle down from its header: it skips the
        // hidden lines and the fold stays collapsed.
        cx.simulate_keystrokes("ctrl-alt-f alt-shift-down");
        assert_eq!(hidden(&workspace, cx), [1..5]);
        let view = view(&workspace, cx);
        let lines: Vec<usize> = view.read_with(cx, |view, cx| {
            let text = view.text(cx);
            view.selection
                .iter()
                .map(|range| line_of(text, range.head))
                .collect()
        });
        assert_eq!(lines, [0, 5]);
        cx.simulate_input("  ");
        cx.run_until_parked();
        let text = view.read_with(cx, |view, cx| view.text(cx).to_string());
        assert!(text.starts_with("  fn a() {\n    if x {"), "{text}");
        assert!(text.contains("\n  fn b()"), "{text}");
        assert_eq!(hidden(&workspace, cx), [1..5], "hidden lines untouched");

        // Multi-select All finds matches inside the fold: the fold opens. (The indentation
        // of `z();`, which the hidden lines have too.)
        cx.simulate_keystrokes("escape down home shift-home");
        run(&workspace, Invocation::new("edit.multi-select-all"), cx);
        cx.run_until_parked();
        let selected = view.read_with(cx, |view, _| view.selection.ranges().len());
        assert!(selected > 1);
        assert!(hidden(&workspace, cx).is_empty());
    }
}
