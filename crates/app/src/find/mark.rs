//! Mark All, Clear all marks and Copy Marked Text.

use birchpad_core::ops;
use gpui_kit::{ClipboardItem, Context};

use crate::workspace::Workspace;

use super::plural;

impl Workspace {
    /// Mark All: marks the matches in the document or the selection, and with Bookmark line
    /// bookmarks their lines. With Purge for each search, earlier marks go first.
    pub(super) fn mark_all(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let options = self.find_bar.read(cx).options;
        let scope = self.scope(&view, cx);
        let text = view.read(cx).text(cx).clone();
        // An empty match has nothing to mark.
        let matches: Vec<_> = searcher
            .find_all_in(&text, scope.range.clone())
            .into_iter()
            .filter(|found| !found.is_empty())
            .collect();
        if self.report_failure(&searcher, cx) {
            return;
        }
        let count = matches.len();
        let buffer = view.read(cx).buffer.clone();
        buffer.update(cx, |buffer, cx| {
            buffer.update_marks(cx, |marks, text| {
                if options.purge {
                    marks.found.clear();
                }
                if options.bookmark_line {
                    marks.bookmark_lines_of(text, &matches);
                }
                marks.found.insert_all(matches, ());
            });
        });
        self.report(
            Some(format!(
                "{}: {} {}",
                t!("find.mark"),
                plural(count, "match", "matches"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    pub(super) fn clear_marks(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let buffer = view.read(cx).buffer.clone();
        buffer.update(cx, |buffer, cx| {
            buffer.update_marks(cx, |marks, _| marks.found.clear());
        });
        self.report(Some(t!("find.marks-cleared")), false, cx);
    }

    /// Copy Marked Text: the marked texts, one per line. Returns what it says of them.
    pub(super) fn copy_marked(&mut self, cx: &mut Context<Self>) -> String {
        let Some(view) = self.active_view(cx) else {
            return t!("file-ops.no-document");
        };
        let (copied, count) = {
            let buffer = view.read(cx).buffer.read(cx);
            let ranges: Vec<_> = buffer
                .marks()
                .found
                .iter()
                .map(|(range, _)| range)
                .collect();
            let doc = buffer.doc();
            (
                ops::copy_ranges(doc.text(), ranges.iter().cloned(), doc.line_ending()),
                ranges.len(),
            )
        };
        if count > 0 {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
        }
        let message = format!(
            "{} {}",
            t!("find.copied"),
            plural(count, "marked text", "marked texts")
        );
        self.report(Some(message.clone()), count == 0, cx);
        message
    }
}
