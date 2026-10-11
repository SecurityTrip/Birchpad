//! Project Panels 1 to 3, as Notepad++'s: each shows one project workspace, a `.workspace`
//! file in Notepad++'s format (`birchpad_config::ProjectWorkspace`), as a tree of projects,
//! their folders and files.
//!
//! The right-click menu builds the tree: on the workspace, New Workspace, Open Workspace,
//! Reload, Save As, Save a Copy As and Add New Project; on a project or folder, Rename, Add
//! Folder, Add Files, Add Files from Directory, Remove, Move Up and Move Down; on a file, Open,
//! Modify File Path, Remove and the moves. A double-click or Enter opens a file. Files that do
//! not exist are shown greyed.
//!
//! Unlike Notepad++, which asks to save a changed workspace, every change is saved to the
//! workspace file at once; New Workspace asks where to keep it first. The file each panel shows
//! is remembered in `state.toml`.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

use birchpad_config::{ProjectFolder, ProjectItem, ProjectWorkspace};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, AsyncWindowContext, ClickEvent, Context, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    MouseButton, PathPromptOptions, UniformListScrollHandle, WeakEntity, Window, div, prelude::*,
    px, rgb, uniform_list,
};

use crate::app_state::AppState;
use crate::workspace::Workspace;

const ROW_HEIGHT: f32 = 22.;
const INDENT: f32 = 14.;

/// What a menu item does to the panel.
type MenuAction = Box<dyn Fn(&mut ProjectPanel, &mut Window, &mut Context<ProjectPanel>)>;

/// Where an item is: the project's index, then the index in each folder on the way down.
pub(crate) type Address = Vec<usize>;

/// The project or folder at `address`.
pub(crate) fn folder_mut<'a>(
    model: &'a mut ProjectWorkspace,
    address: &[usize],
) -> Option<&'a mut ProjectFolder> {
    let (&project, rest) = address.split_first()?;
    let mut folder = model.projects.get_mut(project)?;
    for &index in rest {
        folder = match folder.items.get_mut(index)? {
            ProjectItem::Folder(inner) => inner,
            ProjectItem::File(_) => return None,
        };
    }
    Some(folder)
}

/// The items holding `address`'s item, and its index there; for a project, `None`.
fn parent_items<'a>(
    model: &'a mut ProjectWorkspace,
    address: &[usize],
) -> Option<(&'a mut Vec<ProjectItem>, usize)> {
    let (&last, parent) = address.split_last()?;
    if parent.is_empty() {
        return None;
    }
    Some((&mut folder_mut(model, parent)?.items, last))
}

/// Adds a folder to the project or folder at `at`; returns where it went.
pub(crate) fn add_folder(
    model: &mut ProjectWorkspace,
    at: &[usize],
    name: &str,
) -> Option<Address> {
    let folder = folder_mut(model, at)?;
    folder.items.push(ProjectItem::Folder(ProjectFolder {
        name: name.to_owned(),
        items: Vec::new(),
    }));
    let mut address = at.to_vec();
    address.push(folder.items.len() - 1);
    Some(address)
}

/// Adds `files` to the project or folder at `at`, leaving out those already there.
pub(crate) fn add_files(model: &mut ProjectWorkspace, at: &[usize], files: Vec<PathBuf>) -> bool {
    let Some(folder) = folder_mut(model, at) else {
        return false;
    };
    for file in files {
        if !folder.items.contains(&ProjectItem::File(file.clone())) {
            folder.items.push(ProjectItem::File(file));
        }
    }
    true
}

/// The items for Add Files from Directory: the files of `dir`, and a folder for each of its
/// folders with their files, in name order. Hidden entries (a leading dot) are left out.
pub(crate) fn items_from_directory(dir: &Path) -> Vec<ProjectItem> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(String, PathBuf, bool)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().ok()?.is_dir();
            (!name.starts_with('.')).then(|| (name, entry.path(), is_dir))
        })
        .collect();
    entries.sort_by_key(|(name, _, _)| name.to_lowercase());
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for (name, path, is_dir) in entries {
        if is_dir {
            folders.push(ProjectItem::Folder(ProjectFolder {
                name,
                items: items_from_directory(&path),
            }));
        } else {
            files.push(ProjectItem::File(path));
        }
    }
    folders.extend(files);
    folders
}

/// Renames the project or folder at `address`.
pub(crate) fn rename(model: &mut ProjectWorkspace, address: &[usize], name: &str) -> bool {
    match folder_mut(model, address) {
        Some(folder) => {
            folder.name = name.to_owned();
            true
        }
        None => false,
    }
}

