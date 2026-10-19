//! The Window menu, as Notepad++'s: the documents of the active view (a click goes there), Sort
//! By, which orders its tabs, and Windows..., a dialog listing them by name, path, type and size,
//! sorted by any column, where selected documents are activated, saved or closed, and the tabs
//! take the order shown.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::time::SystemTime;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClickEvent, Context, Entity, EntityId, WeakEntity, Window, div,
    prelude::*, px, uniform_list,
};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::file_ops::SaveMode;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("window.sort", |this, args: SortArgs, _, cx| {
        let pane = this.active_pane().clone();
        let mut documents: Vec<Document> = pane
            .read(cx)
            .items()
            .iter()
            .map(|view| Document::of(view, cx))
            .collect();
        sort(&mut documents, args.by, args.descending);
        let order: Vec<_> = documents
            .into_iter()
            .map(|document| document.view)
            .collect();
        pane.update(cx, |pane, cx| pane.reorder(&order, cx));
        Ok(())
    });
    registry.workspace("window.windows", |this, (), window, cx| {
        let active = this.active_view(cx);
        open(cx.entity().downgrade(), active, window, cx);
        Ok(())
    });
}

/// What Sort By and the dialog's columns order by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SortBy {
    Name,
    Path,
    /// The language.
    Type,
    /// The length of the text.
    Size,
    /// When the file was last changed.
    Modified,
}

#[derive(Deserialize)]
struct SortArgs {
    by: SortBy,
    #[serde(default)]
    descending: bool,
}

/// A document of the view, as the menu and the dialog show it.
#[derive(Clone)]
pub(crate) struct Document {
    pub(crate) view: Entity<EditorView>,
    pub(crate) name: String,
    /// The full path; the name for an untitled document.
    pub(crate) path: String,
    /// The language's name.
    pub(crate) kind: String,
    /// The text's length in bytes.
    pub(crate) size: usize,
    /// The file's modification time; none for an untitled document or a file that is gone.
    pub(crate) modified: Option<SystemTime>,
    /// It has unsaved changes.
    pub(crate) changed: bool,
}

impl Document {
    pub(crate) fn of(view: &Entity<EditorView>, cx: &App) -> Self {
        let buffer = view.read(cx).buffer.read(cx);
        let name = buffer.display_name();
        let path = buffer
            .path()
            .map_or_else(|| name.clone(), |path| path.display().to_string());
        let kind = match (buffer.language(), buffer.user_language()) {
            (Some(language), _) => language.name.to_owned(),
            (None, Some(user)) => user.name.clone(),
            (None, None) => t!("window-menu.normal-text"),
        };
        let modified = buffer
            .path()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok());
        Self {
            view: view.clone(),
            name,
            path,
            kind,
            size: buffer.doc().text().len(),
            modified,
            changed: buffer.is_modified(),
        }
    }
}

/// Text in the order people expect: without regard to case, then as written.
fn by_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// Orders `documents` by `by`, keeping the order of equal ones.
pub(crate) fn sort(documents: &mut [Document], by: SortBy, descending: bool) {
    documents.sort_by(|a, b| {
        let order = match by {
            SortBy::Name => by_text(&a.name, &b.name),
            SortBy::Path => by_text(&a.path, &b.path),
            SortBy::Type => by_text(&a.kind, &b.kind),
            SortBy::Size => a.size.cmp(&b.size),
            // Untitled documents, without a time, first.
            SortBy::Modified => a.modified.cmp(&b.modified),
        };
        if descending { order.reverse() } else { order }
    });
}

/// The documents of the active view, for the Window menu: their names, and which is active.
pub(crate) fn menu_documents(workspace: &Workspace, cx: &App) -> (Vec<String>, Option<usize>) {
    let pane = workspace.active_pane().read(cx);
    let names = pane
        .items()
        .iter()
        .map(|view| view.read(cx).buffer.read(cx).display_name())
        .collect();
    let active = pane.active_item().and_then(|active| pane.index_of(active));
    (names, active)
}

