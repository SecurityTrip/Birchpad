//! Tools > Git (ADR 0066): the lines changed since the last commit in the Git margin, read
//! from HEAD's text of each open file, the blame of the caret's line, the next and the previous
//! change; Compare with Last Commit is Plugins > Compare's Compare to Git HEAD.
//!
//! As with the language servers, each window tells [`sync_workspace`] which documents it has,
//! [`edited`] after each edit, and [`refresh_heads`] when it comes back to the front.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use birchpad_core::{Encoding, Rope};
use birchpad_editor::buffer::{Buffer, ReadOnly};
use birchpad_editor::{GitMark, GitMarks};
use birchpad_git::{Git, GitChange, GitError};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, EntityId, Global, Task, WeakEntity,
};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

/// How long edits wait before the margin is compared again.
pub const COMPARE_DELAY: Duration = Duration::from_millis(300);

struct GitDoc {
    workspace: EntityId,
    buffer: WeakEntity<Buffer>,
    path: PathBuf,
    /// HEAD's text of the file, when Git tracks it.
    head: Option<Rope>,
    /// HEAD's text was read (or found missing).
    read: bool,
    marks: Rc<Vec<(Range<usize>, GitMark)>>,
    /// Tells a comparison's result from that of a later one.
    generation: u64,
    reading: Option<Task<()>>,
    comparing: Option<Task<()>>,
}

/// The documents of every window, with HEAD's text and their marks.
#[derive(Default)]
pub struct GitState {
    docs: HashMap<EntityId, GitDoc>,
    /// The program HEAD's texts were read with.
    program: String,
    /// Git that cannot run is told once.
    warned: bool,
    window: Option<AnyWindowHandle>,
}

impl Global for GitState {}

fn state(cx: &mut App) -> &mut GitState {
    if !cx.has_global::<GitState>() {
        cx.set_global(GitState::default());
    }
    cx.global_mut::<GitState>()
}

/// The Git program of the settings.
pub fn git(cx: &App) -> Git {
    Git::new(AppState::global(cx).settings.git.program())
}

/// Whether Git is asked about `buffer`: with Git on, a file with a path, read, not large.
fn wanted(buffer: &Buffer, cx: &App) -> Option<PathBuf> {
    if !AppState::global(cx).settings.git.enabled
        || buffer.read_only() == Some(ReadOnly::Loading)
        || buffer.is_large(cx)
    {
        return None;
    }
    buffer.path().map(Path::to_owned)
}

/// Brings the documents Git is asked about in step with `workspace`'s: new ones get HEAD's
/// text read, closed ones are forgotten, renamed ones are read again.
pub fn sync_workspace(workspace: &Workspace, window: AnyWindowHandle, cx: &mut Context<Workspace>) {
    let id = cx.entity_id();
    let mut wanted_docs: Vec<(Entity<Buffer>, PathBuf)> = Vec::new();
    for view in workspace.all_views(cx) {
        let buffer = view.read(cx).buffer.clone();
        if wanted_docs
            .iter()
            .any(|(known, _)| known.entity_id() == buffer.entity_id())
        {
            continue;
        }
        if let Some(path) = wanted(buffer.read(cx), cx) {
            wanted_docs.push((buffer, path));
        }
    }
    let program = git(cx).program;
    if !cx.has_global::<GitState>() && wanted_docs.is_empty() {
        return;
    }
    let state = state(cx);
    state.window = Some(window);
    let reread_all = state.program != program;
    state.program = program;
    state.docs.retain(|buffer, doc| {
        doc.workspace != id
            || wanted_docs
                .iter()
                .any(|(known, path)| known.entity_id() == *buffer && *path == doc.path)
    });
    let mut to_read = Vec::new();
    for (buffer, path) in wanted_docs {
        let buffer_id = buffer.entity_id();
        let known = state.docs.contains_key(&buffer_id);
        if !known {
            state.docs.insert(
                buffer_id,
                GitDoc {
                    workspace: id,
                    buffer: buffer.downgrade(),
                    path,
                    head: None,
                    read: false,
                    marks: Rc::default(),
                    generation: 0,
                    reading: None,
                    comparing: None,
                },
            );
        }
        if !known || reread_all {
            to_read.push(buffer_id);
        }
    }
    for buffer in to_read {
        read_head(buffer, cx);
    }
    cx.refresh_windows();
}

