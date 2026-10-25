//! Plugins > Compare (ADR 0039), as Notepad++'s Compare and ComparePlus plugins: two documents
//! side by side, the lines one adds, removes or changes marked across the line and what differs
//! within a changed line marked over it, blank rows that keep the lines both have side by side,
//! the differences along the scrollbar, and the next and the previous difference. The active
//! document is compared with the other view's, or with its last save, the clipboard or Git's
//! HEAD, shown in the other view as a read-only document that Clear Results closes.
//!
//! The marks belong to the documents (`DocumentMarks::compare`), so they follow edits until
//! Clear Results; the old document shows the lines it loses, the new one the lines it gains,
//! both the lines that changed. A moment after either document changes, they are compared
//! again (`compare.recompare`).

use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_commands::Invocation;
use birchpad_core::motion::line_range_with_break;
use birchpad_core::{Document, Rope};
pub use birchpad_tools::compare::{self, Change, Options};
use gpui_kit::{App, Context, Entity, Global, WeakEntity, Window};

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

/// How long after an edit the documents are compared again.
pub const RECOMPARE_DELAY: Duration = Duration::from_millis(300);

/// Compare's options, for the next comparison.
#[derive(Default)]
struct CompareOptions(Options);

impl Global for CompareOptions {}

/// The comparison shown, to compare again after edits and to clear.
pub struct Comparison {
    pub old: WeakEntity<EditorView>,
    pub new: WeakEntity<EditorView>,
    /// The read-only document made for it (last save, clipboard, Git), closed with it.
    pub temporary: Option<WeakEntity<EditorView>>,
    /// Bumped by every edit, so that only the last one's timer compares again.
    edits: u64,
}

impl Global for Comparison {}

pub fn options(cx: &App) -> Options {
    cx.try_global::<CompareOptions>()
        .map(|options| options.0)
        .unwrap_or_default()
}

/// What to compare the active document with, besides the other view's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The file on disk, as last saved.
    LastSave,
    Clipboard,
    /// The file as Git's HEAD has it.
    Git,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("compare.compare", |this, (), window, cx| {
        this.compare(window, cx)
    });
    registry.workspace("compare.clear", |this, (), window, cx| {
        this.clear_comparison(window, cx);
        Ok(())
    });
    for (id, source) in [
        ("compare.with-saved", Source::LastSave),
        ("compare.with-clipboard", Source::Clipboard),
        ("compare.with-git", Source::Git),
    ] {
        registry.workspace(id, move |this, (), window, cx| {
            this.compare_with(source, window, cx)
        });
    }
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
pub fn is_checked(invocation: &Invocation, options: Options) -> Option<bool> {
    match invocation.command.as_str() {
        "compare.ignore-spaces" => Some(options.ignore_whitespace),
        "compare.ignore-case" => Some(options.ignore_case),
        _ => None,
    }
}

/// The byte range of `lines`, with their line breaks.
pub fn byte_range(text: &Rope, lines: &Range<usize>) -> Range<usize> {
    if lines.is_empty() {
        return 0..0;
    }
    line_range_with_break(text, lines.start).start..line_range_with_break(text, lines.end - 1).end
}

