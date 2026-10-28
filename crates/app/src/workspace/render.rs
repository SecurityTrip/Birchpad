//! Drawing the window: the menu bar, the panes, the panels, the bars and the status bar.

use birchpad_config::SplitOrientation;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable as _, h_resizable, resizable_panel, v_resizable};
use gpui_kit::{
    AnyElement, App, Context, ExternalPaths, ModifiersChangedEvent, Window, div, prelude::*, px,
    relative,
};

use crate::app_state::AppState;
use crate::status_bar::StatusInfo;

use super::{Workspace, folder_files};

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_title(window, cx);
        let status = self
            .active_view(cx)
            .map(|view| view.update(cx, |view, cx| StatusInfo::of(view, cx)));
        let ansi = AppState::global(cx).ansi;
        let appearance = &AppState::global(cx).settings.appearance;
        let (show_menu_bar, show_status_bar) = (appearance.menu_bar, appearance.status_bar);
        // Hidden, the menu bar still shows while one of its menus is open (Alt, F10).
        let menu_bar_shown = show_menu_bar
            || self
                .menu_bar
                .as_ref()
                .is_some_and(|menu_bar| menu_bar.read(cx).is_active());
        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            // Ctrl released: the document switcher keeps the document it shows.
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                if !event.modifiers.control {
                    for pane in this.panes.clone() {
                        pane.update(cx, |pane, cx| pane.end_switch(cx));
                    }
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                // A folder goes to Folder as Workspace, as in Notepad++.
                let opens_files = AppState::global(cx).settings.files.drop_folder_opens_files;
                for path in paths.paths() {
                    if path.is_dir() && opens_files {
                        // Preferences > Default Directory: open the folder's files instead.
                        for file in folder_files(path) {
                            this.open_path(&file, window, cx);
                        }
                    } else if path.is_dir() {
                        this.add_workspace_folder(path, window, cx);
                    } else {
                        this.open_path(path, window, cx);
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::theme::paint(crate::theme::ui().background))
            .text_color(crate::theme::paint(crate::theme::ui().text))
            .when_some(self.menu_bar.clone(), birchpad_menu_bar::route_input)
            .when_some(
                self.menu_bar
                    .clone()
                    .filter(|_| !self.modes.text_only() && menu_bar_shown),
                |this, menu_bar| {
                    this.child(
                        div()
                            .flex_none()
                            .h(px(30.))
                            .px_1()
                            .border_b_1()
                            .border_color(crate::theme::paint(crate::theme::ui().border))
                            .child(menu_bar),
                    )
                },
            )
            .when(!self.modes.text_only(), |this| {
                this.children(self.render_toolbar(cx))
            })
            .child({
                let center = self.render_main_area(cx);
                let main = if self.modes.distraction_free {
                    // The text in a column in the middle, as wide as Preferences > Margins >
                    // Distraction Free padding leaves it.
                    let padding = AppState::global(cx)
                        .settings
                        .editor
                        .padding_distraction_free
                        .min(9);
                    let width = 1. - f32::from(padding) / 10.;
                    div()
                        .size_full()
                        .flex()
                        .justify_center()
                        .child(div().h_full().w(relative(width)).child(center))
                        .into_any_element()
                } else if self.modes.post_it {
                    center
                } else {
                    self.render_with_docks(center, cx)
                };
                div()
                    .relative()
                    .flex_1()
                    .min_h(px(0.))
                    .child(main)
                    .when(self.modes.text_only(), |this| {
                        this.child(self.render_leave_button(cx))
                    })
            })
            .when(self.find_bar.read(cx).visible, |this| {
                this.child(self.find_bar.clone())
            })
            .when(self.incremental.read(cx).visible, |this| {
                this.child(self.incremental.clone())
            })
            .when(!self.modes.text_only() && show_status_bar, |this| {
                this.child(crate::status_bar::render(status, ansi, cx))
            })
    }
}

impl Workspace {
    /// The way out of Post-It and Distraction Free, whose menus are hidden: a faint button in
    /// the top right corner.
    fn render_leave_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_1()
            .right_4()
            .opacity(0.4)
            .hover(|style| style.opacity(1.))
            .child(
                Button::new("leave-text-only")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .debug_selector(|| "leave-text-only".into())
                    .tooltip(t!("window-modes.leave"))
                    .on_click(cx.listener(|this, _, window, cx| this.leave_text_only(window, cx))),
            )
    }

    /// The documents, and the search results under them when they are shown.
    fn render_main_area(&self, cx: &App) -> AnyElement {
        if !self.search_results.read(cx).visible {
            return self.render_panes(cx);
        }
        v_resizable("results-split")
            .child(resizable_panel().child(self.render_panes(cx)))
            .child(
                resizable_panel()
                    .size(px(220.))
                    .size_range(px(60.)..px(2000.))
                    .child(
                        div()
                            .size_full()
                            .border_t_1()
                            .border_color(crate::theme::paint(crate::theme::ui().border))
                            .child(self.search_results.clone()),
                    ),
            )
            .into_any_element()
    }

    /// The pane with tabs, or both side by side or stacked, with a splitter between them.
    fn render_panes(&self, cx: &App) -> AnyElement {
        if !self.is_split(cx) {
            let shown = self
                .panes
                .iter()
                .find(|pane| !pane.read(cx).items().is_empty())
                .unwrap_or(&self.panes[0]);
            return shown.clone().into_any_element();
        }
        let group = match AppState::global(cx).state.split {
            SplitOrientation::SideBySide => h_resizable("split-side-by-side"),
            SplitOrientation::Stacked => v_resizable("split-stacked"),
        };
        // A line over the active view, as Notepad++ marks the tab of the focused view.
        let panel = |index: usize| {
            let line = if index == self.active_pane {
                crate::theme::paint(crate::theme::ui().accent)
            } else {
                crate::theme::paint(crate::theme::ui().background)
            };
            resizable_panel().child(
                div()
                    .size_full()
                    .border_t_2()
                    .border_color(line)
                    .child(self.panes[index].clone()),
            )
        };
        group.child(panel(0)).child(panel(1)).into_any_element()
    }
}
