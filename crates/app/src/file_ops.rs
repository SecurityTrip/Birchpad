//! File commands: open, save, save as, save all, close with confirmation, exit, recent files.

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
use crate::commands::CommandRegistry;
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
            bail!("{} no longer exists", args.path.display());
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
        let view = this.active_view(cx).ok_or_else(|| anyhow!("no document"))?;
        this.save(view, SaveMode::Save, window, cx).detach();
        Ok(())
    });
    registry.workspace("file.save-as", |this, (), window, cx| {
        let view = this.active_view(cx).ok_or_else(|| anyhow!("no document"))?;
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
    registry.workspace("file.exit", |this, (), window, cx| {
        this.exit(window, cx);
        Ok(())
    });
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
}

impl Workspace {
    fn open_with_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let directory = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(directory, cx).await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.open_path(&path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The folder dialogs start in: the active file's, else the home folder.
    pub(crate) fn default_directory(&self, cx: &App) -> PathBuf {
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
            report_error(&anyhow!("Cannot save: {reason}"), window, cx);
            return Task::ready(false);
        }
        let existing = buffer.read(cx).path().map(Path::to_owned);
        let directory = self.default_directory(cx);
        let suggested = suggested_name(buffer.read(cx));
        cx.spawn_in(window, async move |this, cx| {
            let path = match (mode, existing) {
                (SaveMode::Save, Some(path)) => path,
                _ => match crate::path_dialog::ask_save(directory, suggested, cx).await {
                    Some(path) => path,
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
            "{} is open in another tab; close it first or choose another name",
            path.display()
        );
    }
    let (text, format, revision) = buffer.read_with(cx, |buffer, _| {
        let doc = buffer.doc();
        (doc.text().clone(), doc.format(), doc.revision())
    });
    let recovery = cx.update(|_, cx| AppState::global(cx).recovery_dir())?;
    let target = path.clone();
    let encoded_text = text.clone();
    buffer.update(cx, |buffer, _| buffer.begin_save());
    let written = cx
        .background_spawn(async move {
            let bytes = birchpad_io::encode(&encoded_text, format.encoding, format.bom)
                .map_err(WriteFailure::Unencodable)?;
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
            workspace.update(cx, |workspace, cx| workspace.refresh_menus(cx))?;
            Ok(())
        }
        Err(WriteFailure::Unencodable(problem)) => Err(anyhow!(
            "Cannot save {}: {problem}. {}. Remove them or convert the document to a Unicode \
             encoding (Encoding > Convert to UTF-8).",
            path.display(),
            first_position(&text, &problem)
        )),
        Err(WriteFailure::Save(error)) => Err(anyhow!(error)),
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
    let answer = cx.prompt(
        PromptLevel::Warning,
        &format!("Save changes to {name}?"),
        path.as_deref(),
        &["Save", "Don't Save", "Cancel"],
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
mod tests {
    use std::fs;

    use birchpad_commands::Invocation;
    use gpui_kit::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

    fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(path, window, cx)
        });
        cx.run_until_parked();
    }

    fn is_modified(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).is_modified()
        })
    }

    fn recent(cx: &mut VisualTestContext) -> Vec<PathBuf> {
        cx.update(|_, cx| AppState::global(cx).state.recent_files.clone())
    }

    #[gpui_kit::test]
    fn save_writes_the_file_in_its_encoding(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cp1251.txt");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../io/tests/fixtures/windows-1251-crlf.txt");
        let original = fs::read(fixture).unwrap();
        fs::write(&path, &original).unwrap();
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, &path, cx);
        assert!(active_text(&workspace, cx).starts_with("Съешь"));

        cx.simulate_input("!");
        assert!(is_modified(&workspace, cx));
        cx.simulate_keystrokes(&secondary("s"));
        cx.run_until_parked();
        assert!(!is_modified(&workspace, cx));
        let mut expected = b"!".to_vec();
        expected.extend_from_slice(&original);
        assert_eq!(fs::read(&path).unwrap(), expected);

        // Characters Windows-1251 lacks block the save and leave the file alone.
        cx.simulate_input("日");
        cx.simulate_keystrokes(&secondary("s"));
        cx.run_until_parked();
        assert!(is_modified(&workspace, cx));
        assert_eq!(fs::read(&path).unwrap(), expected);
    }

    #[gpui_kit::test]
    fn saving_an_untitled_document_asks_for_a_path(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.txt");
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_input("hello");
        cx.simulate_keystrokes(&secondary("s"));
        cx.run_until_parked();
        let chosen = target.clone();
        cx.simulate_new_path_selection(move |_| Some(chosen));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
        assert_eq!(tab_names(&workspace, cx), ["notes.txt"]);
        assert!(!is_modified(&workspace, cx));
    }

    #[gpui_kit::test]
    fn closing_a_modified_document_asks_first(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_keystrokes(&secondary("n"));
        cx.simulate_input("unsaved");
        assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);

        cx.simulate_keystrokes(&secondary("w"));
        cx.run_until_parked();
        assert!(cx.has_pending_prompt());
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);

        cx.simulate_keystrokes(&secondary("w"));
        cx.run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["new 1"]);

        // An unmodified document closes without a question.
        cx.simulate_keystrokes(&secondary("w"));
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt());
    }

    #[gpui_kit::test]
    fn close_all_saves_when_asked_and_stops_on_cancel(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        fs::write(&first, "1").unwrap();
        fs::write(&second, "2").unwrap();
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, &first, cx);
        cx.simulate_input("a");
        open(&workspace, &second, cx);
        cx.simulate_input("b");

        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("file.close-all"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
        cx.simulate_prompt_answer("Save");
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&first).unwrap(), "a1");
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["second.txt"]);
        assert_eq!(fs::read_to_string(&second).unwrap(), "2");
        assert_eq!(
            recent(cx),
            std::slice::from_ref(&first),
            "closed files become recent"
        );

        // Reopening a recent file removes it from the list.
        open(&workspace, &first, cx);
        assert!(recent(cx).is_empty());
    }

    #[gpui_kit::test]
    fn open_dialog_opens_every_chosen_file(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let paths = [dir.path().join("a.txt"), dir.path().join("b.txt")];
        for path in &paths {
            fs::write(path, "x").unwrap();
        }
        let (workspace, cx) = open_workspace(cx);
        cx.simulate_keystrokes(&secondary("o"));
        cx.run_until_parked();
        let chosen = paths.to_vec();
        cx.simulate_path_prompt_response(move |_| Some(chosen));
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["a.txt", "b.txt"]);
    }

    #[gpui_kit::test]
    fn dropped_files_open_in_tabs(cx: &mut TestAppContext) {
        use gpui_kit::{ExternalPaths, FileDropEvent, point, px};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dropped.txt");
        fs::write(&path, "dropped").unwrap();
        let (workspace, cx) = open_workspace(cx);
        let position = point(px(300.), px(300.));
        cx.simulate_event(FileDropEvent::Entered {
            position,
            paths: ExternalPaths(vec![path.clone()].into()),
        });
        cx.simulate_event(FileDropEvent::Submit { position });
        cx.run_until_parked();
        assert_eq!(tab_names(&workspace, cx), ["dropped.txt"]);
        assert_eq!(active_text(&workspace, cx), "dropped");
    }

    #[cfg(unix)]
    #[gpui_kit::test]
    fn read_only_files_cannot_be_edited_or_saved(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ro.txt");
        fs::write(&path, "fixed").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();

        let (workspace, cx) = open_workspace(cx);
        open(&workspace, &path, cx);
        cx.simulate_input("changed");
        assert_eq!(active_text(&workspace, cx), "fixed");
        cx.simulate_keystrokes(&secondary("s"));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fixed");
    }
}
