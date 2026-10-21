//! More of Notepad++'s File menu: Open Containing Folder (in the file manager, a terminal or
//! Folder as Workspace), Open in Default Viewer, Reload from Disk, Save a Copy As, Rename, Move
//! to Recycle Bin, Restore Recent Closed File and Open All Recent Files; and Edit > Copy to
//! Clipboard's full path, file name and folder of the document.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, PromptLevel, WeakEntity, Window, div,
    prelude::*, px,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
pub use crate::editor::EditorView;
use crate::theme::{paint, ui};
use crate::workspace::{Workspace, report_error, report_info};

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace(
        "file.containing-folder",
        |this, args: FolderArgs, window, cx| {
            let path = this.active_file(cx)?;
            let folder = path.parent().map(Path::to_owned).unwrap_or_default();
            match args.target {
                FolderTarget::FileManager => shell(Shell::Reveal(path), cx)?,
                FolderTarget::Terminal => shell(Shell::Terminal(folder), cx)?,
                FolderTarget::Workspace => this.add_workspace_folder(&folder, window, cx),
            }
            Ok(())
        },
    );
    registry.workspace("file.open-default-viewer", |this, (), _, cx| {
        let path = this.active_file(cx)?;
        shell(Shell::Open(path), cx)
    });
    registry.workspace("file.reload", |this, (), window, cx| {
        let view = this.active_document(cx)?;
        this.reload_from_disk(&view, window, cx)
    });
    registry.workspace("file.save-copy-as", |this, (), window, cx| {
        let view = this.active_document(cx)?;
        this.save_copy_as(&view, window, cx);
        Ok(())
    });
    registry.workspace("file.rename", |this, (), window, cx| {
        let view = this.active_document(cx)?;
        open_rename(cx.entity().downgrade(), view, window, cx);
        Ok(())
    });
    registry.workspace("file.move-to-recycle-bin", |this, (), window, cx| {
        let view = this.active_document(cx)?;
        this.move_to_recycle_bin(&view, window, cx)
    });
    registry.workspace("file.restore-recent-closed", |this, (), window, cx| {
        let recent = AppState::global(cx).state.recent_files.clone();
        let next = recent
            .iter()
            .find(|path| path.exists() && this.view_for_path(path, cx).is_none());
        match next {
            Some(path) => this.open_path(&path.clone(), window, cx),
            None => report_info(t!("file-commands.no-recently-closed-file"), window, cx),
        }
        Ok(())
    });
    registry.workspace("file.open-all-recent", |this, (), window, cx| {
        let recent = AppState::global(cx).state.recent_files.clone();
        for path in recent {
            if path.exists() {
                this.open_path(&path, window, cx);
            } else {
                AppState::update_state(cx, |state, _| state.remove_recent(&path));
            }
        }
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("edit.copy-full-path", |this, (), _, cx| {
        let buffer = this.active_buffer(cx)?;
        let text = buffer.read(cx).path().map_or_else(
            || buffer.read(cx).display_name(),
            |path| path.display().to_string(),
        );
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        Ok(())
    });
    registry.workspace("edit.copy-file-name", |this, (), _, cx| {
        let name = this.active_buffer(cx)?.read(cx).display_name();
        cx.write_to_clipboard(ClipboardItem::new_string(name));
        Ok(())
    });
    registry.workspace("edit.copy-folder-path", |this, (), _, cx| {
        let path = this.active_file(cx)?;
        let folder = path.parent().unwrap_or(&path).display().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(folder));
        Ok(())
    });
}

#[derive(Deserialize)]
struct FolderArgs {
    #[serde(rename = "in")]
    target: FolderTarget,
}

/// Where Open Containing Folder opens the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FolderTarget {
    FileManager,
    Terminal,
    Workspace,
}

/// What is handed to the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    /// The file manager, showing this file.
    Reveal(PathBuf),
    /// A terminal in this folder.
    Terminal(PathBuf),
    /// The program the system opens this file with.
    Open(PathBuf),
    /// The web browser, at this address.
    Browse(String),
    /// A browser by name (`firefox`, `chrome`, `edge`, `safari`), showing this file.
    Browser(String, PathBuf),
}

#[cfg(feature = "test-support")]
thread_local! {
    /// What tests asked of the system, instead of doing it.
    pub static SHELL: std::cell::RefCell<Vec<Shell>> = std::cell::RefCell::default();
}

/// Hands `request` to the system (in tests, only notes it).
pub fn shell(request: Shell, cx: &mut App) -> Result<()> {
    #[cfg(feature = "test-support")]
    {
        let _ = &cx;
        SHELL.with(|shell| shell.borrow_mut().push(request));
        Ok(())
    }
    #[cfg(not(feature = "test-support"))]
    match request {
        Shell::Reveal(path) => {
            cx.reveal_path(&path);
            Ok(())
        }
        Shell::Terminal(folder) => crate::panels::open_terminal(&folder),
        Shell::Open(path) => {
            cx.open_with_system(&path);
            Ok(())
        }
        Shell::Browser(browser, path) => open_in_browser(&browser, &path),
        Shell::Browse(url) => {
            cx.open_url(&url);
            Ok(())
        }
    }
}