/// The marks of each side: the old text keeps what it loses and what changes, the new text
/// what it gains and what changes.
pub fn marks(
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

/// The text of the file `name` in the folder `dir` as Git's HEAD has it, as `git show` gives
/// it.
pub fn git_head(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("HEAD:./{name}"))
        .output()
        .map_err(|error| anyhow!("{}", t!("compare.git-cannot-run", reason = error)))?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("{}", t!("compare.git-has-no-such-file", reason = reason));
    }
    Ok(output.stdout)
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
        self.start_comparison(old, new, None, window, cx)
    }

    /// Compares `old` with `new`, keeps the comparison and goes to the first difference.
    fn start_comparison(
        &mut self,
        old: Entity<EditorView>,
        new: Entity<EditorView>,
        temporary: Option<WeakEntity<EditorView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if old.read(cx).buffer == new.read(cx).buffer {
            bail!(
                "{}",
                t!("compare.a-document-is-the-same-as-itself-open-another-on")
            );
        }
        // A comparison already shown makes way, its temporary document too.
        if let Some(previous) = cx.try_global::<Comparison>()
            && (previous.old != old.downgrade() || previous.new != new.downgrade())
        {
            self.clear_comparison(window, cx);
        }
        let same = show_comparison(&old, &new, cx);
        cx.set_global(Comparison {
            old: old.downgrade(),
            new: new.downgrade(),
            temporary,
            edits: 0,
        });
        if same {
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

    /// Clear Results: no marks, no blank rows, and the temporary document closed.
    pub fn clear_comparison(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for view in self.all_views(cx) {
            let buffer = view.read(cx).buffer.clone();
            buffer.update(cx, |buffer, cx| {
                buffer.update_marks(cx, |marks, _| {
                    marks.compare.clear();
                    marks.compare_inline.clear();
                });
            });
            if !view.read(cx).display.padding().is_empty() {
                view.update(cx, |view, cx| view.set_padding(Vec::new(), cx));
            }
        }
        let temporary = cx
            .try_global::<Comparison>()
            .and_then(|comparison| comparison.temporary.clone())
            .and_then(|view| view.upgrade());
        if cx.has_global::<Comparison>() {
            cx.remove_global::<Comparison>();
        }
        if let Some(view) = temporary {
            self.close_view(&view, window, cx);
        }
    }

    /// The active document compared with `source`, shown read-only in the other view.
    fn compare_with(
        &mut self,
        source: Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let new = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("compare.there-is-no-document-to-compare")))?;
        let buffer = new.read(cx).buffer.read(cx);
        let name = buffer.display_name();
        let format = buffer.doc().format();
        let path = buffer.path().map(Path::to_owned);
        let (title, text) = match source {
            Source::LastSave => {
                let path = path.ok_or_else(|| anyhow!("{}", t!("compare.save-it-first")))?;
                let options = birchpad_io::LoadOptions {
                    encoding: Some(format.encoding),
                    ..birchpad_io::LoadOptions::new(AppState::global(cx).ansi)
                };
                let file = birchpad_io::load(&path, options, &std::sync::atomic::AtomicU64::new(0))
                    .with_context(|| path.display().to_string())?;
                (t!("compare.saved-title", name = name), file.text)
            }
            Source::Clipboard => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| anyhow!("{}", t!("compare.the-clipboard-holds-no-text")))?;
                (t!("compare.clipboard-title"), Rope::from(text.as_str()))
            }
            Source::Git => {
                let path = path.ok_or_else(|| anyhow!("{}", t!("compare.save-it-first")))?;
                let (Some(dir), Some(file_name)) = (path.parent(), path.file_name()) else {
                    bail!("{}", t!("compare.save-it-first"));
                };
                let bytes = git_head(dir, &file_name.to_string_lossy())?;
                let text = birchpad_io::decode(bytes, format.encoding).text;
                (
                    t!("compare.git-title", name = name),
                    Rope::from(text.as_str()),
                )
            }
        };
        let temporary = self.open_comparison_document(title, text, format, &new, window, cx);
        self.start_comparison(
            temporary.clone(),
            new,
            Some(temporary.downgrade()),
            window,
            cx,
        )
    }

    /// Opens `text` as a read-only document titled `title` in the view other than `beside`'s,
    /// and makes `beside` active again.
    fn open_comparison_document(
        &mut self,
        title: String,
        text: Rope,
        format: birchpad_core::Format,
        beside: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let language = beside.read(cx).buffer.read(cx).language();
        self.open_document(Document::with_format(text, format), window, cx);
        let view = self
            .active_view(cx)
            .expect("the document just opened is active");
        view.read(cx)
            .buffer
            .clone()
            .update(cx, |buffer: &mut Buffer, cx| {
                buffer.set_title(Some(title), cx);
                buffer.set_language(language, cx);
                buffer.set_requested_read_only(true, cx);
            });
        let target = 1 - self.pane_of(beside, cx).unwrap_or(self.active_pane_index());
        if self.pane_of(&view, cx) != Some(target) {
            // Ignored when it fails: the document is still compared, in the same view.
            let _ = self.dispatch(&Invocation::new("view.move-to-other-view"), window, cx);
        }
        self.activate_view(beside, window, cx);
        view
    }

    /// After an edit of `buffer`: if it is compared, compares again once edits pause.
    pub fn compare_after_edit(
        &mut self,
        buffer: &Entity<Buffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !AppState::global(cx).settings.compare.recompare {
            return;
        }
        let Some(comparison) = cx.try_global::<Comparison>() else {
            return;
        };
        let compared = [&comparison.old, &comparison.new].into_iter().any(|view| {
            view.upgrade()
                .is_some_and(|view| &view.read(cx).buffer == buffer)
        });
        if !compared {
            return;
        }
        let edits = {
            let comparison = cx.global_mut::<Comparison>();
            comparison.edits += 1;
            comparison.edits
        };
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(RECOMPARE_DELAY).await;
            this.update_in(cx, |this, window, cx| {
                let Some(comparison) = cx.try_global::<Comparison>() else {
                    return;
                };
                if comparison.edits != edits {
                    return;
                }
                // A view no pane shows any more was closed.
                let shown = |view: &WeakEntity<EditorView>| {
                    view.upgrade()
                        .filter(|view| this.pane_of(view, cx).is_some())
                };
                match (shown(&comparison.old), shown(&comparison.new)) {
                    (Some(old), Some(new)) => {
                        show_comparison(&old, &new, cx);
                    }
                    // One of them was closed: nothing to compare with any more.
                    _ => this.clear_comparison(window, cx),
                }
            })
            .ok();
        })
        .detach();
    }
}

