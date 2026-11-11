//! Folder as Workspace, as Notepad++'s: folders shown as trees, from which files open.
//!
//! Folders come from File > Open Folder as Workspace..., from folders dropped on the window or
//! named on the command line, and are remembered in `state.toml`. A folder's contents are read
//! when it is first unfolded, folders before files, each in name order without regard to
//! case, and read again every two seconds while it is unfolded, so files that appear, go or
//! are renamed show. A double-click or Enter opens a file; a click on a folder folds or unfolds
//! it. The right-click menu copies paths and names, searches the folder with Find in Files,
//! shows it in the system's file manager, opens a terminal there, or removes a top folder from
//! the workspace.
//!
//! Files are worked with as in an IDE's project tree: New File and New Folder, Rename (F2),
//! Copy, Cut and Paste (Ctrl+C, Ctrl+X, Ctrl+V), Duplicate (Ctrl+D), Move to Recycle Bin
//! (Delete, asked first unless MISC > Folder as Workspace: confirm moving to the recycle bin is
//! off) and moving by dragging onto a folder. The file operations are `birchpad-io`'s
//! (`entries`); open documents follow their files when they are renamed or moved, an unfolded
//! folder stays unfolded where it went, and the tabs of files moved to the recycle bin close
//! unless they have unsaved changes. Top folders are renamed but not moved, duplicated or
//! deleted from here.

use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, ClickEvent, ClipboardItem, Context, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    Modifiers, MouseButton, PromptLevel, ScrollStrategy, SharedString, Task,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};

use crate::app_state::AppState;
use crate::editor::widgets::ListRow as _;
use crate::file_commands::{Shell, ask_name, shell, trash};
use crate::workspace::{Workspace, report_error};

const ROW_HEIGHT: f32 = 22.;
const INDENT: f32 = 14.;
/// How often unfolded folders are read again.
pub const POLL: Duration = Duration::from_secs(2);

/// A file or folder in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// The contents of `folder`: folders first, then files, each in name order without regard to
/// case. Entries whose kind cannot be read are left out.
pub fn list_folder(folder: &Path) -> io::Result<Vec<Entry>> {
    list_folder_with(folder, true)
}