/// Reads HEAD's text of the documents of `workspace` again: a commit, a checkout or a reset
/// may have changed it while the window was in the background.
pub fn refresh_heads(workspace: EntityId, cx: &mut App) {
    let Some(state) = cx.try_global::<GitState>() else {
        return;
    };
    let docs: Vec<EntityId> = state
        .docs
        .iter()
        .filter(|(_, doc)| doc.workspace == workspace)
        .map(|(id, _)| *id)
        .collect();
    for doc in docs {
        read_head(doc, cx);
    }
}

/// The file of `buffer` was read again from disk: HEAD's text may have changed too.
pub fn reloaded(buffer: &Entity<Buffer>, cx: &mut App) {
    if cx
        .try_global::<GitState>()
        .is_some_and(|state| state.docs.contains_key(&buffer.entity_id()))
    {
        read_head(buffer.entity_id(), cx);
    }
}

fn read_head(id: EntityId, cx: &mut App) {
    let Some(doc) = state(cx).docs.get(&id) else {
        return;
    };
    let (path, weak) = (doc.path.clone(), doc.buffer.clone());
    let Some(buffer) = weak.upgrade() else {
        return;
    };
    let encoding = buffer.read(cx).doc().format().encoding;
    let git = git(cx);
    let reading = cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move { git.head_text(&path) })
            .await;
        cx.update(|cx| head_read(id, result, encoding, cx));
    });
    if let Some(doc) = state(cx).docs.get_mut(&id) {
        doc.reading = Some(reading);
    }
}

fn head_read(
    id: EntityId,
    result: Result<Option<Vec<u8>>, GitError>,
    encoding: Encoding,
    cx: &mut App,
) {
    let mut warning = None;
    let state = state(cx);
    let Some(doc) = state.docs.get_mut(&id) else {
        return;
    };
    doc.read = true;
    doc.reading = None;
    doc.head = match result {
        Ok(Some(bytes)) => Some(Rope::from(
            birchpad_io::decode(bytes, encoding).text.as_str(),
        )),
        Ok(None) => None,
        Err(error) => {
            if !state.warned {
                state.warned = true;
                warning = Some(error.to_string());
            }
            None
        }
    };
    if let Some(message) = warning
        && let Some(window) = state.window
    {
        cx.defer(move |cx| {
            let _ = cx.update_window(window, |_, window, cx| {
                crate::workspace::report_warning(message, window, cx);
            });
        });
    }
    compare(id, cx);
}

/// Compares a document with HEAD's text in the background, and marks the margin.
fn compare(id: EntityId, cx: &mut App) {
    let Some(doc) = state(cx).docs.get_mut(&id) else {
        return;
    };
    let Some(head) = doc.head.clone() else {
        doc.marks = Rc::default();
        doc.comparing = None;
        cx.refresh_windows();
        return;
    };
    let Some(buffer) = doc.buffer.upgrade() else {
        return;
    };
    doc.generation += 1;
    let generation = doc.generation;
    let text = buffer.read(cx).doc().text().clone();
    let comparing = cx.spawn(async move |cx| {
        let changes = cx
            .background_spawn(async move { birchpad_git::line_changes(&head, &text) })
            .await;
        cx.update(|cx| {
            if let Some(doc) = state(cx).docs.get_mut(&id)
                && doc.generation == generation
            {
                doc.marks = Rc::new(
                    changes
                        .into_iter()
                        .map(|(lines, change)| (lines, mark(change)))
                        .collect(),
                );
                doc.comparing = None;
                cx.refresh_windows();
            }
        });
    });
    if let Some(doc) = state(cx).docs.get_mut(&id) {
        doc.comparing = Some(comparing);
    }
}

fn mark(change: GitChange) -> GitMark {
    match change {
        GitChange::Added => GitMark::Added,
        GitChange::Modified => GitMark::Modified,
        GitChange::Deleted => GitMark::Deleted,
    }
}

