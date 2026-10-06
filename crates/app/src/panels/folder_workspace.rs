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

use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, ClickEvent, ClipboardItem, Context, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    MouseButton, ScrollStrategy, Task, UniformListScrollHandle, WeakEntity, Window, div,
    prelude::*, px, rgb, uniform_list,
};

use crate::app_state::AppState;
use crate::workspace::Workspace;

const ROW_HEIGHT: f32 = 22.;
const INDENT: f32 = 14.;
/// How often unfolded folders are read again.
const POLL: Duration = Duration::from_secs(2);

/// A file or folder in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) is_dir: bool,
}

/// The contents of `folder`: folders first, then files, each in name order without regard to
/// case. Entries whose kind cannot be read are left out.
pub(crate) fn list_folder(folder: &Path) -> io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(folder)?
        .flatten()
        .filter_map(|entry| {
            let kind = entry.file_type().ok()?;
            // A link to a folder is a folder; to a file, a file.
            let is_dir = kind.is_dir()
                || (kind.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir()));
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
struct Node {
    path: PathBuf,
    name: String,
    is_dir: bool,
    expanded: bool,
    /// `None` until the folder has been read.
    children: Option<Vec<Node>>,
    /// Why the folder could not be read.
    error: Option<String>,
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

    fn root(path: PathBuf) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Self::new(path, name, true)
    }

    /// Takes a new listing, keeping what is known of the folders that are still there.
    fn set_listing(&mut self, listing: io::Result<Vec<Entry>>) {
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

    fn find_mut(&mut self, path: &Path) -> Option<&mut Node> {
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
pub(crate) struct Row {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) depth: usize,
    pub(crate) is_dir: bool,
    pub(crate) expanded: bool,
    pub(crate) is_root: bool,
    /// A folder that could not be read says why.
    pub(crate) error: Option<String>,
}

fn push_rows(node: &Node, depth: usize, is_root: bool, rows: &mut Vec<Row>) {
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

pub(crate) struct FolderWorkspace {
    workspace: WeakEntity<Workspace>,
    roots: Vec<Node>,
    pub(crate) selected: Option<PathBuf>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    _poll: Task<()>,
}

impl Focusable for FolderWorkspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FolderWorkspace {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            _poll: poll,
        }
    }

    /// The top folders.
    pub(crate) fn roots(&self) -> Vec<PathBuf> {
        self.roots.iter().map(|root| root.path.clone()).collect()
    }

    fn remember(&self, cx: &mut Context<Self>) {
        let folders = self.roots();
        AppState::update_state(cx, |state, _| state.panels.folders = folders);
    }

    /// Adds `folder` to the workspace, unfolded and selected; a folder already there is only
    /// selected.
    pub(crate) fn add_root(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let folder = std::path::absolute(&folder).unwrap_or(folder);
        if !self.roots.iter().any(|root| root.path == folder) {
            self.roots.push(Node::root(folder.clone()));
            self.remember(cx);
        }
        self.selected = Some(folder.clone());
        self.expand(&folder, cx);
    }

    pub(crate) fn remove_root(&mut self, folder: &Path, cx: &mut Context<Self>) {
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

    pub(crate) fn remove_all(&mut self, cx: &mut Context<Self>) {
        self.roots.clear();
        self.selected = None;
        self.remember(cx);
        cx.notify();
    }

    fn node_mut(&mut self, path: &Path) -> Option<&mut Node> {
        self.roots.iter_mut().find_map(|root| root.find_mut(path))
    }

    /// Unfolds the folder at `path`, reading it first if it has not been read.
    pub(crate) fn expand(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(node) = self.node_mut(path) else {
            return;
        };
        if !node.is_dir {
            return;
        }
        node.expanded = true;
        if node.children.is_none() {
            node.set_listing(list_folder(path));
        }
        cx.notify();
    }

    pub(crate) fn collapse(&mut self, path: &Path, cx: &mut Context<Self>) {
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
    pub(crate) fn set_all(&mut self, expanded: bool, cx: &mut Context<Self>) {
        for root in &mut self.roots {
            root.set_expanded_all(expanded);
        }
        cx.notify();
    }

    /// Reads the unfolded folders again, in the background.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let mut folders = Vec::new();
        for root in &self.roots {
            root.open_folders(&mut folders);
        }
        if folders.is_empty() {
            return;
        }
        let listing = cx.background_spawn(async move {
            folders
                .into_iter()
                .map(|folder| {
                    let listing = list_folder(&folder);
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
    pub(crate) fn locate(&mut self, file: &Path, cx: &mut Context<Self>) -> bool {
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
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for root in &self.roots {
            push_rows(root, 0, true, &mut rows);
        }
        rows
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
            let (shown, terminal) = (folder.clone(), folder.clone());
            menu = menu
                .separator()
                .item(
                    PopupMenuItem::new("Explorer Here")
                        .on_click(move |_, _, cx| cx.reveal_path(&shown)),
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
            .when(selected, |div| div.bg(rgb(0xddf4ff)))
            .hover(|div| div.bg(rgb(0xf3f4f6)))
            .child(div().w(px(INDENT)).flex_none().child(arrow))
            .child(
                div()
                    .when(row.is_dir, |name| name.text_color(rgb(0x9a6700)))
                    .when(row.is_root, |name| name.font_weight(FontWeight::SEMIBOLD))
                    .when(row.error.is_some(), |name| name.text_color(rgb(0xcf222e)))
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
            .context_menu(menu)
    }
}

/// Opens the system's terminal in `folder`.
fn open_terminal(folder: &Path) -> anyhow::Result<()> {
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
    started
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("cannot open a terminal in {}: {error}", folder.display()))
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
                    .label("Unfold")
                    .tooltip("Unfold All")
                    .on_click(cx.listener(|this, _, _, cx| this.set_all(true, cx))),
            )
            .child(
                Button::new("folders-fold-all")
                    .small()
                    .ghost()
                    .label("Fold")
                    .tooltip("Fold All")
                    .on_click(cx.listener(|this, _, _, cx| this.set_all(false, cx))),
            )
            .child(
                Button::new("folders-locate")
                    .small()
                    .ghost()
                    .label("Locate")
                    .tooltip("Locate Current File")
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
                .text_color(rgb(0x57606a))
                .child("File > Open Folder as Workspace..., or drop a folder here")
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

#[cfg(test)]
#[path = "folder_workspace_tests.rs"]
mod tests;