/// Points the file at `address` somewhere else (Modify File Path).
pub(crate) fn set_file_path(
    model: &mut ProjectWorkspace,
    address: &[usize],
    path: PathBuf,
) -> bool {
    match parent_items(model, address) {
        Some((items, index)) => match items.get_mut(index) {
            Some(ProjectItem::File(file)) => {
                *file = path;
                true
            }
            _ => false,
        },
        None => false,
    }
}

/// Removes the project, folder or file at `address`.
pub(crate) fn remove(model: &mut ProjectWorkspace, address: &[usize]) -> bool {
    if let [project] = address {
        if *project < model.projects.len() {
            model.projects.remove(*project);
            return true;
        }
        return false;
    }
    match parent_items(model, address) {
        Some((items, index)) if index < items.len() => {
            items.remove(index);
            true
        }
        _ => false,
    }
}

/// Moves the item at `address` up or down among its siblings; returns where it went.
pub(crate) fn move_item(
    model: &mut ProjectWorkspace,
    address: &[usize],
    up: bool,
) -> Option<Address> {
    fn swap<T>(items: &mut [T], index: usize, up: bool) -> Option<usize> {
        let other = if up { index.checked_sub(1)? } else { index + 1 };
        if other >= items.len() || index >= items.len() {
            return None;
        }
        items.swap(index, other);
        Some(other)
    }
    let moved = if let [project] = address {
        swap(&mut model.projects, *project, up)?
    } else {
        let (items, index) = parent_items(model, address)?;
        swap(items, index, up)?
    };
    let mut address = address.to_vec();
    *address.last_mut()? = moved;
    Some(address)
}

/// What a row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowKind {
    /// The workspace itself, at the top.
    Workspace,
    Project,
    Folder,
    File {
        path: PathBuf,
        exists: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// `None` for the workspace row.
    pub(crate) address: Option<Address>,
    pub(crate) kind: RowKind,
    pub(crate) name: String,
    pub(crate) depth: usize,
    pub(crate) expanded: bool,
}

pub(crate) struct ProjectPanel {
    /// 1 to 3.
    panel: u8,
    workspace: WeakEntity<Workspace>,
    /// The workspace file shown; none until one is opened or made.
    pub(crate) file: Option<PathBuf>,
    pub(crate) model: ProjectWorkspace,
    /// Why the workspace file could not be read or written.
    pub(crate) error: Option<String>,
    collapsed: HashSet<Address>,
    /// The selected row's address; `Some(None)` for the workspace row.
    pub(crate) selected: Option<Option<Address>>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ProjectPanel {
    pub(crate) fn new(
        panel: u8,
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            panel,
            workspace,
            file: None,
            model: ProjectWorkspace::default(),
            error: None,
            collapsed: HashSet::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        };
        let remembered = AppState::global(cx)
            .state
            .panels
            .projects
            .get(&panel.to_string())
            .cloned();
        if let Some(file) = remembered {
            this.open_workspace(file, cx);
        }
        this
    }

    fn remember(&self, cx: &mut Context<Self>) {
        let (key, file) = (self.panel.to_string(), self.file.clone());
        AppState::update_state(cx, |state, _| match file {
            Some(file) => {
                state.panels.projects.insert(key, file);
            }
            None => {
                state.panels.projects.remove(&key);
            }
        });
    }

    /// Shows the workspace in `file`; a file that cannot be read leaves an empty workspace and
    /// says why.
    pub(crate) fn open_workspace(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        match ProjectWorkspace::load(&file) {
            Ok(model) => {
                self.model = model;
                self.error = None;
            }
            Err(error) => {
                self.model = ProjectWorkspace::default();
                self.error = Some(error.to_string());
            }
        }
        self.file = Some(file);
        self.collapsed.clear();
        self.selected = Some(None);
        self.remember(cx);
        cx.notify();
    }