/// As [`list_folder`]; without `follow_links` (MISC > Allow loading symlinks in Folder as
/// Workspace off), links to folders are left out.
pub fn list_folder_with(folder: &Path, follow_links: bool) -> io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(folder)?
        .flatten()
        .filter_map(|entry| {
            let kind = entry.file_type().ok()?;
            // A link to a folder is a folder; to a file, a file.
            let link_to_folder =
                kind.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir());
            if link_to_folder && !follow_links {
                return None;
            }
            let is_dir = kind.is_dir() || link_to_folder;
            Some(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct Node {
    path: PathBuf,
    pub name: String,
    is_dir: bool,
    pub expanded: bool,
    /// `None` until the folder has been read.
    pub children: Option<Vec<Node>>,
    /// Why the folder could not be read.
    pub error: Option<String>,
}

impl Node {
    fn new(path: PathBuf, name: String, is_dir: bool) -> Self {
        Self {
            path,
            name,
            is_dir,
            expanded: false,
            children: None,
            error: None,
        }
    }

    pub fn root(path: PathBuf) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Self::new(path, name, true)
    }

    /// Takes a new listing, keeping what is known of the folders that are still there.
    pub fn set_listing(&mut self, listing: io::Result<Vec<Entry>>) {
        match listing {
            Ok(entries) => {
                let mut old: Vec<Node> = self.children.take().unwrap_or_default();
                let children = entries
                    .into_iter()
                    .map(|entry| {
                        let path = self.path.join(&entry.name);
                        match old
                            .iter()
                            .position(|node| node.path == path && node.is_dir == entry.is_dir)
                        {
                            Some(index) => old.swap_remove(index),
                            None => Node::new(path, entry.name, entry.is_dir),
                        }
                    })
                    .collect();
                self.children = Some(children);
                self.error = None;
            }
            Err(error) => {
                self.children = Some(Vec::new());
                self.error = Some(error.to_string());
            }
        }
    }

    pub fn find_mut(&mut self, path: &Path) -> Option<&mut Node> {
        if self.path == path {
            return Some(self);
        }
        if !path.starts_with(&self.path) {
            return None;
        }
        self.children
            .as_mut()?
            .iter_mut()
            .find_map(|child| child.find_mut(path))
    }

    /// The folders that are unfolded and read, this one included.
    fn open_folders(&self, out: &mut Vec<PathBuf>) {
        if self.is_dir && self.expanded && self.children.is_some() {
            out.push(self.path.clone());
            for child in self.children.iter().flatten() {
                child.open_folders(out);
            }
        }
    }

    /// Takes the node of `path` out of the tree under this one.
    fn take(&mut self, path: &Path) -> Option<Node> {
        if !path.starts_with(&self.path) {
            return None;
        }
        let children = self.children.as_mut()?;
        if let Some(index) = children.iter().position(|child| child.path == path) {
            return Some(children.remove(index));
        }
        children.iter_mut().find_map(|child| child.take(path))
    }

    /// The node and those under it follow their file or folder from `from` to `to`.
    fn relocate(&mut self, from: &Path, to: &Path) {
        if let Some(path) = birchpad_io::relocated(&self.path, from, to) {
            if self.path == from {
                self.name = to.file_name().map_or_else(
                    || to.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
            }
            self.path = path;
        }
        for child in self.children.iter_mut().flatten() {
            child.relocate(from, to);
        }
    }

    fn set_expanded_all(&mut self, expanded: bool) {
        if self.is_dir {
            self.expanded = expanded && self.children.is_some();
            for child in self.children.iter_mut().flatten() {
                child.set_expanded_all(expanded);
            }
        }
    }
}

/// A row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    pub is_root: bool,
    /// A folder that could not be read says why.
    pub error: Option<String>,
}

pub fn push_rows(node: &Node, depth: usize, is_root: bool, rows: &mut Vec<Row>) {
    rows.push(Row {
        path: node.path.clone(),
        name: node.name.clone(),
        depth,
        is_dir: node.is_dir,
        expanded: node.expanded,
        is_root,
        error: node.error.clone(),
    });
    if node.expanded {
        for child in node.children.iter().flatten() {
            push_rows(child, depth + 1, false, rows);
        }
    }
}

/// What Copy or Cut set aside for Paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub path: PathBuf,
    /// Cut: Paste moves it; otherwise Paste copies it.
    pub cut: bool,
}

/// A file or folder dragged in the tree, to be dropped on a folder.
#[derive(Clone)]
pub struct DraggedEntry {
    pub path: PathBuf,
    name: SharedString,
}

impl DraggedEntry {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            name: name.into(),
        }
    }
}

impl Render for DraggedEntry {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(crate::theme::paint(crate::theme::ui().selected))
            .child(self.name.clone())
    }
}

pub struct FolderWorkspace {
    workspace: WeakEntity<Workspace>,
    roots: Vec<Node>,
    pub selected: Option<PathBuf>,
    /// What Copy or Cut set aside.
    pub clipboard: Option<Clip>,
    scroll: UniformListScrollHandle,
    pub focus_handle: FocusHandle,
    _poll: Task<()>,
}

impl Focusable for FolderWorkspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FolderWorkspace {
    pub fn new(workspace: WeakEntity<Workspace>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let roots: Vec<Node> = AppState::global(cx)
            .state
            .panels
            .folders
            .iter()
            .cloned()
            .map(Node::root)
            .collect();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            workspace,
            roots,
            selected: None,
            clipboard: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    /// The top folders.
    pub fn roots(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|root| root.path.clone()).collect()
    }

    fn remember(&self, cx: &mut Context<Self>) {
        let folders = self.roots();
        AppState::update_state(cx, |state, _| state.panels.folders = folders);
    }

    /// Adds `folder` to the workspace, unfolded and selected; a folder already there is only
    /// selected.
    pub fn add_root(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let folder = std::path::absolute(&folder).unwrap_or(folder);
        if !self.roots.iter().any(|root| root.path == folder) {
            self.roots.push(Node::root(folder.clone()));
            self.remember(cx);
        }
        self.selected = Some(folder.clone());
        self.expand(&folder, cx);
    }

    pub fn remove_root(&mut self, folder: &Path, cx: &mut Context<Self>) {
        self.roots.retain(|root| root.path != folder);
        if self
            .selected
            .as_deref()
            .is_some_and(|selected| selected.starts_with(folder))
        {
            self.selected = None;
        }
        self.remember(cx);
        cx.notify();
    }