/// Compares `old` with `new` and shows it: the marks of both, what differs within changed lines
/// (`compare.inline`) and the blank rows (`compare.align`). Whether they are the same.
fn show_comparison(old: &Entity<EditorView>, new: &Entity<EditorView>, cx: &mut App) -> bool {
    let settings = AppState::global(cx).settings.compare.clone();
    let old_buffer = old.read(cx).buffer.clone();
    let new_buffer = new.read(cx).buffer.clone();
    let old_text = old_buffer.read(cx).doc().text().clone();
    let new_text = new_buffer.read(cx).doc().text().clone();
    let (old_string, new_string) = (old_text.to_string(), new_text.to_string());
    let options = options(cx);
    let differences = compare::compare(&old_string, &new_string, options);
    let [old_marks, new_marks] = marks(&old_text, &new_text, &differences);
    let [old_inline, new_inline] = if settings.inline {
        compare::inline_differences(&old_string, &new_string, &differences, options)
    } else {
        [Vec::new(), Vec::new()]
    };
    for (buffer, side, inline) in [
        (old_buffer, old_marks, old_inline),
        (new_buffer, new_marks, new_inline),
    ] {
        buffer.update(cx, |buffer, cx| {
            buffer.update_marks(cx, |marks, _| {
                marks.compare.clear();
                for (range, change) in side {
                    marks.compare.insert(range, change);
                }
                marks.compare_inline.clear();
                for range in inline {
                    marks.compare_inline.insert(range, ());
                }
            });
        });
    }
    let [old_padding, new_padding] = if settings.align {
        compare::padding(&differences)
    } else {
        [Vec::new(), Vec::new()]
    };
    old.update(cx, |view, cx| view.set_padding(old_padding, cx));
    new.update(cx, |view, cx| view.set_padding(new_padding, cx));
    differences.is_empty()
}
