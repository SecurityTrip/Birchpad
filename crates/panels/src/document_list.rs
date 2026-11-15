//! Document List, as Notepad++'s (View > Document List): the open documents with their names
//! and paths. A click shows a document, a middle click closes it (asking about unsaved changes
//! as File > Close does), and a click on a column's heading sorts by it, in reverse on a second
//! click and back to the order of the tabs on a third. Modified documents are marked with `*`;
//! documents of the second view, in split view, with `(2)`.

use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton, SharedString, Window,
    div, prelude::*, px,
};
use std::rc::Rc;

use crate::Host;
use birchpad_editor::EditorView;
use birchpad_widgets::ListRow as _;

/// The columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Path,
}

/// One open document's row.
#[derive(Clone)]
pub struct Entry {
    pub view: Entity<EditorView>,
    pub name: String,
    /// The full path; empty for an untitled document.
    pub path: String,
    pub modified: bool,
    pub second_view: bool,
    pub active: bool,
}

impl Entry {
    /// The row as text: for tests, and the order of the name column.
    pub fn label(&self) -> String {
        format!(
            "{}{}{}",
            self.name,
            if self.modified { " *" } else { "" },
            if self.second_view { " (2)" } else { "" }
        )
    }
}

pub struct DocumentList {
    host: Rc<dyn Host>,
    /// The column sorted by, and whether in reverse; none for the order of the tabs.
    pub sort: Option<(Column, bool)>,
    focus_handle: FocusHandle,
}

impl Focusable for DocumentList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentList {
    pub fn new(host: Rc<dyn Host>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            host,
            sort: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The rows, in the order shown.
    pub fn entries(&self, cx: &App) -> Vec<Entry> {
        let active = self.host.active_view(cx);
        let mut entries: Vec<Entry> = self
            .host
            .views(cx)
            .into_iter()
            .map(|(view, second_view)| {
                let buffer = view.read(cx).buffer.read(cx);
                Entry {
                    name: buffer.display_name(),
                    path: buffer
                        .path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                    modified: buffer.is_modified(),
                    second_view,
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
    pub fn sort_by(&mut self, column: Column, cx: &mut Context<Self>) {
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
            .child(format!(
                "{}{arrow}",
                birchpad_i18n::get_or(
                    &format!("panels.document-list.{}", birchpad_i18n::slug(label)),
                    label
                )
            ))
            .on_click(cx.listener(move |this, _, _, cx| this.sort_by(column, cx)))
    }
}

impl Render for DocumentList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries(cx);
        let rows = entries.into_iter().enumerate().map(|(index, entry)| {
            let host = self.host.clone();
            let shown = entry.view.clone();
            let closed = entry.view.clone();
            let closing = self.host.clone();
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
                .list_row(entry.active)
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .overflow_hidden()
                        .when(entry.modified, |name| {
                            name.text_color(birchpad_widgets::theme::paint(
                                birchpad_widgets::theme::ui().modified,
                            ))
                        })
                        .child(name),
                )
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .overflow_hidden()
                        .text_color(birchpad_widgets::theme::paint(
                            birchpad_widgets::theme::ui().muted,
                        ))
                        .child(entry.path.clone()),
                )
                .on_click(move |_, window, cx| {
                    host.activate(&shown, window, cx);
                    let focus = shown.read(cx).focus_handle.clone();
                    window.focus(&focus, cx);
                })
                .on_mouse_up(MouseButton::Middle, move |_, window, cx| {
                    closing.close(&closed, window, cx);
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
                    .border_color(birchpad_widgets::theme::paint(
                        birchpad_widgets::theme::ui().border,
                    ))
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
