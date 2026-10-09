//! Document List, as Notepad++'s (View > Document List): the open documents with their names
//! and paths. A click shows a document, a middle click closes it (asking about unsaved changes
//! as File > Close does), and a click on a column's heading sorts by it, in reverse on a second
//! click and back to the order of the tabs on a third. Modified documents are marked with `*`;
//! documents of the second view, in split view, with `(2)`.

use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton, SharedString,
    WeakEntity, Window, div, prelude::*, px,
};

use crate::editor::EditorView;
use crate::workspace::Workspace;

/// The columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Name,
    Path,
}

/// One open document's row.
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) view: Entity<EditorView>,
    pub(crate) name: String,
    /// The full path; empty for an untitled document.
    pub(crate) path: String,
    pub(crate) modified: bool,
    pub(crate) second_view: bool,
    pub(crate) active: bool,
}

impl Entry {
    /// The row as text: for tests, and the order of the name column.
    pub(crate) fn label(&self) -> String {
        format!(
            "{}{}{}",
            self.name,
            if self.modified { " *" } else { "" },
            if self.second_view { " (2)" } else { "" }
        )
    }
}

pub(crate) struct DocumentList {
    workspace: WeakEntity<Workspace>,
    /// The column sorted by, and whether in reverse; none for the order of the tabs.
    pub(crate) sort: Option<(Column, bool)>,
    focus_handle: FocusHandle,
}

impl Focusable for DocumentList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentList {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace,
            sort: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The rows, in the order shown.
    pub(crate) fn entries(&self, cx: &App) -> Vec<Entry> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };
        let workspace = workspace.read(cx);
        let active = workspace.active_view(cx);
        let split = workspace.is_split(cx);
        let mut entries: Vec<Entry> = workspace
            .all_views(cx)
            .into_iter()
            .map(|view| {
                let buffer = view.read(cx).buffer.read(cx);
                Entry {
                    name: buffer.display_name(),
                    path: buffer
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    modified: buffer.is_modified(),
                    second_view: split && workspace.pane_of(&view, cx) == Some(1),
                    active: active.as_ref() == Some(&view),
                    view,
                }
            })
            .collect();
        if let Some((column, reverse)) = self.sort {
            let key = |entry: &Entry| match column {
                Column::Name => entry.name.to_lowercase(),
                Column::Path => entry.path.to_lowercase(),
            };
            // Stable: equal keys keep the order of the tabs.
            entries.sort_by(|a, b| {
                let order = key(a).cmp(&key(b));
                if reverse { order.reverse() } else { order }
            });
        }
        entries
    }

    /// A click on a column's heading: by it, in reverse, then the order of the tabs.
    pub(crate) fn sort_by(&mut self, column: Column, cx: &mut Context<Self>) {
        self.sort = match self.sort {
            Some((sorted, false)) if sorted == column => Some((column, true)),
            Some((sorted, true)) if sorted == column => None,
            _ => Some((column, false)),
        };
        cx.notify();
    }

    fn heading(
        &self,
        column: Column,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let arrow = match self.sort {
            Some((sorted, false)) if sorted == column => " ▲",
            Some((sorted, true)) if sorted == column => " ▼",
            _ => "",
        };
        div()
            .id(label)
            .debug_selector(move || format!("document-list-heading-{label}"))
            .flex_1()
            .px_2()
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .child(format!("{label}{arrow}"))
            .on_click(cx.listener(move |this, _, _, cx| this.sort_by(column, cx)))
    }
}

impl Render for DocumentList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries(cx);
        let rows = entries.into_iter().enumerate().map(|(index, entry)| {
            let workspace = self.workspace.clone();
            let shown = entry.view.clone();
            let closed = entry.view.clone();
            let closing = self.workspace.clone();
            let name: SharedString = entry.label().into();
            div()
                .id(("document-list-row", index))
                .debug_selector(move || format!("document-list-{index}"))
                .flex()
                .flex_row()
                .h(px(22.))
                .items_center()
                .whitespace_nowrap()
                .overflow_hidden()
                .cursor_pointer()
                .when(entry.active, |row| {
                    row.bg(crate::theme::paint(crate::theme::ui().selected))
                })
                .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .overflow_hidden()
                        .when(entry.modified, |name| {
                            name.text_color(crate::theme::paint(crate::theme::ui().modified))
                        })
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .overflow_hidden()
                        .text_color(crate::theme::paint(crate::theme::ui().muted))
                        .child(entry.path.clone()),
                )
                .on_click(move |_, window, cx| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.activate_view(&shown, window, cx);
                            let focus = shown.read(cx).focus_handle.clone();
                            window.focus(&focus, cx);
                        })
                        .ok();
                })
                .on_mouse_up(MouseButton::Middle, move |_, window, cx| {
                    closing
                        .update(cx, |workspace, cx| {
                            workspace
                                .close_with_confirmation(vec![closed.clone()], window, cx)
                                .detach();
                        })
                        .ok();
                })
        });
        div()
            .id("document-list")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_none()
                    .h(px(24.))
                    .items_center()
                    .border_b_1()
                    .border_color(crate::theme::paint(crate::theme::ui().border))
                    .child(self.heading(Column::Name, "Name", cx))
                    .child(self.heading(Column::Path, "Path", cx)),
            )
            .child(
                div()
                    .id("document-list-rows")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