/// Opens `path` in the browser named `browser`.
#[cfg(not(feature = "test-support"))]
fn open_in_browser(browser: &str, path: &Path) -> Result<()> {
    use std::process::Command;
    let started = if cfg!(windows) {
        let program = match browser {
            "chrome" => "chrome",
            "edge" => "msedge",
            other => other,
        };
        Command::new("cmd")
            .args(["/C", "start", "", program])
            .arg(path)
            .spawn()
    } else if cfg!(target_os = "macos") {
        let app = match browser {
            "firefox" => "Firefox",
            "chrome" => "Google Chrome",
            "edge" => "Microsoft Edge",
            _ => "Safari",
        };
        Command::new("open").args(["-a", app]).arg(path).spawn()
    } else {
        let program = match browser {
            "chrome" => "google-chrome",
            "edge" => "microsoft-edge",
            other => other,
        };
        Command::new(program).arg(path).spawn()
    };
    started.map(|_| ()).map_err(|error| {
        anyhow!(
            "{}",
            t!(
                "file-commands.cannot-start-the-browser",
                browser = browser,
                error = error
            )
        )
    })
}

/// Moves `path` to the system's recycle bin (in tests, deletes it).
fn trash(path: &Path) -> Result<()> {
    #[cfg(feature = "test-support")]
    {
        std::fs::remove_file(path)?;
        Ok(())
    }
    #[cfg(not(feature = "test-support"))]
    trash::delete(path).map_err(|error| anyhow!("{error}"))
}

/// Why a name cannot be a document's new name, if it cannot.
pub fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("{}", t!("file-commands.the-name-is-empty"));
    }
    if name.contains(['/', '\\']) || matches!(name, "." | "..") {
        bail!(
            "{}",
            t!("file-commands.is-not-a-name-of-a-file", name = name)
        );
    }
    Ok(())
}

