//! The plugin panel: lines a plugin or a script shows (ADR 0036). A click on a line tells the
//! plugin that filled it.

use gpui_kit::{
    App, Context, FocusHandle, Focusable, FontWeight, SharedString, WeakEntity, Window, div,
    prelude::*, px,
};

use crate::extensions::PanelOwner;
use crate::workspace::{Workspace, report_error};

pub(crate) struct PluginPanel {
    workspace: WeakEntity<Workspace>,
    pub(crate) owner: Option<PanelOwner>,
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    focus_handle: FocusHandle,
}

impl Focusable for PluginPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PluginPanel {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace,
            owner: None,
            title: String::new(),
            lines: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub(crate) fn fill(
        &mut self,
        owner: PanelOwner,
        title: String,
        lines: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.owner = Some(owner);
        self.title = title;
        self.lines = lines;
        cx.notify();
    }
}

impl Render for PluginPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.lines.iter().enumerate().map(|(index, line)| {
            let workspace = self.workspace.clone();
            let owner = self.owner.clone();
            let text: SharedString = line.clone().into();
            div()
                .id(("plugin-panel-row", index))
                .debug_selector(move || format!("plugin-panel-{index}"))
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .overflow_hidden()
                .cursor_pointer()
                .hover(|row| row.bg(crate::theme::paint(crate::theme::ui().hovered)))
                .child(text)
                .on_click(move |_, window, cx| {
                    let Some(owner) = owner.clone() else {
                        return;
                    };
                    workspace
                        .update(cx, |workspace, cx| {
                            if let Err(error) =
                                workspace.plugin_panel_clicked(&owner, index, window, cx)
                            {
                                report_error(&error, window, cx);
                            }
                        })
                        .ok();
                })
        });
        div()
            .id("plugin-panel")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .when(!self.title.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex_none()
                        .h(px(24.))
                        .px_2()
                        .flex()
                        .items_center()
                        .font_weight(FontWeight::SEMIBOLD)
                        .border_b_1()
                        .border_color(crate::theme::paint(crate::theme::ui().border))
                        .child(self.title.clone()),
                )
            })
            .child(
                div()
                    .id("plugin-panel-rows")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