    pub fn remove_all(&mut self, cx: &mut Context<Self>) {
        self.roots.clear();
        self.selected = None;
        self.remember(cx);
        cx.notify();
    }

    fn node_mut(&mut self, path: &Path) -> Option<&mut Node> {
        self.roots.iter_mut().find_map(|root| root.find_mut(path))
    }

    /// Unfolds the folder at `path`, reading it first if it has not been read.
    pub fn expand(&mut self, path: &Path, cx: &mut Context<Self>) {
        let follow = AppState::global(cx).settings.misc.follow_symlinks;
        let Some(node) = self.node_mut(path) else {
            return;
        };
        if !node.is_dir {
            return;
        }
        node.expanded = true;
        if node.children.is_none() {
            node.set_listing(list_folder_with(path, follow));
        }
        cx.notify();
    }

    pub fn collapse(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(node) = self.node_mut(path) {
            node.expanded = false;
            cx.notify();
        }
    }

    fn toggle(&mut self, path: &Path, cx: &mut Context<Self>) {
        let expanded = self.node_mut(path).is_some_and(|node| node.expanded);
        if expanded {
            self.collapse(path, cx);
        } else {
            self.expand(path, cx);
        }
    }

    /// Folds every folder, or unfolds those already read.
    pub fn set_all(&mut self, expanded: bool, cx: &mut Context<Self>) {
        for root in &mut self.roots {
            root.set_expanded_all(expanded);
        }
        cx.notify();
    }

    /// Reads the unfolded folders again, in the background.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let mut folders = Vec::new();
        for root in &self.roots {
            root.open_folders(&mut folders);
        }
        if folders.is_empty() {
            return;
        }
        let follow = AppState::global(cx).settings.misc.follow_symlinks;
        let listing = cx.background_spawn(async move {
            folders
                .into_iter()
                .map(|folder| {
                    let listing = list_folder_with(&folder, follow);
                    (folder, listing)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let listed = listing.await;
            this.update(cx, |this, cx| {
                let mut changed = false;
                for (folder, listing) in listed {
                    let Some(node) = this.node_mut(&folder) else {
                        continue;
                    };
                    let before: Vec<(PathBuf, bool)> = node
                        .children
                        .iter()
                        .flatten()
                        .map(|child| (child.path.clone(), child.is_dir))
                        .collect();
                    let error = node.error.clone();
                    node.set_listing(listing);
                    let after: Vec<(PathBuf, bool)> = node
                        .children
                        .iter()
                        .flatten()
                        .map(|child| (child.path.clone(), child.is_dir))
                        .collect();
                    changed |= before != after || error != node.error;
                }
                if changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Unfolds the folders down to `file` and selects it; false if no top folder holds it.
    pub fn locate(&mut self, file: &Path, cx: &mut Context<Self>) -> bool {
        let Some(root) = self
            .roots
            .iter()
            .map(|root| root.path.clone())
            .find(|root| file.starts_with(root))
        else {
            return false;
        };
        let mut folder = root.clone();
        self.expand(&folder, cx);
        let relative = file.strip_prefix(&root).unwrap_or(file).to_owned();
        let parts: Vec<_> = relative.components().collect();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            folder.push(part);
            self.expand(&folder, cx);
        }
        self.selected = Some(file.to_owned());
        if let Some(index) = self.rows().iter().position(|row| row.path == file) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
        cx.notify();
        true
    }

    /// The rows shown, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for root in &self.roots {
            push_rows(root, 0, true, &mut rows);
        }
        rows
    }

    // --- Working with files ----------------------------------------------------------------

    fn is_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| root.path == path)
    }

    /// Refuses to move, duplicate or delete a top folder.
    fn not_a_root(&self, path: &Path) -> Result<()> {
        if self.is_root(path) {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            bail!(
                "{}",
                t!("panels.folder-workspace.top-folder-stays", name = name)
            );
        }
        Ok(())
    }

    /// The folder of the entry at `path`: itself, or the folder a file is in.
    pub fn folder_of(path: &Path) -> PathBuf {
        if path.is_dir() {
            path.to_owned()
        } else {
            path.parent().map(Path::to_owned).unwrap_or_default()
        }
    }

    /// Runs `operation` on `path` with the folders in it no longer watched for changes of open
    /// files, then watches what is still wanted.
    fn letting_go<T, E>(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&Path) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let folder = path.to_owned();
        self.workspace
            .update(cx, |workspace, _| workspace.unwatch_under(&folder))
            .ok();
        let result = operation(path);
        if result.is_err() {
            self.workspace
                .update(cx, |workspace, cx| workspace.sync_watches(cx))
                .ok();
        }
        result
    }

