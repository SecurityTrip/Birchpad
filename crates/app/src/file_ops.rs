//! File commands: open, save, save as, save all, close with confirmation (one document, all,
//! all but this or the pinned ones, those left or right of it, the unchanged ones), exit,
//! recent files.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use birchpad_io::{SaveError, Unencodable};
use gpui_kit::{
    App, AppContext as _, AsyncWindowContext, Context, Entity, PromptLevel, Task, WeakEntity,
    Window,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::buffer::{Buffer, ReadOnly};
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::encoding_ui::first_position;
use crate::workspace::{Workspace, report_error};

#[derive(Deserialize)]
struct RecentArgs {
    path: PathBuf,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("file.open", |this, (), window, cx| {
        this.open_with_dialog(window, cx);
        Ok(())
    });
    registry.workspace("file.open-recent", |this, args: RecentArgs, window, cx| {
        if !args.path.exists() {
            AppState::update_state(cx, |state, _| state.remove_recent(&args.path));
            this.refresh_menus(cx);
            bail!(
                "{}",
                t!("file-ops.no-longer-exists", path = args.path.display())
            );
        }
        this.open_path(&args.path, window, cx);
        Ok(())
    });
    registry.workspace("file.clear-recent", |this, (), _, cx| {
        AppState::update_state(cx, |state, _| state.recent_files.clear());
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("file.save", |this, (), window, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        this.save(view, SaveMode::Save, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.save-as", |this, (), window, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        this.save(view, SaveMode::SaveAs, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.save-all", |this, (), window, cx| {
        this.save_all(window, cx).detach();
        Ok(())
    });
    registry.workspace("file.close", |this, (), window, cx| {
        let views = this.active_view(cx).into_iter().collect();
        this.close_with_confirmation(views, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.close-all", |this, (), window, cx| {
        let views = this.all_views(cx);
        this.close_with_confirmation(views, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.close-others", |this, (), window, cx| {
        let active = this.active_view(cx);
        let views = this
            .all_views(cx)
            .into_iter()
            .filter(|view| Some(view) != active.as_ref())
            .collect();
        this.close_with_confirmation(views, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.close-all-but-pinned", |this, (), window, cx| {
        let views = this
            .all_views(cx)
            .into_iter()
            .filter(|view| !view.read(cx).tab.pinned)
            .collect();
        this.close_with_confirmation(views, window, cx).detach();
        Ok(())
    });
    for (id, left) in [("file.close-left", true), ("file.close-right", false)] {
        registry.workspace(id, move |this, (), window, cx| {
            let pane = this.active_pane().read(cx);
            let Some(active) = pane.active_item() else {
                return Ok(());
            };
            let at = pane.index_of(active).unwrap_or_default();
            let views = if left {
                pane.items()[..at].to_vec()
            } else {
                pane.items()[at + 1..].to_vec()
            };
            this.close_with_confirmation(views, window, cx).detach();
            Ok(())
        });
    }
    registry.workspace("file.close-unchanged", |this, (), window, cx| {
        let views = this
            .all_views(cx)
            .into_iter()
            .filter(|view| !view.read(cx).buffer.read(cx).is_modified())
            .collect();
        this.close_with_confirmation(views, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.exit", |this, (), window, cx| {
        this.exit(window, cx);
        Ok(())
    });
}

/// The folder of a file chosen in a dialog, for Default Directory > Remember.
pub(crate) fn remember_directory(path: &Path, cx: &mut App) {
    if let Some(folder) = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty())
    {
        let folder = folder.to_owned();
        AppState::update_state(cx, |state, _| state.last_directory = Some(folder));
    }
}

/// Where the copy of the file goes before a save overwrites it (Preferences > Backup > Backup
/// on save): next to it (`simple`), or in `folder` (a `nppBackup` folder next to the file if
/// empty) with the time in its name (`verbose`).
pub(crate) fn backup_path(
    target: &Path,
    mode: birchpad_config::BackupOnSave,
    folder: &str,
    now: chrono::NaiveDateTime,
) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy().into_owned();
    match mode {
        birchpad_config::BackupOnSave::None => None,
        birchpad_config::BackupOnSave::Simple => Some(target.with_file_name(format!("{name}.bak"))),
        birchpad_config::BackupOnSave::Verbose => {
            let folder = match folder.trim() {
                "" => target.parent()?.join("nppBackup"),
                folder => PathBuf::from(folder),
            };
            let stamp = now.format("%Y-%m-%d_%H%M%S");
            Some(folder.join(format!("{name}.{stamp}.bak")))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveMode {
    /// Save to the buffer's file; untitled buffers ask for a path.
    Save,
    /// Always ask for a path.
    SaveAs,
}

/// Why writing failed; unencodable text is reported with its position.
enum WriteFailure {
    Unencodable(Unencodable),
    Save(SaveError),
    /// The copy of Backup on save could not be made; the file is left as it was.
    Backup(PathBuf, std::io::Error),
}

impl Workspace {
    fn open_with_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let directory = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(directory, cx).await else {
                return;
            };
            if let Some(first) = paths.first() {
                cx.update(|_, cx| remember_directory(first, cx)).ok();
            }
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.open_path(&path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The folder dialogs start in (Preferences > Default Directory): the active file's, the one
    /// last used, or one of one's own; else the home folder.
    pub(crate) fn default_directory(&self, cx: &App) -> PathBuf {
        let app = AppState::global(cx);
        let files = &app.settings.files;
        let chosen = match files.default_directory {
            birchpad_config::DefaultDirectory::Follow => None,
            birchpad_config::DefaultDirectory::Remember => app.state.last_directory.clone(),
            birchpad_config::DefaultDirectory::Custom => {
                Some(PathBuf::from(files.default_directory_path.trim()))
            }
        };
        if let Some(folder) = chosen.filter(|folder| folder.is_dir()) {
            return folder;
        }
        self.active_view(cx)
            .and_then(|view| {
                let buffer = view.read(cx).buffer.read(cx);
                buffer.path().and_then(Path::parent).map(Path::to_owned)
            })
            .or_else(home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Saves the buffer shown in `view`. Resolves to `true` if it was written, `false` if the
    /// user cancelled; errors are shown to the user and resolve to `false` too.
    pub(crate) fn save(
        &mut self,
        view: Entity<EditorView>,
        mode: SaveMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let buffer = view.read(cx).buffer.clone();
        let blocked = match buffer.read(cx).read_only() {
            Some(ReadOnly::Loading) => Some("the file is still being read".to_owned()),
            Some(ReadOnly::Decoding(_)) => Some(
                "the file was not decoded exactly; reopen it in the right encoding (Encoding \
                 menu) before saving"
                    .to_owned(),
            ),
            Some(ReadOnly::Requested) if mode == SaveMode::Save => {
                Some("the file was opened read-only (-ro); use Save As".to_owned())
            }
            Some(ReadOnly::File) if mode == SaveMode::Save => Some(format!(
                "{} is read-only; use Save As",
                buffer.read(cx).display_name()
            )),
            _ => None,
        };
        if let Some(reason) = blocked {
            report_error(
                &anyhow!("{}", t!("file-ops.cannot-save", reason = reason)),
                window,
                cx,
            );
            return Task::ready(false);
        }
        let existing = buffer.read(cx).path().map(Path::to_owned);
        let directory = self.default_directory(cx);
        let suggested = suggested_name(buffer.read(cx));
        cx.spawn_in(window, async move |this, cx| {
            let path = match (mode, existing) {
                (SaveMode::Save, Some(path)) => path,
                _ => match crate::path_dialog::ask_save(directory, suggested, cx).await {
                    Some(path) => {
                        cx.update(|_, cx| remember_directory(&path, cx)).ok();
                        path
                    }
                    None => return false,
                },
            };
            match write_buffer(&this, &buffer, path, cx).await {
                Ok(()) => true,
                Err(error) => {
                    cx.update(|window, cx| report_error(&error, window, cx))
                        .ok();
                    false
                }
            }
        })
    }

    /// Saves every modified buffer, asking for paths of untitled ones.
    fn save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let views = self.unique_buffer_views(cx);
        cx.spawn_in(window, async move |this, cx| {
            for view in views {
                let modified = view.read_with(cx, |view, cx| view.buffer.read(cx).is_modified());
                if !modified {
                    continue;
                }
                let saved = this.update_in(cx, |this, window, cx| {
                    this.activate_view(&view, window, cx);
                    this.save(view.clone(), SaveMode::Save, window, cx)
                });
                let Ok(task) = saved else {
                    return;
                };
                if !task.await {
                    return;
                }
            }
        })
    }

    /// One view per open buffer, in tab order.
    fn unique_buffer_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
        let mut seen = Vec::new();
        self.all_views(cx)
            .into_iter()
            .filter(|view| {
                let id = view.read(cx).buffer.entity_id();
                let new = !seen.contains(&id);
                seen.push(id);
                new
            })
            .collect()
    }

    /// Closes `views`, asking to save modified buffers first. Resolves to `false` if the user
    /// cancelled (the remaining views stay open).
    pub(crate) fn close_with_confirmation(
        &mut self,
        views: Vec<Entity<EditorView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        cx.spawn_in(window, async move |this, cx| {
            for view in views {
                if !confirm_close(&this, &view, cx).await {
                    return false;
                }
                if this
                    .update_in(cx, |this, window, cx| this.close_view(&view, window, cx))
                    .is_err()
                {
                    return false;
                }
            }
            true
        })
    }

    /// Asks about every modified document, saving those the user wants saved. Resolves to
    /// `false` if the user cancelled.
    pub(crate) fn ask_to_save_all(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let views = self.unique_buffer_views(cx);
        cx.spawn_in(window, async move |this, cx| {
            for view in views {
                if !ask_to_save(&this, &view, cx).await {
                    return false;
                }
            }
            true
        })
    }

    /// Whether quitting can go ahead without asking anything: unsaved changes are backed up,
    /// or there are none.
    pub(crate) fn can_quit_now(&self, cx: &App) -> bool {
        AppState::global(cx).backs_up_unsaved() || !self.has_unsaved_changes(cx)
    }

    /// Gets ready to quit: asks about unsaved changes unless they are backed up, then saves
    /// the session. Resolves to `false` if the user cancelled.
    pub(crate) fn prepare_to_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if self.can_quit_now(cx) {
            self.save_session_for_quit(cx);
            return Task::ready(true);
        }
        let asking = self.ask_to_save_all(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !asking.await {
                return false;
            }
            this.update(cx, |this, cx| this.save_session_for_quit(cx))
                .is_ok()
        })
    }

    /// File > Exit: asks about unsaved changes (unless they are backed up), saves the session
    /// and quits.
    pub(crate) fn exit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready = self.prepare_to_quit(window, cx);
        cx.spawn(async move |_, cx| {
            if ready.await {
                cx.update(|cx| cx.quit());
            }
        })
        .detach();
    }

    /// Whether closing the window needs confirmation; used by the window's close button.
    pub(crate) fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.all_views(cx)
            .iter()
            .any(|view| view.read(cx).buffer.read(cx).is_modified())
    }

    /// Remembers every open file as recently closed, before the window goes away.
    pub(crate) fn remember_open_files(&self, cx: &mut App) {
        let paths: Vec<PathBuf> = self
            .all_views(cx)
            .iter()
            .rev()
            .filter_map(|view| view.read(cx).buffer.read(cx).path().map(Path::to_owned))
            .collect();
        for path in paths {
            AppState::add_recent(&path, cx);
        }
    }
}

/// Encodes the buffer's current revision and writes it in the background.
async fn write_buffer(
    workspace: &WeakEntity<Workspace>,
    buffer: &Entity<Buffer>,
    path: PathBuf,
    cx: &mut AsyncWindowContext,
) -> Result<()> {
    let open_elsewhere = workspace.read_with(cx, |workspace, cx| {
        workspace.all_views(cx).iter().any(|view| {
            view.read(cx).buffer != *buffer && view.read(cx).buffer.read(cx).path() == Some(&path)
        })
    })?;
    if open_elsewhere {
        bail!(
            "{}",
            t!(
                "file-ops.is-open-in-another-tab-close-it-first-or-choose",
                path = path.display()
            )
        );
    }
    let (text, format, revision) = buffer.read_with(cx, |buffer, _| {
        let doc = buffer.doc();
        (doc.text().clone(), doc.format(), doc.revision())
    });
    let recovery = cx.update(|_, cx| AppState::global(cx).recovery_dir())?;
    let backup = cx.update(|_, cx| {
        let files = &AppState::global(cx).settings.files;
        backup_path(
            &path,
            files.backup_on_save,
            &files.backup_folder,
            chrono::Local::now().naive_local(),
        )
    })?;
    let target = path.clone();
    let encoded_text = text.clone();
    buffer.update(cx, |buffer, _| buffer.begin_save());
    let written = cx
        .background_spawn(async move {
            let bytes = birchpad_io::encode_document(&encoded_text, format)
                .map_err(WriteFailure::Unencodable)?;
            // The file as it was, before it is overwritten.
            if let Some(backup) = backup.filter(|_| target.is_file()) {
                if let Some(folder) = backup.parent() {
                    let _ = std::fs::create_dir_all(folder);
                }
                std::fs::copy(&target, &backup)
                    .map_err(|error| WriteFailure::Backup(backup.clone(), error))?;
            }
            let written = birchpad_io::save(&target, &bytes, recovery.as_deref())
                .map_err(WriteFailure::Save)?;
            // What the file looks like now, so that this write is not taken for a change
            // made by another program.
            let stamp = birchpad_io::stamp(&written).ok().flatten();
            Ok(stamp.map(|stamp| (stamp, birchpad_io::Head::of(&bytes))))
        })
        .await;
    if written.is_err() {
        buffer.update(cx, |buffer, _| buffer.end_save());
    }
    match written {
        Ok(disk) => {
            buffer.update(cx, |buffer, cx| {
                buffer.did_save(path.clone(), revision, text.clone(), disk, cx);
            });
            cx.update(|_, cx| AppState::remove_recent(&path, cx))?;
            workspace.update_in(cx, |workspace, window, cx| {
                workspace.refresh_menus(cx);
                if let Some(view) = workspace.view_of_buffer(buffer, cx) {
                    workspace.plugin_event(birchpad_plugin::EventKind::Saved, &view, window, cx);
                }
            })?;
            Ok(())
        }
        Err(WriteFailure::Unencodable(problem)) => Err(anyhow!(
            "{}",
            t!(
                "file-ops.cannot-save-remove-them-or-convert-the-document",
                path = path.display(),
                problem = problem,
                first_position = first_position(&text, &problem)
            )
        )),
        Err(WriteFailure::Save(error)) => Err(anyhow!(error)),
        Err(WriteFailure::Backup(backup, error)) => Err(anyhow!(
            "{}",
            t!(
                "file-ops.cannot-make-the-backup-copy",
                path = backup.display(),
                error = error
            )
        )),
    }
}

/// Asks whether to save the buffer of `view` before closing it. `true` means go ahead. A
/// document that another view still shows is not asked about.
async fn confirm_close(
    workspace: &WeakEntity<Workspace>,
    view: &Entity<EditorView>,
    cx: &mut AsyncWindowContext,
) -> bool {
    let Ok(shown_elsewhere) = workspace.read_with(cx, |workspace, cx| {
        let buffer = &view.read(cx).buffer;
        workspace
            .all_views(cx)
            .iter()
            .any(|other| other != view && &other.read(cx).buffer == buffer)
    }) else {
        return false;
    };
    shown_elsewhere || ask_to_save(workspace, view, cx).await
}

/// Asks whether to save the buffer of `view` if it is modified, and saves it on Save. `true`
/// unless the user cancelled or saving failed.
async fn ask_to_save(
    workspace: &WeakEntity<Workspace>,
    view: &Entity<EditorView>,
    cx: &mut AsyncWindowContext,
) -> bool {
    let (modified, name, path) = view.read_with(cx, |view, cx| {
        let buffer = view.buffer.read(cx);
        let path = buffer.path().map(|path| path.display().to_string());
        (buffer.is_modified(), buffer.display_name(), path)
    });
    if !modified {
        return true;
    }
    if workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.activate_view(view, window, cx)
        })
        .is_err()
    {
        return false;
    }
    let answers = [
        t!("common.save"),
        t!("file-ops.don-t-save"),
        t!("common.cancel"),
    ];
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    let answer = cx.prompt(
        PromptLevel::Warning,
        &t!("file-ops.save-question", name = name),
        path.as_deref(),
        &answers,
    );
    match answer.await {
        Ok(0) => {
            let saving = workspace.update_in(cx, |workspace, window, cx| {
                workspace.save(view.clone(), SaveMode::Save, window, cx)
            });
            match saving {
                Ok(task) => task.await,
                Err(_) => false,
            }
        }
        Ok(1) => true,
        _ => false,
    }
}

/// "new 1.txt" for untitled buffers, the file name otherwise.
fn suggested_name(buffer: &Buffer) -> String {
    match buffer.path().and_then(Path::file_name) {
        Some(name) => name.to_string_lossy().into_owned(),
        None => format!("{}.txt", buffer.display_name()),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "../tests/unit/file_ops/tests.rs"]
mod tests;
