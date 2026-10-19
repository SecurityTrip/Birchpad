//! Plugins > Compare (ADR 0039), as Notepad++'s Compare plugin: two documents side by side, the
//! lines one adds, removes or changes marked across the line, and the next and the previous
//! difference.
//!
//! The marks belong to the documents (`DocumentMarks::compare`), so they follow edits until
//! Clear Results; the old document shows the lines it loses, the new one the lines it gains,
//! both the lines that changed.

use std::ops::Range;

use anyhow::{Result, anyhow, bail};
use birchpad_commands::Invocation;
use birchpad_core::Rope;
use birchpad_core::motion::line_range_with_break;
use birchpad_tools::compare::{self, Change, Options};
use gpui_kit::{App, Context, Entity, Global, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

/// Compare's options, for the next comparison.
#[derive(Default)]
struct CompareOptions(Options);

impl Global for CompareOptions {}

pub(crate) fn options(cx: &App) -> Options {
    cx.try_global::<CompareOptions>()
        .map(|options| options.0)
        .unwrap_or_default()
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("compare.compare", |this, (), window, cx| {
        this.compare(window, cx)
    });
    registry.workspace("compare.clear", |this, (), _, cx| {
        for view in this.all_views(cx) {
            let buffer = view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| {
                buffer.update_marks(cx, |marks, _| marks.compare.clear());
            });
        }
        Ok(())
    });
    for (id, forward) in [("compare.next", true), ("compare.previous", false)] {
        registry.editor(id, move |this, (), _, cx| {
            let pos = this.selection.primary().head;
            let target = {
                let marks = this.buffer.read(cx).marks();
                if forward {
                    marks.compare.next_after(pos)
                } else {
                    marks.compare.previous_before(pos)
                }
            };
            let target =
                target.ok_or_else(|| anyhow!("{}", t!("compare.there-are-no-differences-here")))?;
            this.go_to(target.start, cx);
            Ok(())
        });
    }
    registry.workspace("compare.ignore-spaces", |this, (), _, cx| {
        let mut current = options(cx);
        current.ignore_whitespace = !current.ignore_whitespace;
        cx.set_global(CompareOptions(current));
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("compare.ignore-case", |this, (), _, cx| {
        let mut current = options(cx);
        current.ignore_case = !current.ignore_case;
        cx.set_global(CompareOptions(current));
        this.refresh_menus(cx);
        Ok(())
    });
}

/// Whether a Compare option shows a check mark.
pub(crate) fn is_checked(invocation: &Invocation, options: Options) -> Option<bool> {
    match invocation.command.as_str() {
        "compare.ignore-spaces" => Some(options.ignore_whitespace),
        "compare.ignore-case" => Some(options.ignore_case),
        _ => None,
    }
}

/// The byte range of `lines`, with their line breaks.
fn byte_range(text: &Rope, lines: &Range<usize>) -> Range<usize> {
    if lines.is_empty() {
        return 0..0;
    }
    line_range_with_break(text, lines.start).start..line_range_with_break(text, lines.end - 1).end
}

/// The marks of each side: the old text keeps what it loses and what changes, the new text
/// what it gains and what changes.
pub(crate) fn marks(
    old: &Rope,
    new: &Rope,
    differences: &[compare::Difference],
) -> [Vec<(Range<usize>, Change)>; 2] {
    let mut sides = [Vec::new(), Vec::new()];
    for difference in differences {
        if difference.change != Change::Added {
            sides[0].push((byte_range(old, &difference.old), difference.change));
        }
        if difference.change != Change::Removed {
            sides[1].push((byte_range(new, &difference.new), difference.change));
        }
    }
    for side in &mut sides {
        side.retain(|(range, _)| !range.is_empty());
    }
    sides
}

impl Workspace {
    /// The document to compare the active one with: the other view's, or the tab before the
    /// active one (after it, for the first tab), which goes to the other view.
    fn compare_partner(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(Entity<EditorView>, Entity<EditorView>)> {
        let new = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("compare.there-is-no-document-to-compare")))?;
        let other = 1 - self.active_pane_index();
        if let Some(old) = self.panes()[other].read(cx).active_item().cloned() {
            return Ok((old, new));
        }
        let pane = self.active_pane().read(cx);
        let index = pane.index_of(&new).unwrap_or(0);
        let partner = if index > 0 { index - 1 } else { index + 1 };
        let Some(old) = pane.items().get(partner).cloned() else {
            bail!(
                "{}",
                t!("compare.open-a-second-document-to-compare-this-one-with")
            );
        };
        // Side by side: the new document moves to the other view.
        self.dispatch(&Invocation::new("view.move-to-other-view"), window, cx)?;
        let new = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("compare.there-is-no-document-to-compare")))?;
        Ok((old, new))
    }

    /// Compares the active document with its partner and marks the differences in both.
    fn compare(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let (old, new) = self.compare_partner(window, cx)?;
        let old_buffer = old.read(cx).buffer.clone();
        let new_buffer = new.read(cx).buffer.clone();
        if old_buffer == new_buffer {
            bail!(
                "{}",
                t!("compare.a-document-is-the-same-as-itself-open-another-on")
            );
        }
        let old_text = old_buffer.read(cx).doc().text().clone();
        let new_text = new_buffer.read(cx).doc().text().clone();
        let differences =
            compare::compare(&old_text.to_string(), &new_text.to_string(), options(cx));
        let [old_marks, new_marks] = marks(&old_text, &new_text, &differences);
        for (buffer, side) in [(old_buffer, old_marks), (new_buffer, new_marks)] {
            buffer.update(cx, |buffer, cx| {
                buffer.update_marks(cx, |marks, _| {
                    marks.compare.clear();
                    for (range, change) in side {
                        marks.compare.insert(range, change);
                    }
                });
            });
        }
        if differences.is_empty() {
            crate::workspace::report_success(t!("compare.the-documents-are-the-same"), window, cx);
        } else {
            new.update(cx, |view, cx| {
                if let Some(first) = view.buffer.read(cx).marks().compare.next_from(0) {
                    view.go_to(first.0.start, cx);
                }
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/compare/tests.rs"]
mod tests;
