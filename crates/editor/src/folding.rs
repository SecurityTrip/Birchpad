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
use birchpad_ui::CommandRegistry;

use crate::EditorCommands as _;

#[derive(Deserialize)]
struct LevelArgs {
    level: usize,
}

impl LevelArgs {
    /// The level, one of Notepad++'s eight (Alt+1 to Alt+8).
    fn level(&self) -> anyhow::Result<usize> {
        anyhow::ensure!(
            (1..=8).contains(&self.level),
            "fold levels go from 1 to 8, not {}",
            self.level
        );
        Ok(self.level)
    }
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("view.fold-all", |this, (), _, cx| {
        this.fold_all(true, cx);
        Ok(())
    });
    registry.editor("view.unfold-all", |this, (), _, cx| {
        this.fold_all(false, cx);
        Ok(())
    });
    registry.editor("view.fold-level", |this, args: LevelArgs, _, cx| {
        this.fold_level(args.level()?, true, cx);
        Ok(())
    });
    registry.editor("view.unfold-level", |this, args: LevelArgs, _, cx| {
        this.fold_level(args.level()?, false, cx);
        Ok(())
    });
    registry.editor("view.hide-lines", |this, (), _, cx| this.hide_lines(cx));
    registry.editor("view.show-hidden-lines", |this, (), _, cx| {
        let text = this.text(cx).clone();
        this.user_hidden.set_lines(&text, Vec::new());
        this.sync_hidden(cx);
        cx.notify();
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
pub(crate) struct FoldCache {
    version: u64,
    pub(crate) folds: Vec<Fold>,
    /// Lines of the text the folds are for.
    lines: usize,
}

impl EditorView {
    /// The buffer's folds in lines, recomputed when the buffer's folds change.
    pub(crate) fn folds(&mut self, cx: &App) -> &[Fold] {
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
    pub(crate) fn follow_folds(&mut self, changes: &ChangeSet, cx: &App) {
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

    /// Headers of the collapsed folds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn collapsed_lines(&self, cx: &App) -> Vec<usize> {
        self.collapsed.lines(self.text(cx))
    }

    pub(crate) fn is_collapsed(&self, line: usize, cx: &App) -> bool {
        self.collapsed.contains(self.text(cx), line)
    }

    /// Hides the lines of the collapsed folds and of View > Hide Lines in the display map.
    pub(crate) fn sync_hidden(&mut self, cx: &App) {
        let text = self.text(cx).clone();
        let collapsed = self.collapsed.lines(&text);
        let mut hidden = hidden_lines(self.folds(cx), &collapsed);
        hidden.extend(self.hidden_blocks(&text));
        hidden.sort_by_key(|range| range.start);
        let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(hidden.len());
        for range in hidden {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        self.display.set_hidden(merged);
    }

    /// The blocks of lines View > Hide Lines hid, in order.
    pub(crate) fn hidden_blocks(&self, text: &birchpad_core::Rope) -> Vec<std::ops::Range<usize>> {
        let mut blocks: Vec<std::ops::Range<usize>> = Vec::new();
        for line in self.user_hidden.lines(text) {
            match blocks.last_mut() {
                Some(last) if last.end == line => last.end = line + 1,
                _ => blocks.push(line..line + 1),
            }
        }
        blocks
    }

    /// The lines just above hidden blocks, in `lines`: where they are shown again.
    pub(crate) fn hidden_anchors(
        &self,
        text: &birchpad_core::Rope,
        lines: std::ops::Range<usize>,
    ) -> Vec<usize> {
        self.hidden_blocks(text)
            .into_iter()
            .map(|block| block.start - 1)
            .filter(|anchor| lines.contains(anchor))
            .collect()
    }

    /// View > Hide Lines (Alt+H): the lines of the selection. The first line stays, as the
    /// line the hidden ones are shown again from.
    fn hide_lines(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let text = self.text(cx).clone();
        let lines: Vec<usize> = birchpad_core::ops::line_blocks(&text, &self.selection)
            .into_iter()
            .flatten()
            .filter(|line| *line > 0)
            .collect();
        if lines.is_empty() {
            anyhow::bail!("{}", t!("folding.the-first-line-cannot-be-hidden"));
        }
        for line in lines {
            self.user_hidden.add(&text, line);
        }
        self.set_collapsed(Vec::new(), true, cx);
        cx.notify();
        Ok(())
    }

    /// Shows again the lines hidden below `anchor` (a click on its mark).
    pub(crate) fn show_hidden_below(&mut self, anchor: usize, cx: &mut Context<Self>) -> bool {
        let text = self.text(cx).clone();
        let Some(block) = self
            .hidden_blocks(&text)
            .into_iter()
            .find(|block| block.start == anchor + 1)
        else {
            return false;
        };
        for line in block {
            self.user_hidden.remove(&text, line);
        }
        self.sync_hidden(cx);
        cx.notify();
        true
    }

    /// Forgets collapsed headers that are no longer fold points (after a reparse). Before the
    /// first parse there are no folds yet: nothing is forgotten then.
    pub(crate) fn prune_collapsed(&mut self, cx: &App) {
        if self.buffer.read(cx).folds_pending() {
            return;
        }
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
    pub(crate) fn toggle_fold(&mut self, line: usize, cx: &mut Context<Self>) {
        if fold_at(self.folds(cx), line).is_none() {
            return;
        }
        let collapse = !self.is_collapsed(line, cx);
        self.set_collapsed(vec![line], collapse, cx);
    }

    /// Expands the folds that hide a caret: edits, search results and Go To land in view.
    pub(crate) fn reveal_carets(&mut self, cx: &App) {
        let text = self.text(cx).clone();
        for range in self.selection.ranges().to_vec() {
            let line = line_of(&text, range.head);
            // Hidden by View > Hide Lines: its block shows again.
            if let Some(block) = self
                .hidden_blocks(&text)
                .into_iter()
                .find(|block| block.contains(&line))
            {
                for hidden in block {
                    self.user_hidden.remove(&text, hidden);
                }
                self.sync_hidden(cx);
            }
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
    pub(crate) fn skip_hidden(
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
#[path = "../tests/unit/folding/tests.rs"]
mod tests;
