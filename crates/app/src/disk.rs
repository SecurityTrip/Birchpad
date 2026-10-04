//! Files changed by other programs (ADR 0018). The folders of open files are watched, and open
//! files are checked again when the window comes back to the front. A changed file with no
//! unsaved changes is reloaded (silently, or after asking); with unsaved changes the user
//! chooses; a deleted file is kept or closed. Questions wait until the window is in front.
//! View > Monitoring (tail -f) follows a file, reading only what was appended to it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use birchpad_config::ChangeDetection;
use birchpad_io::DiskStamp;
use futures::StreamExt as _;
use gpui_kit::{
    AppContext as _, AsyncWindowContext, Context, Entity, PromptLevel, Task, WeakEntity, Window,
};
use notify::Watcher as _;

use crate::app_state::AppState;
use crate::buffer::{Buffer, ReadOnly};
use crate::commands::CommandRegistry;
use crate::workspace::Workspace;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.monitoring", |this, (), window, cx| {
        this.toggle_monitoring(window, cx)
    });
}

/// Watching files of a workspace.
#[derive(Default)]
pub(crate) struct DiskState {
    watcher: Option<notify::RecommendedWatcher>,
    /// The folders watched.
    watched: HashSet<PathBuf>,
    checking: bool,
    /// Something changed during a check: check again after it.
    again: bool,
    tasks: Vec<Task<()>>,
}

/// A buffer to check: its file and how it looked last.
struct Candidate {
    buffer: Entity<Buffer>,
    path: PathBuf,
}

