//! Replace, Replace All, and Replace All in All Opened Documents.

use birchpad_core::search::{Direction, Searcher};
use birchpad_core::{Edit, Transaction};
use gpui_kit::{App, Context, Entity, Window};

use crate::app_state::AppState;
use crate::editor::EditorView;
use crate::workspace::Workspace;

use super::plural;

impl Workspace {
    /// Replace: replaces the selection if it is a match, then finds the next one.
    pub(super) fn replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        self.record_search(
            "search.replace-text",
            Direction::Forward,
            Some(&template),
            false,
            cx,
        );
        // The Find Next that follows is part of it.
        let recording = cx
            .try_global::<crate::macros::Macros>()
            .is_some_and(crate::macros::Macros::is_recording);
        if recording {
            crate::macros::pause(true, cx);
        }
        let (text, selected) = {
            let view = view.read(cx);
            let range = view.selection.primary();
            (view.text(cx).clone(), range.from()..range.to())
        };
        if searcher.is_match(&text, selected.clone())
            && (!selected.is_empty() || searcher.is_regex())
        {
            let replacement = searcher.replacement(&text, selected.clone(), &template);
            let caret = selected.start + replacement.len();
            let transaction =
                Transaction::from_edits(&text, [Edit::replace(selected.clone(), replacement)])
                    .expect("a match lies on character boundaries");
            let stays = AppState::global(cx).settings.searching.replace_stays;
            let replaced = selected.start..caret;
            let applied = view.update(cx, |view, cx| {
                let applied = view.apply_command_edit(transaction, cx);
                if applied {
                    if stays {
                        view.select_range(replaced, cx);
                    } else {
                        view.go_to(caret, cx);
                    }
                }
                applied
            });
            // Replace: Don't move to the following occurrence.
            if applied && stays {
                if recording {
                    crate::macros::pause(false, cx);
                }
                return;
            }
            if !applied {
                if recording {
                    crate::macros::pause(false, cx);
                }
                self.report(Some(t!("find.the-document-is-read-only")), true, cx);
                return;
            }
        }
        self.find(Direction::Forward, window, cx);
        if recording {
            crate::macros::pause(false, cx);
        }
    }
}

impl Workspace {
    /// The edits of Replace All in `view` (in its scope), and how many there are.
    pub fn replace_all_edits(
        &self,
        searcher: &Searcher,
        template: &str,
        view: &Entity<EditorView>,
        range: std::ops::Range<usize>,
        cx: &App,
    ) -> Option<(Transaction, usize)> {
        let text = view.read(cx).text(cx);
        let replacements = searcher.replacements(text, range, template);
        let count = replacements.len();
        if count == 0 {
            return None;
        }
        let edits = replacements
            .into_iter()
            .map(|(range, replacement)| Edit::replace(range, replacement));
        let transaction = Transaction::from_edits(text, edits).expect("matches do not overlap");
        Some((transaction, count))
    }

    /// Replace All: every match in the document or the selection, as one undo step.
    pub fn replace_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        self.record_search(
            "search.replace-text",
            Direction::Forward,
            Some(&template),
            true,
            cx,
        );
        let scope = self.scope(&view, cx);
        let edits = self.replace_all_edits(&searcher, &template, &view, scope.range.clone(), cx);
        if self.report_failure(&searcher, cx) {
            return;
        }
        let count = edits.as_ref().map_or(0, |(_, count)| *count);
        if let Some((transaction, _)) = edits
            && !view.update(cx, |view, cx| view.apply_command_edit(transaction, cx))
        {
            self.report(Some(t!("find.the-document-is-read-only")), true, cx);
            return;
        }
        self.report(
            Some(format!(
                "{}: {} {}",
                t!("find.replace-all"),
                plural(count, "occurrence replaced", "occurrences replaced"),
                scope.describe()
            )),
            count == 0,
            cx,
        );
    }

    /// Replace All in All Opened Documents: each document as one undo step; read-only ones are
    /// left as they are.
    pub(super) fn replace_all_in_all(&mut self, cx: &mut Context<Self>) {
        let Some((searcher, _)) = self.searcher(cx) else {
            return;
        };
        let template = self.find_bar.read(cx).replacement(cx);
        // One view per document: views of one buffer share its text.
        let mut seen = Vec::new();
        let views: Vec<Entity<EditorView>> = self
            .all_views(cx)
            .into_iter()
            .filter(|view| {
                let buffer = view.read(cx).buffer.entity_id();
                let new = !seen.contains(&buffer);
                seen.push(buffer);
                new
            })
            .collect();
        let (mut total, mut documents, mut read_only) = (0, 0, 0);
        for view in views {
            let range = 0..view.read(cx).text(cx).len();
            let Some((transaction, count)) =
                self.replace_all_edits(&searcher, &template, &view, range, cx)
            else {
                continue;
            };
            if view.update(cx, |view, cx| view.apply_command_edit(transaction, cx)) {
                total += count;
                documents += 1;
            } else {
                read_only += 1;
            }
        }
        if self.report_failure(&searcher, cx) {
            return;
        }
        let mut message = format!(
            "{}: {}",
            t!("find.replace-all-in-all-opened-documents"),
            t!(
                "find.replaced-in",
                replaced = plural(total, "occurrence replaced", "occurrences replaced"),
                documents = plural(documents, "document", "documents")
            )
        );
        if read_only > 0 {
            message.push_str(&format!(
                "; {}",
                plural(
                    read_only,
                    "document is read-only",
                    "documents are read-only"
                )
            ));
        }
        self.report(Some(message), total == 0, cx);
    }
}