impl Workspace {
    fn active_document(&self, cx: &App) -> Result<Entity<EditorView>> {
        self.active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))
    }

    fn active_buffer(&self, cx: &App) -> Result<Entity<Buffer>> {
        Ok(self.active_document(cx)?.read(cx).buffer.clone())
    }

    /// The active document's file; an error for an untitled document.
    fn active_file(&self, cx: &App) -> Result<PathBuf> {
        let buffer = self.active_buffer(cx)?;
        let buffer = buffer.read(cx);
        buffer.path().map(Path::to_owned).ok_or_else(|| {
            anyhow!(
                "{}",
                t!(
                    "file-commands.has-no-file-yet",
                    name = buffer.display_name()
                )
            )
        })
    }

    /// File > Reload from Disk: reads the file again, after asking whether to drop unsaved
    /// changes.
    pub fn reload_from_disk(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let buffer = view.read(cx).buffer.clone();
        let (name, path, modified) = {
            let buffer = buffer.read(cx);
            (
                buffer.display_name(),
                buffer.path().map(Path::to_owned),
                buffer.is_modified(),
            )
        };
        let Some(path) = path else {
            bail!("{}", t!("file-commands.has-no-file-yet", name = name));
        };
        if !path.exists() {
            bail!(
                "{}",
                t!("file-commands.no-longer-exists", path = path.display())
            );
        }
        if !modified {
            buffer.update(cx, |buffer, cx| buffer.reload(false, cx));
            return Ok(());
        }
        let answers = [t!("disk.reload"), t!("common.cancel")];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = window.prompt(
            PromptLevel::Warning,
            &t!("file-commands.discard-changes-and-reload", name = name),
            Some(&path.display().to_string()),
            &answers,
            cx,
        );
        cx.spawn(async move |_, cx| {
            if answer.await.ok() == Some(0) {
                buffer.update(cx, |buffer, cx| buffer.reload(false, cx));
            }
        })
        .detach();
        Ok(())
    }

    /// File > Save a Copy As: writes the text to another file, in the document's encoding; the
    /// document stays as it is, with its own file.
    pub fn save_copy_as(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = view.read(cx).buffer.clone();
        let directory = self.default_directory(cx);
        let suggested = buffer.read(cx).display_name();
        let view = view.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Some(target) = crate::path_dialog::ask_save(directory, suggested, cx).await else {
                return;
            };
            let result = this
                .update_in(cx, |this, window, cx| {
                    this.write_copy(&view, &target, window, cx)
                })
                .and_then(|result| result);
            if let Err(error) = result {
                cx.update(|window, cx| report_error(&error, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Writes a copy of the document of `view` to `target`. The document's own file is saved
    /// as File > Save does; a file another tab shows is refused.
    pub fn write_copy(
        &mut self,
        view: &Entity<EditorView>,
        target: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let buffer = view.read(cx).buffer.clone();
        if buffer.read(cx).path() == Some(target) {
            self.save(view.clone(), crate::file_ops::SaveMode::Save, window, cx)
                .detach();
            return Ok(());
        }
        if self.view_for_path(target, cx).is_some() {
            bail!(
                "{}",
                t!(
                    "file-ops.is-open-in-another-tab-close-it-first-or-choose",
                    path = target.display()
                )
            );
        }
        let doc = buffer.read(cx).doc();
        let format = doc.format();
        let bytes = birchpad_io::encode_document(doc.text(), format)
            .map_err(|problem| anyhow!("{problem}"))?;
        let recovery = AppState::global(cx).recovery_dir();
        birchpad_io::save(target, &bytes, recovery.as_deref()).map_err(|error| anyhow!(error))?;
        Ok(())
    }

    /// File > Rename: the file in its folder gets the name `name`; an untitled document only
    /// shows it.
    pub fn rename(
        &mut self,
        view: &Entity<EditorView>,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let name = name.trim();
        check_name(name)?;
        let buffer = view.read(cx).buffer.clone();
        let Some(path) = buffer.read(cx).path().map(Path::to_owned) else {
            buffer.update(cx, |buffer, cx| buffer.set_title(Some(name.to_owned()), cx));
            return Ok(());
        };
        let target = path.with_file_name(name);
        if target == path {
            return Ok(());
        }
        // A new name that differs only in case is the same file on Windows and macOS.
        let same_file = target
            .to_string_lossy()
            .eq_ignore_ascii_case(&path.to_string_lossy());
        if target.exists() && !same_file {
            bail!(
                "{}",
                t!("file-commands.already-exists", path = target.display())
            );
        }
        if self.view_for_path(&target, cx).is_some() {
            bail!(
                "{}",
                t!(
                    "file-ops.is-open-in-another-tab-close-it-first-or-choose",
                    path = target.display()
                )
            );
        }
        std::fs::rename(&path, &target).map_err(|error| {
            anyhow!(
                "{}",
                t!(
                    "file-commands.cannot-rename",
                    path = path.display(),
                    error = error
                )
            )
        })?;
        buffer.update(cx, |buffer, cx| buffer.renamed(target, cx));
        self.sync_watches(cx);
        self.refresh_menus(cx);
        Ok(())
    }

    /// File > Move to Recycle Bin: after asking, the file goes to the recycle bin and its tabs
    /// close.
    pub fn move_to_recycle_bin(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let buffer = view.read(cx).buffer.clone();
        let Some(path) = buffer.read(cx).path().map(Path::to_owned) else {
            bail!(
                "{}",
                t!(
                    "file-commands.has-no-file-yet",
                    name = buffer.read(cx).display_name()
                )
            );
        };
        let answers = [t!("file-commands.move"), t!("common.cancel")];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = window.prompt(
            PromptLevel::Warning,
            &t!(
                "file-commands.move-to-the-recycle-bin",
                name = buffer.read(cx).display_name()
            ),
            Some(&path.display().to_string()),
            &answers,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let result = trash(&path).map_err(|error| {
                anyhow!(
                    "{}",
                    t!(
                        "file-commands.cannot-move-to-the-recycle-bin",
                        path = path.display(),
                        error = error
                    )
                )
            });
            this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    for view in this.all_views(cx) {
                        if view.read(cx).buffer == buffer {
                            this.close_view(&view, window, cx);
                        }
                    }
                    // A file in the recycle bin is not a recent file.
                    AppState::remove_recent(&path, cx);
                    this.refresh_menus(cx);
                }
                Err(error) => report_error(&error, window, cx),
            })
            .ok();
        })
        .detach();
        Ok(())
    }
}

/// File > Rename...: the new name, in a dialog.
fn open_rename(
    workspace: WeakEntity<Workspace>,
    view: Entity<EditorView>,
    window: &mut Window,
    cx: &mut App,
) {
    let current = view.read(cx).buffer.read(cx).display_name();
    let input = cx.new(|cx| InputState::new(window, cx).default_value(current));
    let error = cx.new(|_| None::<String>);
    let focus = input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let (input, error, workspace, view) = (
            input.clone(),
            error.clone(),
            workspace.clone(),
            view.clone(),
        );
        let shown = error.clone();
        dialog
            .title(t!("file-commands.rename"))
            .w(px(420.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Input::new(&input).id("rename-name"))
                    .child(ErrorLine(shown)),
            )
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("rename-ok").label(t!("file-commands.rename")),
                    )),
            )
            .on_ok(move |_, _, cx| {
                let name = input.read(cx).value().to_string();
                let result = workspace
                    .update(cx, |workspace, cx| workspace.rename(&view, &name, cx))
                    .unwrap_or(Ok(()));
                match result {
                    Ok(()) => true,
                    Err(problem) => {
                        error.update(cx, |error, cx| {
                            *error = Some(format!("{problem:#}"));
                            cx.notify();
                        });
                        false
                    }
                }
            })
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

/// An error line under a dialog's field.
#[derive(IntoElement)]
struct ErrorLine(Entity<Option<String>>);

impl RenderOnce for ErrorLine {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .debug_selector(|| "rename-error".into())
            .text_color(paint(ui().error))
            .children(self.0.read(cx).clone())
    }
}