/// Window > Windows...
pub(crate) fn open(
    workspace: WeakEntity<Workspace>,
    active: Option<Entity<EditorView>>,
    window: &mut Window,
    cx: &mut App,
) {
    let dialog = cx.new(|_| WindowsDialog::new(workspace, active.as_ref()));
    window.open_dialog(cx, move |modal, _, _| {
        modal
            .title(t!("window-menu.windows"))
            .w(px(720.))
            .child(dialog.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

/// The Windows dialog: the documents of the active view.
pub(crate) struct WindowsDialog {
    workspace: WeakEntity<Workspace>,
    /// The column sorted by, and whether in reverse; none for the order of the tabs.
    pub(crate) sort: Option<(SortBy, bool)>,
    /// The selected documents' views.
    pub(crate) selected: HashSet<EntityId>,
    /// The row a Shift+click selects from.
    anchor: Option<EntityId>,
}

impl WindowsDialog {
    /// The dialog, with the active document `active` selected.
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        active: Option<&Entity<EditorView>>,
    ) -> Self {
        let anchor = active.map(Entity::entity_id);
        Self {
            workspace,
            sort: None,
            selected: anchor.into_iter().collect(),
            anchor,
        }
    }

    /// The rows, in the order shown.
    pub(crate) fn rows(&self, cx: &App) -> Vec<Document> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };
        let mut documents: Vec<Document> = workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .items()
            .iter()
            .map(|view| Document::of(view, cx))
            .collect();
        if let Some((by, descending)) = self.sort {
            sort(&mut documents, by, descending);
        }
        documents
    }

    /// The selected documents, in the order shown.
    pub(crate) fn selection(&self, cx: &App) -> Vec<Entity<EditorView>> {
        self.rows(cx)
            .into_iter()
            .filter(|row| self.selected.contains(&row.view.entity_id()))
            .map(|row| row.view)
            .collect()
    }

    /// A click on a column's heading: sorts by it, in reverse on a second click.
    pub(crate) fn sort_by(&mut self, by: SortBy, cx: &mut Context<Self>) {
        self.sort = match self.sort {
            Some((current, descending)) if current == by => Some((by, !descending)),
            _ => Some((by, false)),
        };
        cx.notify();
    }

    /// A click on a row: selects it alone, adds or removes it (`toggle`, Ctrl), or selects from
    /// the last one clicked (`range`, Shift).
    pub(crate) fn click(&mut self, row: usize, toggle: bool, range: bool, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let Some(clicked) = rows.get(row).map(|row| row.view.entity_id()) else {
            return;
        };
        let anchor = self
            .anchor
            .and_then(|anchor| rows.iter().position(|row| row.view.entity_id() == anchor));
        if range && let Some(anchor) = anchor {
            let (from, to) = (anchor.min(row), anchor.max(row));
            self.selected = rows[from..=to]
                .iter()
                .map(|row| row.view.entity_id())
                .collect();
        } else if toggle {
            if !self.selected.remove(&clicked) {
                self.selected.insert(clicked);
            }
            self.anchor = Some(clicked);
        } else {
            self.selected = HashSet::from([clicked]);
            self.anchor = Some(clicked);
        }
        cx.notify();
    }

    /// Activate: shows the one selected document and closes the dialog.
    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selection(cx);
        let [view] = selection.as_slice() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.activate_view(view, window, cx)
            })
            .ok();
        window.close_dialog(cx);
    }

    /// Save: saves the selected documents.
    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let views = self.selection(cx);
        self.workspace
            .update(cx, |workspace, cx| {
                for view in views {
                    workspace.save(view, SaveMode::Save, window, cx).detach();
                }
            })
            .ok();
        cx.notify();
    }

    /// Close Window(s): closes the selected documents, asking about unsaved changes.
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let views = self.selection(cx);
        let task = self
            .workspace
            .update(cx, |workspace, cx| {
                workspace.close_with_confirmation(views, window, cx)
            })
            .ok();
        let this = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            if let Some(task) = task {
                task.await;
            }
            this.update(cx, |this, cx| {
                // What closed is no longer selected.
                let shown: HashSet<EntityId> = this
                    .rows(cx)
                    .iter()
                    .map(|row| row.view.entity_id())
                    .collect();
                this.selected.retain(|id| shown.contains(id));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Sort Tabs: the tabs take the order shown.
    pub(crate) fn sort_tabs(&mut self, cx: &mut Context<Self>) {
        let order: Vec<_> = self.rows(cx).into_iter().map(|row| row.view).collect();
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = workspace.read(cx).active_pane().clone();
            pane.update(cx, |pane, cx| pane.reorder(&order, cx));
        }
        // The rows were sorted; they are the tabs' order now.
        self.sort = None;
        cx.notify();
    }
}