    /// Explorer Here: the system's file manager, showing the entry itself.
    pub fn reveal(&self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = shell(Shell::Reveal(path.to_owned()), cx) {
            report_error(&error, window, cx);
        }
    }

    /// Reads `folder` again now, if it has been read.
    fn reread(&mut self, folder: &Path, cx: &mut Context<Self>) {
        let follow = AppState::global(cx).settings.misc.follow_symlinks;
        if let Some(node) = self.node_mut(folder)
            && node.children.is_some()
        {
            node.set_listing(list_folder_with(folder, follow));
        }
        cx.notify();
    }

    /// Shows `path`, just made in `folder`: the folder read again and unfolded, the entry
    /// selected.
    fn show_new(&mut self, folder: &Path, path: &Path, cx: &mut Context<Self>) {
        self.reread(folder, cx);
        self.expand(folder, cx);
        self.select(path);
        cx.notify();
    }

    fn select(&mut self, path: &Path) {
        self.selected = Some(path.to_owned());
        if let Some(index) = self.rows().iter().position(|row| row.path == path) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
    }

    /// New File: the empty file `name` in `folder`, opened in the editor.
    pub fn new_file(
        &mut self,
        folder: &Path,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        let path = birchpad_io::create_file(folder, name.trim())?;
        self.show_new(folder, &path, cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.open_path(&path, window, cx))
            .ok();
        Ok(path)
    }

    /// New Folder: the folder `name` in `folder`.
    pub fn new_folder(
        &mut self,
        folder: &Path,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        let path = birchpad_io::create_folder(folder, name.trim())?;
        self.show_new(folder, &path, cx);
        Ok(path)
    }

    /// Rename: the file or folder at `path` gets the name `name`; a top folder too.
    pub fn rename_entry(
        &mut self,
        path: &Path,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        let to = self.letting_go(path, cx, |path| birchpad_io::rename(path, name.trim()))?;
        if to != path {
            self.moved(path, &to, cx);
        }
        Ok(to)
    }

    /// Moves the file or folder at `path` into `folder` (dragged there, or cut and pasted).
    pub fn move_entry(
        &mut self,
        path: &Path,
        folder: &Path,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        self.not_a_root(path)?;
        let to = self.letting_go(path, cx, |path| birchpad_io::move_into(path, folder))?;
        if to != path {
            self.moved(path, &to, cx);
            self.expand(folder, cx);
            self.select(&to);
        }
        Ok(to)
    }

    /// Duplicate: a copy beside the entry, under a name of its own.
    pub fn duplicate_entry(&mut self, path: &Path, cx: &mut Context<Self>) -> Result<PathBuf> {
        self.not_a_root(path)?;
        let copy = birchpad_io::duplicate(path)?;
        let folder = copy.parent().map(Path::to_owned).unwrap_or_default();
        self.show_new(&folder, &copy, cx);
        Ok(copy)
    }

    /// Copy: Paste copies `path`.
    pub fn copy_entry(&mut self, path: &Path) {
        self.clipboard = Some(Clip {
            path: path.to_owned(),
            cut: false,
        });
    }

    /// Cut: Paste moves `path`.
    pub fn cut_entry(&mut self, path: &Path) -> Result<()> {
        self.not_a_root(path)?;
        self.clipboard = Some(Clip {
            path: path.to_owned(),
            cut: true,
        });
        Ok(())
    }

    /// Paste: what was copied, or cut, into the folder of `target`.
    pub fn paste_into(&mut self, target: &Path, cx: &mut Context<Self>) -> Result<PathBuf> {
        let Some(clip) = self.clipboard.clone() else {
            bail!("{}", t!("panels.folder-workspace.nothing-to-paste"));
        };
        let folder = Self::folder_of(target);
        if clip.cut {
            let to = self.move_entry(&clip.path, &folder, cx)?;
            self.clipboard = None;
            Ok(to)
        } else {
            let copy = birchpad_io::copy_into(&clip.path, &folder)?;
            self.show_new(&folder, &copy, cx);
            Ok(copy)
        }
    }

