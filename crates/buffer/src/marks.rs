//! What a document carries besides its text: bookmarks, the styles of tokens and Mark's
//! matches, and the lines changed since it was opened.

use std::ops::Range as ByteRange;

use birchpad_core::{ChangeHistory, LineChange, LineMarkers, RangeSet, Rope, Transaction};
use gpui_kit::{AppContext as _, Context};

use birchpad_ui::AppState;

use super::{Buffer, BufferEvent, ChangeTracking, Comparing};

/// Number of styles of Search > Style All Occurrences of Token, as in Notepad++.
pub const MARK_STYLES: usize = 5;

/// Decorations that belong to the document, so every view of it shows them: bookmarks, the
/// token styles and the matches of Search > Mark. They follow every edit, undo and redo.
#[derive(Debug, Clone, Default)]
pub struct DocumentMarks {
    pub bookmarks: LineMarkers,
    pub styles: [RangeSet<()>; MARK_STYLES],
    /// Marked by the Mark tab of the find panel ("Find Mark Style" in Notepad++).
    pub found: RangeSet<()>,
    /// The matches of incremental search with Highlight all.
    pub incremental: RangeSet<()>,
    /// The lines Plugins > Compare found different, whole with their line breaks.
    pub compare: RangeSet<birchpad_tools::compare::Change>,
    /// What differs within the lines Compare found changed.
    pub compare_inline: RangeSet<()>,
}

impl DocumentMarks {
    /// Bookmarks the line where each of `ranges` starts: the "Bookmark line" option of Mark
    /// All.
    pub fn bookmark_lines_of(&mut self, text: &Rope, ranges: &[ByteRange<usize>]) {
        let mut lines = self.bookmarks.lines(text);
        lines.extend(
            ranges
                .iter()
                .map(|range| birchpad_core::motion::line_of(text, range.start)),
        );
        self.bookmarks.set_lines(text, lines);
    }

    pub(super) fn map(&mut self, transaction: &Transaction, text: &Rope) {
        let changes = transaction.changes();
        self.bookmarks.map(changes, text);
        for style in &mut self.styles {
            style.map(changes);
        }
        self.found.map(changes);
        self.incremental.map(changes);
        self.compare.map(changes);
        self.compare_inline.map(changes);
    }
}

impl Buffer {
    /// Bookmarks `lines` now, or once the file has been read.
    pub fn restore_bookmarks(&mut self, lines: Vec<usize>, cx: &mut Context<Self>) {
        self.pending_bookmarks = Some(lines);
        if self.loading.is_none() {
            self.set_pending_bookmarks();
            cx.emit(BufferEvent::MarksChanged);
        }
    }

    pub(super) fn set_pending_bookmarks(&mut self) {
        if let Some(lines) = self.pending_bookmarks.take() {
            self.marks.bookmarks.set_lines(self.doc.text(), lines);
        }
    }

    /// Bookmarked lines, including those waiting for the file to be read.
    pub fn bookmark_lines(&self) -> Vec<usize> {
        match &self.pending_bookmarks {
            Some(lines) => lines.clone(),
            None => self.marks.bookmarks.lines(self.doc.text()),
        }
    }
}

impl Buffer {
    pub fn marks(&self) -> &DocumentMarks {
        &self.marks
    }

    /// The lines changed since the document was opened: those that differ from the saved text,
    /// and those changed and saved.
    pub fn changed_lines(&self) -> (&LineMarkers, &LineMarkers) {
        (&self.changes.modified, &self.changes.saved)
    }

    /// Search > Change History > Clear All Changes: the text as it is now is where changes
    /// count from.
    pub fn clear_change_history(&mut self, cx: &mut Context<Self>) {
        self.changes = ChangeTracking::new(self.doc.text().clone());
        cx.emit(BufferEvent::MarksChanged);
        cx.notify();
    }

    /// Compares the text with the change history in the background. A comparison already
    /// running compares again when it ends.
    pub(super) fn compare_changes(&mut self, cx: &mut Context<Self>) {
        if let Some(comparing) = &mut self.changes.comparing {
            comparing.again = true;
            return;
        }
        if AppState::global(cx).settings.editor.change_history
            == birchpad_config::ChangeHistory::Off
        {
            return;
        }
        let text = self.doc.text().clone();
        let mut history = self.changes.history.clone();
        let generation = self.changes.generation;
        let task = cx.spawn(async move |this, cx| {
            let (history, text, lines) = cx
                .background_spawn(async move {
                    let lines = history.lines(&text);
                    (history, text, lines)
                })
                .await;
            this.update(cx, |buffer, cx| {
                buffer.finish_comparing(history, generation, &text, lines, cx);
            })
            .ok();
        });
        self.changes.comparing = Some(Comparing {
            since: None,
            again: false,
            task,
        });
    }

    /// Takes the changed lines of `text` from a finished comparison, moved through the edits
    /// made since.
    pub(super) fn finish_comparing(
        &mut self,
        history: ChangeHistory,
        generation: u64,
        text: &Rope,
        lines: Vec<(ByteRange<usize>, LineChange)>,
        cx: &mut Context<Self>,
    ) {
        let Some(comparing) = self.changes.comparing.take() else {
            return;
        };
        // This runs inside the comparison's task: let it finish instead of cancelling it.
        comparing.task.detach();
        if generation == self.changes.generation {
            // It also compared the saved text with the original: keep that.
            self.changes.history = history;
            let lines_of = |kind: LineChange| {
                lines
                    .iter()
                    .filter(move |(_, change)| *change == kind)
                    .flat_map(|(range, _)| range.clone())
            };
            self.changes
                .modified
                .set_lines(text, lines_of(LineChange::Modified));
            self.changes
                .saved
                .set_lines(text, lines_of(LineChange::Saved));
            if let Some(since) = &comparing.since {
                self.changes.modified.map(since, self.doc.text());
                self.changes.saved.map(since, self.doc.text());
            }
            cx.emit(BufferEvent::MarksChanged);
            cx.notify();
        }
        if comparing.again || generation != self.changes.generation {
            self.compare_changes(cx);
        }
    }

    /// Changes the document's decorations (bookmarks, token styles) and redraws its views.
    pub fn update_marks(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut DocumentMarks, &Rope),
    ) {
        change(&mut self.marks, self.doc.text());
        cx.emit(BufferEvent::MarksChanged);
        cx.notify();
    }
}