/// A document was edited: its margin is compared again once edits pause.
pub fn edited(buffer: &Entity<Buffer>, cx: &mut App) {
    let id = buffer.entity_id();
    let tracked = cx
        .try_global::<GitState>()
        .and_then(|state| state.docs.get(&id))
        .is_some_and(|doc| doc.head.is_some());
    if !tracked {
        return;
    }
    let waiting = cx.spawn(async move |cx| {
        cx.background_executor().timer(COMPARE_DELAY).await;
        cx.update(|cx| compare(id, cx));
    });
    if let Some(doc) = state(cx).docs.get_mut(&id) {
        doc.comparing = Some(waiting);
    }
}

/// The marks of a buffer for the editor's Git margin: `None` when Git does not track it.
pub fn marks(buffer: EntityId, cx: &App) -> GitMarks {
    let doc = cx.try_global::<GitState>()?.docs.get(&buffer)?;
    doc.head.as_ref().map(|_| doc.marks.clone())
}

/// Why `buffer` has no Git changes to go to or blame.
fn untracked_reason(buffer: &Entity<Buffer>, cx: &App) -> String {
    let buffer = buffer.read(cx);
    if !AppState::global(cx).settings.git.enabled {
        return t!("git-changes.turned-off");
    }
    if buffer.path().is_none() {
        return t!("git-changes.save-first");
    }
    t!("git-changes.not-tracked", name = buffer.display_name())
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("git.blame", |this, (), window, cx| blame(this, window, cx));
    for (id, forward) in [("git.next-change", true), ("git.previous-change", false)] {
        registry.workspace(id, move |this, (), _, cx| go_to_change(this, forward, cx));
    }
}

fn active(this: &Workspace, cx: &App) -> Result<Entity<crate::editor::EditorView>> {
    this.active_view(cx)
        .ok_or_else(|| anyhow!("{}", t!("language-servers.no-document")))
}

/// Tools > Git > Blame Line: the caret's line's commit in the call tip.
fn blame(
    this: &mut Workspace,
    window: &mut gpui_kit::Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let view = active(this, cx)?;
    let buffer = view.read(cx).buffer.clone();
    if !AppState::global(cx).settings.git.enabled {
        bail!("{}", t!("git-changes.turned-off"));
    }
    let Some(path) = buffer.read(cx).path().map(Path::to_owned) else {
        bail!("{}", t!("git-changes.save-first"));
    };
    let text = buffer.read(cx).doc().text().clone();
    let head = view.read(cx).selection.primary().head;
    let line = birchpad_core::motion::line_of(&text, head);
    let git = git(cx);
    cx.spawn_in(window, async move |_, cx| {
        let contents = text.to_string().into_bytes();
        let result = cx
            .background_spawn(async move { git.blame_line(&path, line, &contents) })
            .await;
        cx.update(|window, cx| match result {
            Ok(blame) => view.update(cx, |view, cx| view.show_tip(blame.describe(), cx)),
            Err(error) => report_error(&anyhow!("{error}"), window, cx),
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// Tools > Git > Next Change and Previous Change.
fn go_to_change(this: &mut Workspace, forward: bool, cx: &mut Context<Workspace>) -> Result<()> {
    let view = active(this, cx)?;
    let buffer = view.read(cx).buffer.clone();
    let Some(found) = marks(buffer.entity_id(), cx) else {
        bail!("{}", untracked_reason(&buffer, cx));
    };
    let changes: birchpad_git::LineChanges = found
        .iter()
        .map(|(lines, mark)| {
            let change = match mark {
                GitMark::Added => GitChange::Added,
                GitMark::Modified => GitChange::Modified,
                GitMark::Deleted => GitChange::Deleted,
            };
            (lines.clone(), change)
        })
        .collect();
    let text = buffer.read(cx).doc().text().clone();
    let head = view.read(cx).selection.primary().head;
    let line = birchpad_core::motion::line_of(&text, head);
    let Some(target) = birchpad_git::next_change(&changes, line, forward) else {
        bail!(
            "{}",
            t!(
                "git-changes.no-changes",
                name = buffer.read(cx).display_name()
            )
        );
    };
    let target = target.min(birchpad_core::motion::line_count(&text).saturating_sub(1));
    let start = birchpad_core::motion::line_range(&text, target).start;
    view.update(cx, |view, cx| view.go_to(start, cx));
    Ok(())
}

/// A window closed: its documents are forgotten.
pub fn forget_workspace(workspace: EntityId, cx: &mut App) {
    if cx.has_global::<GitState>() {
        state(cx).docs.retain(|_, doc| doc.workspace != workspace);
    }
}