    /// Move to Recycle Bin, after asking when MISC asks to.
    pub fn delete_entry(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.not_a_root(path)?;
        if !AppState::global(cx).settings.misc.confirm_workspace_delete {
            return self.remove_entry(path, window, cx);
        }
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let answers = [t!("file-commands.move"), t!("common.cancel")];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = window.prompt(
            PromptLevel::Warning,
            &t!("file-commands.move-to-the-recycle-bin", name = name),
            Some(&path.display().to_string()),
            &answers,
            cx,
        );
        let path = path.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.remove_entry(&path, window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
        })
        .detach();
        Ok(())
    }

    /// Moves `path` to the recycle bin now; the tabs of its files close.
    fn remove_entry(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.letting_go(path, cx, trash).map_err(|error| {
            anyhow::anyhow!(
                "{}",
                t!(
                    "file-commands.cannot-move-to-the-recycle-bin",
                    path = path.display(),
                    error = error
                )
            )
        })?;
        let folder = path.parent().map(Path::to_owned).unwrap_or_default();
        self.reread(&folder, cx);
        if self
            .selected
            .as_deref()
            .is_some_and(|selected| selected.starts_with(path))
        {
            self.selected = Some(folder);
        }
        if self
            .clipboard
            .as_ref()
            .is_some_and(|clip| clip.path.starts_with(path))
        {
            self.clipboard = None;
        }
        let removed = path.to_owned();
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.close_removed(&removed, window, cx)
            })
            .ok();
        Ok(())
    }

    /// After the entry `from` became `to`: the tree keeps what it knew of it (an unfolded
    /// folder stays unfolded), and the selection, what was set aside for Paste and the open
    /// documents follow it.
    fn moved(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        if let Some(root) = self.roots.iter_mut().find(|root| root.path == from) {
            root.relocate(from, to);
            self.remember(cx);
        } else {
            let node = self.roots.iter_mut().find_map(|root| root.take(from));
            if let Some(mut node) = node
                && let Some(parent) = to.parent()
                && let Some(children) = self
                    .node_mut(parent)
                    .and_then(|parent| parent.children.as_mut())
            {
                node.relocate(from, to);
                children.push(node);
            }
            for folder in [from.parent(), to.parent()].into_iter().flatten() {
                self.reread(folder, cx);
            }
        }
        if let Some(selected) = self
            .selected
            .as_deref()
            .and_then(|selected| birchpad_io::relocated(selected, from, to))
        {
            self.selected = Some(selected);
        }
        if let Some(clip) = self.clipboard.as_mut()
            && let Some(path) = birchpad_io::relocated(&clip.path, from, to)
        {
            clip.path = path;
        }
        let (from, to) = (from.to_owned(), to.to_owned());
        self.workspace
            .update(cx, |workspace, cx| workspace.follow_moved(&from, &to, cx))
            .ok();
        cx.notify();
    }

    /// New File...: its name, in a dialog.
    pub fn ask_new_file(&self, folder: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let (this, folder) = (cx.entity().downgrade(), folder.to_owned());
        ask_name(
            t!("panels.folder-workspace.new-file-title"),
            t!("panels.folder-workspace.create"),
            String::new(),
            move |name, window, cx| {
                this.update(cx, |this, cx| this.new_file(&folder, name, window, cx))
                    .map_or(Ok(()), |made| made.map(|_| ()))
            },
            window,
            cx,
        );
    }

    /// New Folder...: its name, in a dialog.
    pub fn ask_new_folder(&self, folder: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let (this, folder) = (cx.entity().downgrade(), folder.to_owned());
        ask_name(
            t!("panels.folder-workspace.new-folder-title"),
            t!("panels.folder-workspace.create"),
            String::new(),
            move |name, _, cx| {
                this.update(cx, |this, cx| this.new_folder(&folder, name, cx))
                    .map_or(Ok(()), |made| made.map(|_| ()))
            },
            window,
            cx,
        );
    }

    /// Rename...: the new name, in a dialog that starts with the old one.
    pub fn ask_rename(&self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let (this, path) = (cx.entity().downgrade(), path.to_owned());
        let current = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        ask_name(
            t!("panels.folder-workspace.rename-title"),
            t!("panels.folder-workspace.rename-title"),
            current,
            move |name, _, cx| {
                this.update(cx, |this, cx| this.rename_entry(&path, name, cx))
                    .map_or(Ok(()), |renamed| renamed.map(|_| ()))
            },
            window,
            cx,
        );
    }

    /// Runs a file operation of the menu or a key, telling what went wrong.
    fn run(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        operation: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) -> Result<()>,
    ) {
        if let Err(error) = operation(self, window, cx) {
            report_error(&error, window, cx);
        }
        cx.notify();
    }

    /// A drop of `dragged` on the row of `target`: it moves into the target's folder.
    pub fn drop_on(
        &mut self,
        dragged: &DraggedEntry,
        target: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = Self::folder_of(target);
        // On itself, or where it already is: nothing to do.
        if folder == dragged.path || dragged.path.parent() == Some(folder.as_path()) {
            return;
        }
        let path = dragged.path.clone();
        self.run(window, cx, |this, _, cx| {
            this.move_entry(&path, &folder, cx).map(|_| ())
        });
    }

    /// The file operation of a key on the selected row, if the key has one: F2, Delete, and
    /// Ctrl with C, X, V or D.
    fn file_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selected) = self.selected.clone() else {
            return false;
        };
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if modifiers == Modifiers::secondary_key(cx) {
            match key {
                "c" => self.copy_entry(&selected),
                "x" => self.run(window, cx, |this, _, _| this.cut_entry(&selected)),
                "v" => self.run(window, cx, |this, _, cx| {
                    this.paste_into(&selected, cx).map(|_| ())
                }),
                "d" => self.run(window, cx, |this, _, cx| {
                    this.duplicate_entry(&selected, cx).map(|_| ())
                }),
                _ => return false,
            }
            return true;
        }
        if modifiers.modified() {
            return false;
        }
        match key {
            "f2" => self.ask_rename(&selected, window, cx),
            "delete" => self.run(window, cx, |this, window, cx| {
                this.delete_entry(&selected, window, cx)
            }),
            _ => return false,
        }
        true
    }

    /// Opens a file in the editor, or folds or unfolds a folder.
    fn activate(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        if row.is_dir {
            self.toggle(&row.path, cx);
            return;
        }
        let path = row.path.clone();
        self.workspace
            .update(cx, |workspace, cx| workspace.open_path(&path, window, cx))
            .ok();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.modified() {
            return;
        }
        let rows = self.rows();
        let index = self
            .selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.path == selected));
        let select = |this: &mut Self, index: usize| {
            if let Some(row) = rows.get(index) {
                this.selected = Some(row.path.clone());
                this.scroll.scroll_to_item(index, ScrollStrategy::Center);
            }
        };
        match event.keystroke.key.as_str() {
            "down" if !rows.is_empty() => {
                select(self, index.map_or(0, |i| (i + 1).min(rows.len() - 1)))
            }
            "up" if !rows.is_empty() => select(self, index.map_or(0, |i| i.saturating_sub(1))),
            "right" => {
                if let Some(row) = index.map(|i| &rows[i]).filter(|row| row.is_dir) {
                    self.expand(&row.path, cx);
                }
            }
            "left" => match index.map(|i| &rows[i]) {
                Some(row) if row.is_dir && row.expanded => self.collapse(&row.path, cx),
                Some(row) => {
                    // To the folder above.
                    if let Some(parent) = rows[..index.unwrap_or(0)]
                        .iter()
                        .rposition(|above| above.depth + 1 == row.depth)
                    {
                        select(self, parent);
                    }
                }
                None => {}
            },
            "enter" => {
                if let Some(row) = index.map(|i| rows[i].clone()) {
                    self.activate(&row, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn context_menu(
        &self,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let this = cx.entity().downgrade();
        let workspace = self.workspace.clone();
        let row = row.clone();
        let pasting = self.clipboard.is_some();
        move |menu, _, _| {
            let path = row.path.clone();
            let folder = if row.is_dir {
                row.path.clone()
            } else {
                row.path.parent().map(Path::to_owned).unwrap_or_default()
            };
            let copy = |text: String| {
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                }
            };
            let mut menu = menu;
            let op = |label: String,
                      operation: fn(
                &mut FolderWorkspace,
                &Path,
                &mut Window,
                &mut Context<FolderWorkspace>,
            ) -> Result<()>| {
                let (this, path) = (this.clone(), path.clone());
                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.run(window, cx, |this, window, cx| {
                            operation(this, &path, window, cx)
                        })
                    })
                    .ok();
                })
            };
            menu = menu
                .item(op(
                    t!("panels.folder-workspace.new-file"),
                    |this, path, window, cx| {
                        this.ask_new_file(&FolderWorkspace::folder_of(path), window, cx);
                        Ok(())
                    },
                ))
                .item(op(
                    t!("panels.folder-workspace.new-folder"),
                    |this, path, window, cx| {
                        this.ask_new_folder(&FolderWorkspace::folder_of(path), window, cx);
                        Ok(())
                    },
                ))
                .separator()
                .item(op(
                    t!("panels.folder-workspace.rename"),
                    |this, path, window, cx| {
                        this.ask_rename(path, window, cx);
                        Ok(())
                    },
                ));
            if !row.is_root {
                menu = menu.item(op(
                    t!("panels.folder-workspace.duplicate"),
                    |this, path, _, cx| this.duplicate_entry(path, cx).map(|_| ()),
                ));
            }
            menu = menu.item(op(
                t!("panels.folder-workspace.copy"),
                |this, path, _, _| {
                    this.copy_entry(path);
                    Ok(())
                },
            ));
            if !row.is_root {
                menu = menu.item(op(t!("panels.folder-workspace.cut"), |this, path, _, _| {
                    this.cut_entry(path)
                }));
            }
            if pasting {
                menu = menu.item(op(
                    t!("panels.folder-workspace.paste"),
                    |this, path, _, cx| this.paste_into(path, cx).map(|_| ()),
                ));
            }
            if !row.is_root {
                menu = menu.item(op(
                    t!("panels.folder-workspace.move-to-recycle-bin"),
                    |this, path, window, cx| this.delete_entry(path, window, cx),
                ));
            }
            menu = menu.separator();
            if !row.is_dir {
                let (workspace, file) = (workspace.clone(), path.clone());
                menu = menu.item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
                    workspace
                        .update(cx, |workspace, cx| workspace.open_path(&file, window, cx))
                        .ok();
                }));
                let file = path.clone();
                menu = menu.item(
                    PopupMenuItem::new("Run by System")
                        .on_click(move |_, _, cx| cx.open_with_system(&file)),
                );
                menu = menu.separator();
            }
            menu = menu
                .item(PopupMenuItem::new("Copy Path").on_click(copy(path.display().to_string())))
                .item(PopupMenuItem::new("Copy File Name").on_click(copy(row.name.clone())));
            if row.is_dir {
                let (workspace, searched) = (workspace.clone(), folder.clone());
                menu = menu
                    .separator()
                    .item(
                        PopupMenuItem::new("Find in Files...").on_click(move |_, window, cx| {
                            workspace
                                .update(cx, |workspace, cx| {
                                    workspace.find_in_folder(&searched, window, cx)
                                })
                                .ok();
                        }),
                    );
            }
            let (shown, terminal) = (path.clone(), folder.clone());
            let revealing = this.clone();
            menu = menu
                .separator()
                .item(
                    PopupMenuItem::new("Explorer Here").on_click(move |_, window, cx| {
                        revealing
                            .update(cx, |this, cx| this.reveal(&shown, window, cx))
                            .ok();
                    }),
                )
                .item(
                    PopupMenuItem::new("Terminal Here").on_click(move |_, window, cx| {
                        if let Err(error) = open_terminal(&terminal) {
                            crate::workspace::report_error(&error, window, cx);
                        }
                    }),
                );
            if row.is_root {
                let (this, removed) = (this.clone(), path.clone());
                let all = this.clone();
                menu = menu
                    .separator()
                    .item(PopupMenuItem::new("Remove").on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| this.remove_root(&removed, cx))
                            .ok();
                    }))
                    .item(PopupMenuItem::new("Remove All").on_click(move |_, _, cx| {
                        all.update(cx, |this, cx| this.remove_all(cx)).ok();
                    }));
            }
            menu
        }
    }

    fn render_row(
        &self,
        index: usize,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.selected.as_ref() == Some(&row.path);
        let arrow = if !row.is_dir {
            " "
        } else if row.expanded {
            "▾"
        } else {
            "▸"
        };
        let clicked = row.clone();
        let menu = self.context_menu(row, cx);
        let dragged = DraggedEntry {
            path: row.path.clone(),
            name: row.name.clone().into(),
        };
        let target = row.path.clone();
        let label = match &row.error {
            Some(error) => format!("{} ({error})", row.name),
            None => row.name.clone(),
        };
        div()
            .id(("folder-row", index))
            .w_full()
            .debug_selector(move || format!("folder-row-{index}"))
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(px(4. + row.depth as f32 * INDENT))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .list_row(selected)
            .child(div().w(px(INDENT)).flex_none().child(arrow))
            .child(
                div()
                    .when(row.is_dir, |name| {
                        name.text_color(crate::theme::paint(crate::theme::ui().directory))
                    })
                    .when(row.is_root, |name| name.font_weight(FontWeight::SEMIBOLD))
                    .when(row.error.is_some(), |name| {
                        name.text_color(crate::theme::paint(crate::theme::ui().error))
                    })
                    .child(label),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let path = row.path.clone();
                    move |this, _, window, cx| {
                        window.focus(&this.focus_handle, cx);
                        this.selected = Some(path.clone());
                        cx.notify();
                    }
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                window.focus(&this.focus_handle, cx);
                this.selected = Some(clicked.path.clone());
                if clicked.is_dir || event.click_count() >= 2 {
                    this.activate(&clicked, window, cx);
                }
                cx.notify();
            }))
            .when(!row.is_root, |row| {
                row.on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            })
            .drag_over::<DraggedEntry>(|style, _, _, _| {
                style.bg(crate::theme::paint(crate::theme::ui().selected))
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedEntry, window, cx| {
                    this.drop_on(dragged, &target, window, cx);
                }),
            )
            .context_menu(menu)
    }
}

