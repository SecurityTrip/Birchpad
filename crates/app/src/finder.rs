//! Search > Go to File...: the files of the workspace's folders (Folder as Workspace) and the
//! recent files, picked by typing parts of their path (`app work` finds
//! `notepad\crates\app\src\workspace.rs`). The recent files come first, as they are listed in
//! File > Recent Files; the folders' files follow as they are read, in the background. Which
//! files are listed, hidden ones too, and how many at most, is Preferences > Go to File's.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use birchpad_config::FinderSettings;
use birchpad_io::{Filters, FolderOptions};
use gpui_kit::{App, WeakEntity, Window};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::quick_pick::{self, PickItem, QuickPick};
use crate::workspace::Workspace;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("search.go-to-file", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

/// A file Go to File lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    /// As it is shown and matched: the full path of a recent file, the folder's name and the
    /// path in it for a folder's.
    pub text: String,
    pub recent: bool,
}

/// How a file of the folder `root` is shown: the folder's name and the path below it.
pub fn shown_path(root: &Path, path: &Path) -> String {
    let below = path.strip_prefix(root).unwrap_or(path);
    match root.file_name() {
        Some(name) => Path::new(name).join(below).display().to_string(),
        None => path.display().to_string(),
    }
}

/// The recent files that are still there, most recent first.
pub fn recent_files(recent: &[PathBuf]) -> Vec<Found> {
    recent
        .iter()
        .filter(|path| path.is_file())
        .map(|path| Found {
            path: path.clone(),
            text: path.display().to_string(),
            recent: true,
        })
        .collect()
}

/// The recent files, then the files of each folder (`(root, files)`) not listed already, up
/// to `limit` in all.
pub fn merge(recent: &[Found], folders: &[(PathBuf, Vec<PathBuf>)], limit: usize) -> Vec<Found> {
    let mut all: Vec<Found> = recent.iter().take(limit).cloned().collect();
    let mut known: std::collections::HashSet<PathBuf> =
        all.iter().map(|found| found.path.clone()).collect();
    'folders: for (root, files) in folders {
        for path in files {
            if all.len() >= limit {
                break 'folders;
            }
            if known.insert(path.clone()) {
                all.push(Found {
                    text: shown_path(root, path),
                    path: path.clone(),
                    recent: false,
                });
            }
        }
    }
    all
}

/// The files of `roots` that `settings` take, each folder's at most the limit; folders that
/// cannot be read give none. Stops early once `cancel` is set.
pub fn list_folders(
    roots: &[PathBuf],
    settings: &FinderSettings,
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<(PathBuf, Vec<PathBuf>)>> {
    let filters = Filters::parse(&settings.filters)
        .map_err(|error| anyhow::anyhow!("{}", t!("finder.filters", error = error)))?;
    let options = FolderOptions {
        subfolders: true,
        hidden: settings.hidden,
    };
    let limit = settings.file_limit();
    Ok(roots
        .iter()
        .map(|root| {
            let mut files =
                birchpad_io::files_in(root, &filters, options, cancel).unwrap_or_default();
            files.truncate(limit);
            (root.clone(), files)
        })
        .collect())
}

/// Sets its flag when dropped: stops the listing once the dialog is closed.
struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn items(found: &[Found]) -> (Vec<PickItem>, usize) {
    let recent = found.iter().filter(|found| found.recent).count();
    let items = found
        .iter()
        .map(|found| {
            let hint = if found.recent {
                t!("finder.recent")
            } else {
                String::new()
            };
            PickItem::new(found.text.clone(), hint)
        })
        .collect();
    (items, recent)
}

/// Opens Go to File.
pub fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let app = AppState::global(cx);
    let settings = app.settings.finder.clone();
    let roots = app.state.panels.folders.clone();
    let recent = recent_files(&app.state.recent_files);
    let limit = settings.file_limit();
    let found = Rc::new(std::cell::RefCell::new(merge(&recent, &[], limit)));
    let (first, recent_count) = items(&found.borrow());
    let cancel = Arc::new(AtomicBool::new(false));
    let guard = CancelOnDrop(cancel.clone());
    let empty = if roots.is_empty() {
        t!("finder.no-files")
    } else {
        t!("finder.reading-folders")
    };
    let picked = found.clone();
    let pick = quick_pick::open(
        t!("finder.go-to-file"),
        t!("finder.type-parts-of-a-path"),
        empty,
        first,
        recent_count,
        Rc::new(move |index, window, cx| {
            let _ = &guard;
            let Some(path) = picked.borrow().get(index).map(|found| found.path.clone()) else {
                return;
            };
            workspace
                .update(cx, |workspace, cx| workspace.open_path(&path, window, cx))
                .ok();
        }),
        window,
        cx,
    );
    if roots.is_empty() {
        return;
    }
    let pick = pick.downgrade();
    window
        .spawn(cx, async move |cx| {
            let listed: anyhow::Result<Vec<(PathBuf, Vec<PathBuf>)>> = cx
                .background_executor()
                .spawn(async move { list_folders(&roots, &settings, &cancel) })
                .await;
            cx.update(|window, cx| match listed {
                Ok(folders) => {
                    let all = merge(&recent, &folders, limit);
                    let (list, recent) = items(&all);
                    *found.borrow_mut() = all;
                    show(&pick, list, recent, t!("finder.no-file-matches"), cx);
                }
                Err(error) => {
                    show(&pick, Vec::new(), 0, t!("finder.no-file-matches"), cx);
                    crate::workspace::report_error(&error, window, cx);
                }
            })
            .ok();
        })
        .detach();
}

fn show(
    pick: &WeakEntity<QuickPick>,
    list: Vec<PickItem>,
    recent: usize,
    empty: String,
    cx: &mut App,
) {
    pick.update(cx, |pick, cx| {
        pick.set_empty(empty);
        if !list.is_empty() {
            pick.set_items(list, recent, cx);
        }
    })
    .ok();
}