/// The columns: what each sorts by, its heading and width.
const COLUMNS: [(SortBy, &str, f32); 4] = [
    (SortBy::Name, "window-menu.name", 180.),
    (SortBy::Path, "window-menu.path", 290.),
    (SortBy::Type, "window-menu.type", 110.),
    (SortBy::Size, "window-menu.size", 80.),
];

impl Render for WindowsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let rows = self.rows(cx);
        let selected = self.selected.clone();
        let count = selected.len();
        let headings = COLUMNS.iter().map(|&(by, key, width)| {
            let arrow = match self.sort {
                Some((current, false)) if current == by => " ▲",
                Some((current, true)) if current == by => " ▼",
                _ => "",
            };
            div()
                .id(key)
                .debug_selector(move || format!("windows-heading-{key}"))
                .w(px(width))
                .flex_none()
                .px_2()
                .truncate()
                .hover(|style| style.bg(paint(colors.hovered)))
                .child(format!("{}{arrow}", t!(key)))
                .on_click(cx.listener(move |this, _, _, cx| this.sort_by(by, cx)))
        });
        let this = cx.entity().downgrade();
        let list = uniform_list("windows-rows", rows.len(), move |range, _, _| {
            range
                .map(|index| {
                    let row = &rows[index];
                    let this = this.clone();
                    let cells = [
                        format!("{}{}", row.name, if row.changed { " *" } else { "" }),
                        row.path.clone(),
                        row.kind.clone(),
                        row.size.to_string(),
                    ];
                    div()
                        .id(("windows-row", index))
                        .debug_selector(move || format!("windows-row-{index}"))
                        .flex()
                        .flex_row()
                        .h(px(22.))
                        .items_center()
                        .when(selected.contains(&row.view.entity_id()), |row| {
                            row.bg(paint(colors.selected))
                        })
                        .hover(|row| row.bg(paint(colors.hovered)))
                        .children(cells.into_iter().zip(COLUMNS).map(|(text, (_, _, width))| {
                            div().w(px(width)).flex_none().px_2().truncate().child(text)
                        }))
                        .on_click(move |event: &ClickEvent, window, cx| {
                            let modifiers = event.modifiers();
                            this.update(cx, |this, cx| {
                                this.click(index, modifiers.secondary(), modifiers.shift, cx);
                                if event.click_count() == 2 {
                                    this.activate(window, cx);
                                }
                            })
                            .ok();
                        })
                })
                .collect()
        })
        .h(px(300.));
        let button = |id: &'static str, label: String| Button::new(id).label(label);
        div()
            .flex()
            .flex_row()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .border_1()
                    .border_color(paint(colors.border))
                    .text_sm()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .h(px(24.))
                            .items_center()
                            .border_b_1()
                            .border_color(paint(colors.border))
                            .bg(paint(colors.surface))
                            .children(headings),
                    )
                    .child(list),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w(px(130.))
                    .child(
                        button("windows-activate", t!("window-menu.activate"))
                            .primary()
                            .disabled(count != 1)
                            .on_click(cx.listener(|this, _, window, cx| this.activate(window, cx))),
                    )
                    .child(
                        button("windows-save", t!("window-menu.save"))
                            .disabled(count == 0)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        button("windows-close", t!("window-menu.close-windows"))
                            .disabled(count == 0)
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    )
                    .child(
                        button("windows-sort-tabs", t!("window-menu.sort-tabs"))
                            .disabled(self.sort.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.sort_tabs(cx))),
                    ),
            )
    }
}

#[cfg(test)]
#[path = "../tests/unit/window_menu/tests.rs"]
mod tests;