/// Opens the system's terminal in `folder`.
pub fn open_terminal(folder: &Path) -> anyhow::Result<()> {
    use std::process::Command;
    let started = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(folder)
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(folder)
            .spawn()
    } else {
        Command::new("x-terminal-emulator")
            .current_dir(folder)
            .spawn()
    };
    started.map(|_| ()).map_err(|error| {
        anyhow::anyhow!(
            "{}",
            t!(
                "panels.folder-workspace.cannot-open-a-terminal-in",
                folder = folder.display(),
                error = error
            )
        )
    })
}

impl Render for FolderWorkspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows();
        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .p_1()
            .flex_none()
            .child(
                Button::new("folders-unfold-all")
                    .small()
                    .ghost()
                    .label(t!("panels.folder-workspace.unfold"))
                    .tooltip(t!("panels.folder-workspace.unfold-all"))
                    .on_click(cx.listener(|this, _, _, cx| this.set_all(true, cx))),
            )
            .child(
                Button::new("folders-fold-all")
                    .small()
                    .ghost()
                    .label(t!("panels.folder-workspace.fold"))
                    .tooltip(t!("panels.folder-workspace.fold-all"))
                    .on_click(cx.listener(|this, _, _, cx| this.set_all(false, cx))),
            )
            .child(
                Button::new("folders-locate")
                    .small()
                    .ghost()
                    .label(t!("panels.folder-workspace.locate"))
                    .tooltip(t!("panels.folder-workspace.locate-current-file"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let file = this.workspace.upgrade().and_then(|workspace| {
                            let workspace = workspace.read(cx);
                            let view = workspace.active_view(cx)?;
                            view.read(cx).buffer.read(cx).path().map(Path::to_owned)
                        });
                        if let Some(file) = file {
                            this.locate(&file, cx);
                        }
                    })),
            );
        let body = if rows.is_empty() {
            div()
                .p_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child(t!(
                    "panels.folder-workspace.file-open-folder-as-workspace-or-drop-a-folder-here"
                ))
                .into_any_element()
        } else {
            let rows = std::rc::Rc::new(rows);
            let list_rows = rows.clone();
            uniform_list(
                "folder-rows",
                rows.len(),
                cx.processor(move |this, range: Range<usize>, _, cx| {
                    range
                        .map(|index| this.render_row(index, &list_rows[index], cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        div()
            .id("folder-workspace")
            .key_context("FolderWorkspace")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().flex_1().min_h(px(0.)).child(body))
    }
}