    /// New Workspace: an empty workspace kept in `file`.
    pub(crate) fn new_workspace(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        self.model = ProjectWorkspace::default();
        self.file = Some(file);
        self.collapsed.clear();
        self.selected = Some(None);
        self.save(cx);
        self.remember(cx);
    }

    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = self.file.clone() {
            self.open_workspace(file, cx);
        }
    }

    /// Saves to the workspace file; a failure is shown in the panel.
    fn save(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = &self.file {
            self.error = self.model.save(file).err().map(|error| error.to_string());
        }
        cx.notify();
    }

    /// Save As (`switch`) or Save a Copy As.
    pub(crate) fn save_as(&mut self, file: PathBuf, switch: bool, cx: &mut Context<Self>) {
        match self.model.save(&file) {
            Ok(()) if switch => {
                self.file = Some(file);
                self.error = None;
                self.remember(cx);
            }
            Ok(()) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Applies a change to the tree and saves it.
    pub(crate) fn change(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ProjectWorkspace) -> Option<Option<Address>>,
    ) {
        if let Some(selected) = change(&mut self.model) {
            self.selected = Some(selected);
            self.save(cx);
        }
    }

    pub(crate) fn add_project(&mut self, name: &str, cx: &mut Context<Self>) {
        self.change(cx, |model| {
            model.projects.push(ProjectFolder {
                name: name.to_owned(),
                items: Vec::new(),
            });
            Some(Some(vec![model.projects.len() - 1]))
        });
    }

    /// The rows shown, top to bottom.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let Some(file) = &self.file else {
            return Vec::new();
        };
        let mut rows = vec![Row {
            address: None,
            kind: RowKind::Workspace,
            name: file.file_name().map_or_else(
                || file.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            depth: 0,
            expanded: true,
        }];
        for (index, project) in self.model.projects.iter().enumerate() {
            self.push_folder(project, vec![index], 1, RowKind::Project, &mut rows);
        }
        rows
    }

    fn push_folder(
        &self,
        folder: &ProjectFolder,
        address: Address,
        depth: usize,
        kind: RowKind,
        rows: &mut Vec<Row>,
    ) {
        let expanded = !self.collapsed.contains(&address);
        rows.push(Row {
            address: Some(address.clone()),
            kind,
            name: folder.name.clone(),
            depth,
            expanded,
        });
        if !expanded {
            return;
        }
        for (index, item) in folder.items.iter().enumerate() {
            let mut inner = address.clone();
            inner.push(index);
            match item {
                ProjectItem::Folder(child) => {
                    self.push_folder(child, inner, depth + 1, RowKind::Folder, rows)
                }
                ProjectItem::File(path) => rows.push(Row {
                    address: Some(inner),
                    kind: RowKind::File {
                        path: path.clone(),
                        exists: path.is_file(),
                    },
                    name: path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    ),
                    depth: depth + 1,
                    expanded: false,
                }),
            }
        }
    }

    fn toggle(&mut self, address: &Address, cx: &mut Context<Self>) {
        if !self.collapsed.remove(address) {
            self.collapsed.insert(address.clone());
        }
        cx.notify();
    }

    /// Opens a file, or folds or unfolds a project or folder.
    fn activate(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        match (&row.kind, &row.address) {
            (RowKind::File { path, .. }, _) => {
                let path = path.clone();
                self.workspace
                    .update(cx, |workspace, cx| workspace.open_path(&path, window, cx))
                    .ok();
            }
            (RowKind::Project | RowKind::Folder, Some(address)) => {
                let address = address.clone();
                self.toggle(&address, cx);
            }
            _ => {}
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let rows = self.rows();
        let index = self
            .selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.address == selected));
        match event.keystroke.key.as_str() {
            "down" if !rows.is_empty() => {
                let next = index.map_or(0, |i| (i + 1).min(rows.len() - 1));
                self.selected = Some(rows[next].address.clone());
            }
            "up" if !rows.is_empty() => {
                let previous = index.map_or(0, |i| i.saturating_sub(1));
                self.selected = Some(rows[previous].address.clone());
            }
            "enter" => {
                if let Some(row) = index.map(|i| rows[i].clone()) {
                    self.activate(&row, window, cx);
                }
            }
            "delete" => {
                if let Some(Some(address)) = self.selected.clone() {
                    self.change(cx, |model| remove(model, &address).then_some(None));
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Asks for a name, then applies `then` with it.
    fn ask_name(
        &self,
        title: &'static str,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let Some(name) = crate::path_dialog::ask_text(title, initial, cx).await else {
                return;
            };
            this.update(cx, |this, cx| then(this, name, cx)).ok();
        })
        .detach();
    }

    /// Asks for files to open; for a folder, `folders`.
    fn ask_paths(
        &self,
        folders: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Vec<PathBuf>, &mut Context<Self>) + 'static,
    ) {
        cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
            let asked = cx.update(|_, cx| {
                cx.prompt_for_paths(PathPromptOptions {
                    files: !folders,
                    directories: folders,
                    multiple: !folders,
                    prompt: None,
                })
            });
            let Ok(asked) = asked else {
                return;
            };
            let Ok(Ok(Some(paths))) = asked.await else {
                return;
            };
            this.update(cx, |this, cx| then(this, paths, cx)).ok();
        })
        .detach();
    }

    /// Asks where to keep a workspace file.
    fn ask_save_path(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, PathBuf, &mut Context<Self>) + 'static,
    ) {
        let directory = self
            .file
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_owned)
            .unwrap_or_else(|| PathBuf::from("."));
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) =
                crate::path_dialog::ask_save(directory, "project.workspace".into(), cx).await
            else {
                return;
            };
            this.update(cx, |this, cx| then(this, path, cx)).ok();
        })
        .detach();
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
            // A menu item that runs `run` on the panel.
            let item = |label: &'static str, run: MenuAction| {
                let this = this.clone();
                PopupMenuItem::new(label).on_click(move |_: &ClickEvent, window, cx| {
                    this.update(cx, |panel, cx| run(panel, window, cx)).ok();
                })
            };
            let address = row.address.clone().unwrap_or_default();
            let moves = |menu: PopupMenu| {
                let (up, down) = (address.clone(), address.clone());
                menu.separator()
                    .item(item(
                        "Move Up",
                        Box::new(move |panel, _, cx| {
                            let up = up.clone();
                            panel.change(cx, |model| move_item(model, &up, true).map(Some));
                        }),
                    ))
                    .item(item(
                        "Move Down",
                        Box::new(move |panel, _, cx| {
                            let down = down.clone();
                            panel.change(cx, |model| move_item(model, &down, false).map(Some));
                        }),
                    ))
            };
            let removing = address.clone();
            let remove_item = item(
                "Remove",
                Box::new(move |panel, _, cx| {
                    let at = removing.clone();
                    panel.change(cx, |model| remove(model, &at).then_some(None));
                }),
            );
            match &row.kind {
                RowKind::Workspace => menu
                    .item(item(
                        "New Workspace...",
                        Box::new(|panel, window, cx| {
                            panel.ask_save_path(window, cx, |panel, path, cx| {
                                panel.new_workspace(path, cx)
                            });
                        }),
                    ))
                    .item(item(
                        "Open Workspace...",
                        Box::new(|panel, window, cx| {
                            panel.ask_paths(false, window, cx, |panel, paths, cx| {
                                if let Some(path) = paths.into_iter().next() {
                                    panel.open_workspace(path, cx);
                                }
                            });
                        }),
                    ))
                    .item(item(
                        "Reload Workspace",
                        Box::new(|panel, _, cx| panel.reload(cx)),
                    ))
                    .separator()
                    .item(item(
                        "Save As...",
                        Box::new(|panel, window, cx| {
                            panel.ask_save_path(window, cx, |panel, path, cx| {
                                panel.save_as(path, true, cx)
                            });
                        }),
                    ))
                    .item(item(
                        "Save a Copy As...",
                        Box::new(|panel, window, cx| {
                            panel.ask_save_path(window, cx, |panel, path, cx| {
                                panel.save_as(path, false, cx)
                            });
                        }),
                    ))
                    .separator()
                    .item(item(
                        "Add New Project...",
                        Box::new(|panel, window, cx| {
                            panel.ask_name(
                                "Add New Project",
                                "Project Name".into(),
                                window,
                                cx,
                                |panel, name, cx| {
                                    panel.add_project(&name, cx);
                                },
                            );
                        }),
                    )),
                RowKind::Project | RowKind::Folder => {
                    let (renaming, adding, files, directory) = (
                        address.clone(),
                        address.clone(),
                        address.clone(),
                        address.clone(),
                    );
                    let name = row.name.clone();
                    let menu = menu
                        .item(item(
                            "Rename...",
                            Box::new(move |panel, window, cx| {
                                let at = renaming.clone();
                                panel.ask_name(
                                    "Rename",
                                    name.clone(),
                                    window,
                                    cx,
                                    move |panel, name, cx| {
                                        panel.change(cx, |model| {
                                            rename(model, &at, &name).then_some(Some(at.clone()))
                                        });
                                    },
                                );
                            }),
                        ))
                        .separator()
                        .item(item(
                            "Add Folder...",
                            Box::new(move |panel, window, cx| {
                                let at = adding.clone();
                                panel.ask_name(
                                    "Add Folder",
                                    "Folder Name".into(),
                                    window,
                                    cx,
                                    move |panel, name, cx| {
                                        panel.change(cx, |model| {
                                            add_folder(model, &at, &name).map(Some)
                                        });
                                    },
                                );
                            }),
                        ))
                        .item(item(
                            "Add Files...",
                            Box::new(move |panel, window, cx| {
                                let at = files.clone();
                                panel.ask_paths(false, window, cx, move |panel, paths, cx| {
                                    panel.change(cx, |model| {
                                        add_files(model, &at, paths).then_some(Some(at.clone()))
                                    });
                                });
                            }),
                        ))
                        .item(item(
                            "Add Files from Directory...",
                            Box::new(move |panel, window, cx| {
                                let at = directory.clone();
                                panel.ask_paths(true, window, cx, move |panel, paths, cx| {
                                    let Some(dir) = paths.into_iter().next() else {
                                        return;
                                    };
                                    panel.change(cx, |model| {
                                        let folder = folder_mut(model, &at)?;
                                        folder.items.extend(items_from_directory(&dir));
                                        Some(Some(at.clone()))
                                    });
                                });
                            }),
                        ))
                        .separator()
                        .item(remove_item);
                    moves(menu)
                }
                RowKind::File { path, .. } => {
                    let (opened, modified) = (path.clone(), address.clone());
                    let workspace = workspace.clone();
                    let menu = menu
                        .item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
                            workspace
                                .update(cx, |workspace, cx| {
                                    workspace.open_path(&opened, window, cx)
                                })
                                .ok();
                        }))
                        .item(item(
                            "Modify File Path...",
                            Box::new(move |panel, window, cx| {
                                let at = modified.clone();
                                panel.ask_paths(false, window, cx, move |panel, paths, cx| {
                                    let Some(path) = paths.into_iter().next() else {
                                        return;
                                    };
                                    panel.change(cx, |model| {
                                        set_file_path(model, &at, path).then_some(Some(at.clone()))
                                    });
                                });
                            }),
                        ))
                        .separator()
                        .item(remove_item);
                    moves(menu)
                }
            }
        }
    }

    fn render_row(
        &self,
        index: usize,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.selected.as_ref() == Some(&row.address);
        let arrow = match row.kind {
            RowKind::Project | RowKind::Folder if row.expanded => "▾",
            RowKind::Project | RowKind::Folder => "▸",
            _ => " ",
        };
        let clicked = row.clone();
        let right_clicked = row.address.clone();
        let menu = self.context_menu(row, cx);
        let (color, weight) = match &row.kind {
            RowKind::Workspace => (0x1f2328, FontWeight::BOLD),
            RowKind::Project => (0x1f2328, FontWeight::SEMIBOLD),
            RowKind::Folder => (0x9a6700, FontWeight::NORMAL),
            RowKind::File { exists: true, .. } => (0x1f2328, FontWeight::NORMAL),
            RowKind::File { exists: false, .. } => (0x8c959f, FontWeight::NORMAL),
        };
        div()
            .id(("project-row", index))
            .w_full()
            .debug_selector(move || format!("project-row-{index}"))
            .h(px(ROW_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl(px(4. + row.depth as f32 * INDENT))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .when(selected, |div| {
                div.bg(crate::theme::paint(crate::theme::ui().selected))
            })
            .hover(|div| div.bg(crate::theme::paint(crate::theme::ui().hovered)))
            .child(div().w(px(INDENT)).flex_none().child(arrow))
            .child(
                div()
                    .text_color(rgb(color))
                    .font_weight(weight)
                    .child(row.name.clone()),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.selected = Some(right_clicked.clone());
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                window.focus(&this.focus_handle, cx);
                this.selected = Some(clicked.address.clone());
                let folds = matches!(clicked.kind, RowKind::Project | RowKind::Folder);
                if folds || event.click_count() >= 2 {
                    this.activate(&clicked, window, cx);
                }
                cx.notify();
            }))
            .context_menu(menu)
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows();
        let body = if self.file.is_none() {
            div()
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child("No workspace")
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(
                            Button::new("project-new")
                                .small()
                                .label("New Workspace...")
                                .debug_selector(|| "project-new".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.ask_save_path(window, cx, |panel, path, cx| {
                                        panel.new_workspace(path, cx)
                                    });
                                })),
                        )
                        .child(
                            Button::new("project-open")
                                .small()
                                .ghost()
                                .label("Open Workspace...")
                                .debug_selector(|| "project-open".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.ask_paths(false, window, cx, |panel, paths, cx| {
                                        if let Some(path) = paths.into_iter().next() {
                                            panel.open_workspace(path, cx);
                                        }
                                    });
                                })),
                        ),
                )
                .into_any_element()
        } else {
            let rows = std::rc::Rc::new(rows);
            let list_rows = rows.clone();
            uniform_list(
                "project-rows",
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
            .id(("project-panel", usize::from(self.panel)))
            .key_context("ProjectPanel")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .children(self.error.clone().map(|error| {
                div()
                    .p_1()
                    .flex_none()
                    .text_color(crate::theme::paint(crate::theme::ui().error))
                    .child(error)
            }))
            .child(div().flex_1().min_h(px(0.)).child(body))
    }
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