impl Workspace {
    /// Starts watching the folders of open files, and checking monitored files every second
    /// (watchers miss changes on some network drives).
    pub(crate) fn start_watching(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sender, mut events) = futures::channel::mpsc::unbounded::<()>();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // Reading a file is no change; reloading one must not set off another check.
            if event.is_ok_and(|event| !event.kind.is_access()) {
                sender.unbounded_send(()).ok();
            }
        });
        match watcher {
            Ok(watcher) => self.disk_state.watcher = Some(watcher),
            Err(error) => eprintln!("cannot watch files for changes: {error}"),
        }
        let listening = cx.spawn_in(window, async move |this, cx| {
            while events.next().await.is_some() {
                // A program writing a file sets off several events: check once they stop.
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                while events.try_recv().is_ok() {}
                let checked = this.update_in(cx, |this, window, cx| this.check_files(window, cx));
                if checked.is_err() {
                    break;
                }
            }
        });
        let polling = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let checked = this.update_in(cx, |this, window, cx| {
                    if this.monitored_buffers(cx).next().is_some() {
                        this.check_files(window, cx);
                    }
                });
                if checked.is_err() {
                    break;
                }
            }
        });
        self.disk_state.tasks = vec![listening, polling];
        self.sync_watches(cx);
    }

    fn monitored_buffers<'a>(
        &self,
        cx: &'a gpui_kit::App,
    ) -> impl Iterator<Item = Entity<Buffer>> + 'a {
        self.all_views(cx)
            .into_iter()
            .map(|view| view.read(cx).buffer.clone())
            .filter(|buffer| buffer.read(cx).is_monitoring())
    }

    /// Watches the folders of the files that are checked for changes, and only those.
    pub(crate) fn sync_watches(&mut self, cx: &mut Context<Self>) {
        if self.disk_state.watcher.is_none() {
            return;
        }
        let detection = AppState::global(cx).settings.files.change_detection;
        let wanted: HashSet<PathBuf> = self
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.read(cx))
            .filter(|buffer| detection != ChangeDetection::Off || buffer.is_monitoring())
            .filter_map(|buffer| buffer.path()?.parent().map(Path::to_owned))
            .collect();
        let Some(watcher) = &mut self.disk_state.watcher else {
            return;
        };
        for dir in self.disk_state.watched.difference(&wanted) {
            watcher.unwatch(dir).ok();
        }
        let mut watched = HashSet::new();
        for dir in wanted {
            let ok = self.disk_state.watched.contains(&dir)
                || watcher
                    .watch(&dir, notify::RecursiveMode::NonRecursive)
                    .is_ok();
            if ok {
                watched.insert(dir);
            }
        }
        self.disk_state.watched = watched;
    }

    /// Checks whether other programs changed or deleted open files, and deals with each.
    pub(crate) fn check_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disk_state.checking {
            self.disk_state.again = true;
            return;
        }
        let detection = AppState::global(cx).settings.files.change_detection;
        let active = self
            .active_view(cx)
            .map(|view| view.read(cx).buffer.clone());
        let mut seen = HashSet::new();
        let candidates: Vec<Candidate> = self
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.clone())
            .filter(|buffer| seen.insert(buffer.entity_id()))
            .filter(|buffer| {
                let state = buffer.read(cx);
                state.is_monitoring()
                    || match detection {
                        ChangeDetection::All => true,
                        ChangeDetection::Current => active.as_ref() == Some(buffer),
                        ChangeDetection::Off => false,
                    }
            })
            .filter_map(|buffer| {
                let path = buffer.read(cx).path()?.to_owned();
                Some(Candidate { buffer, path })
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        self.disk_state.checking = true;
        cx.spawn_in(window, async move |this, cx| {
            let paths: Vec<PathBuf> = candidates.iter().map(|c| c.path.clone()).collect();
            let stamps = cx
                .background_spawn(async move {
                    paths
                        .iter()
                        .map(|path| birchpad_io::stamp(path))
                        .collect::<Vec<_>>()
                })
                .await;
            for (candidate, now) in candidates.into_iter().zip(stamps) {
                // A file that cannot be looked at (a disconnected network drive) is left
                // alone.
                if let Ok(now) = now {
                    handle_change(&this, &candidate.buffer, now, cx).await;
                }
            }
            this.update_in(cx, |this, window, cx| {
                this.disk_state.checking = false;
                if std::mem::take(&mut this.disk_state.again) {
                    this.check_files(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// View > Monitoring (tail -f) for the active document.
    fn toggle_monitoring(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let Some(view) = self.active_view(cx) else {
            return Ok(());
        };
        let buffer = view.read(cx).buffer.clone();
        let (monitoring, has_path, modified, name) = {
            let buffer = buffer.read(cx);
            (
                buffer.is_monitoring(),
                buffer.path().is_some(),
                buffer.is_modified(),
                buffer.display_name(),
            )
        };
        if !monitoring && !has_path {
            bail!("Monitoring follows a file: save {name} first");
        }
        if !monitoring && modified {
            bail!("Save or undo the changes to {name} before monitoring it");
        }
        buffer.update(cx, |buffer, cx| buffer.set_monitoring(!monitoring, cx));
        self.sync_watches(cx);
        self.refresh_menus(cx);
        if !monitoring {
            self.check_files(window, cx);
        }
        Ok(())
    }
}

/// Deals with what happened to a buffer's file: `now` is how it looks (`None`: deleted).
async fn handle_change(
    workspace: &WeakEntity<Workspace>,
    buffer: &Entity<Buffer>,
    now: Option<DiskStamp>,
    cx: &mut AsyncWindowContext,
) {
    let (path, disk, modified, monitoring, busy) = buffer.read_with(cx, |buffer, _| {
        (
            buffer.path().map(Path::to_owned),
            buffer.disk(),
            buffer.is_modified(),
            buffer.is_monitoring(),
            buffer.is_saving() || buffer.read_only() == Some(ReadOnly::Loading),
        )
    });
    let Some(path) = path else {
        return;
    };
    if busy || disk == now {
        return;
    }
    let Ok(settings) = cx.update(|_, cx| AppState::global(cx).settings.files.clone()) else {
        return;
    };
    match now {
        Some(now) if monitoring => tail(buffer, &path, now, cx).await,
        Some(_) if !modified && settings.reload_silently => {
            buffer.update(cx, |buffer, cx| {
                buffer.reload(settings.reload_scrolls_to_end, cx);
            });
        }
        Some(_) => {
            let question = if modified {
                (
                    "was changed by another program. Reload it and lose your changes?",
                    ["Reload", "Keep My Changes"],
                )
            } else {
                (
                    "was changed by another program. Reload it?",
                    ["Reload", "Don't Reload"],
                )
            };
            let Some(answer) = ask(workspace, buffer, &path, question, cx).await else {
                return;
            };
            buffer.update(cx, |buffer, cx| {
                if answer == 0 {
                    buffer.reload(settings.reload_scrolls_to_end, cx);
                } else {
                    buffer.keep_changed(now, cx);
                }
            });
        }
        None => {
            let question = (
                "does not exist any more. Keep it in the editor?",
                ["Keep in Editor", "Close"],
            );
            let Some(answer) = ask(workspace, buffer, &path, question, cx).await else {
                return;
            };
            if answer == 0 {
                buffer.update(cx, |buffer, cx| buffer.keep_deleted(cx));
            } else {
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        for view in workspace.all_views(cx) {
                            if &view.read(cx).buffer == buffer {
                                workspace.close_view(&view, window, cx);
                            }
                        }
                    })
                    .ok();
            }
        }
    }
}

/// Asks about a file, with its tab in front. `None` while the window is in the background (the
/// question waits until it is back in front) or if the user dismissed the question.
async fn ask(
    workspace: &WeakEntity<Workspace>,
    buffer: &Entity<Buffer>,
    path: &Path,
    (message, answers): (&str, [&str; 2]),
    cx: &mut AsyncWindowContext,
) -> Option<usize> {
    let active = cx
        .update(|window, _| window.is_window_active())
        .unwrap_or(false);
    if !active {
        return None;
    }
    let name = buffer.read_with(cx, |buffer, _| buffer.display_name());
    workspace
        .update_in(cx, |workspace, window, cx| {
            let view = workspace
                .all_views(cx)
                .into_iter()
                .find(|view| &view.read(cx).buffer == buffer);
            if let Some(view) = view {
                workspace.activate_view(&view, window, cx);
            }
        })
        .ok()?;
    let detail = path.display().to_string();
    let answer = cx.prompt(
        PromptLevel::Warning,
        &format!("{name} {message}"),
        Some(&detail),
        &answers,
    );
    answer.await.ok().filter(|answer| *answer < 2)
}

/// Monitoring: adds what was appended to the file, or reads it again if it was rewritten.
async fn tail(buffer: &Entity<Buffer>, path: &Path, now: DiskStamp, cx: &mut AsyncWindowContext) {
    let (disk, head, encoding) = buffer.read_with(cx, |buffer, _| {
        (buffer.disk(), buffer.head(), buffer.doc().format().encoding)
    });
    let path = path.to_owned();
    let appended = match (disk, head) {
        (Some(disk), Some(head)) if now.len >= disk.len => {
            cx.background_spawn(async move {
                if !head.matches(&path).ok()? {
                    return None;
                }
                let bytes = birchpad_io::read_from(&path, disk.len).ok()?;
                let (text, used) = birchpad_io::decode_appended(&bytes, encoding)?;
                let read = DiskStamp {
                    len: disk.len + used as u64,
                    modified: now.modified,
                };
                Some((text, read))
            })
            .await
        }
        _ => None,
    };
    buffer.update(cx, |buffer, cx| match appended {
        Some((text, read)) => buffer.append_from_disk(&text, read, true, cx),
        None => buffer.reload(true, cx),
    });
}
