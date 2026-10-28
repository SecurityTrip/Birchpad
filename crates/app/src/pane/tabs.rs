//! The tab bar: each tab, its context menu and colors, its name, and the document peeker.

use birchpad_commands::Invocation;

use birchpad_config::TabSettings;
use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_range};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, IconName, Sizable};
use gpui_kit::{
    AnyElement, AnyView, App, ClickEvent, Context, Entity, MouseButton, SharedString, Window, div,
    prelude::*, px,
};
use serde_json::json;

use crate::buffer::ReadOnly;
use crate::editor::EditorView;
use crate::theme::{paint, ui};

use birchpad_editor::TAB_COLORS;

use super::{DraggedTab, Pane, PaneEvent};

/// The theme's color `number` (1 to 5) for a tab.
pub(super) fn tab_color(number: u8) -> Option<birchpad_theme::Color> {
    let ui = ui();
    match number {
        1 => Some(ui.tab_color_1),
        2 => Some(ui.tab_color_2),
        3 => Some(ui.tab_color_3),
        4 => Some(ui.tab_color_4),
        5 => Some(ui.tab_color_5),
        _ => None,
    }
}

/// A pushpin, drawn on a pinned tab.
pub(super) const PIN_ICON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 2h5M6.5 2v4L4 9h8L9.5 6V2M8 9v5"/></svg>"#;

/// Alternate icons: a pencil on a tab with unsaved changes, and a padlock on a read-only one.
pub(super) const PENCIL_ICON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M10.5 2.5l3 3L6 13H3v-3z M9 4l3 3"/></svg>"#;
pub(super) const LOCK_ICON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7.5h8v6H4z M5.5 7.5V5a2.5 2.5 0 0 1 5 0v2.5"/></svg>"#;

/// How many lines the document peeker shows, and how many characters of each.
pub const PEEK_LINES: usize = 12;
pub const PEEK_COLUMNS: usize = 72;

impl Pane {
    /// The tab's context menu: commands for its document, as Notepad++'s.
    pub(super) fn tab_menu(
        &self,
        item: &Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let pane = cx.entity().downgrade();
        let view = item.clone();
        let marks = item.read(cx).tab;
        move |menu, _, _| {
            let entry = |invocation: Invocation, label: String| {
                let pane = pane.clone();
                let view = view.clone();
                PopupMenuItem::new(label).on_click(move |_: &ClickEvent, _, cx| {
                    let invocation = invocation.clone();
                    let view = Some(view.clone());
                    pane.update(cx, |_, cx| cx.emit(PaneEvent::Command { view, invocation }))
                        .ok();
                })
            };
            let command = |id: &str| entry(Invocation::new(id), crate::menus::command_title(id));
            let mut menu = menu
                .item(command("file.close"))
                .item(command("file.close-others"))
                .item(command("file.close-all-but-pinned"))
                .item(command("file.close-left"))
                .item(command("file.close-right"))
                .item(command("file.close-unchanged"))
                .separator()
                .item(command("file.save"))
                .item(command("file.save-as"))
                .item(command("file.print"))
                .separator()
                .item(command("file.rename"))
                .item(command("file.move-to-recycle-bin"))
                .item(command("file.reload"))
                .separator()
                .item(entry(
                    Invocation::with_args(
                        "file.containing-folder",
                        json!({ "in": "file-manager" }),
                    ),
                    crate::menus::command_title("file.containing-folder"),
                ))
                .item(command("edit.copy-full-path"))
                .item(command("edit.copy-file-name"))
                .item(command("edit.copy-folder-path"))
                .separator()
                .item(command("view.pin-tab").checked(marks.pinned))
                .separator();
            for color in 1..=TAB_COLORS {
                let label = format!("Apply Color {color}");
                let text = crate::i18n::get_or(&crate::menus::menu_key(&label), &label);
                let invocation = Invocation::with_args("view.tab-color", json!({ "color": color }));
                menu = menu.item(entry(invocation, text).checked(marks.color == Some(color)));
            }
            menu.item(command("view.remove-tab-color"))
                .separator()
                .item(command("view.move-to-other-view"))
                .item(command("view.clone-to-other-view"))
        }
    }

    /// The start of a document's text from its first line shown, for the document peeker.
    pub(super) fn peek(item: &Entity<EditorView>, cx: &App) -> Vec<String> {
        let view = item.read(cx);
        let first = view.visible_lines().map_or(0, |lines| lines.start);
        peek_lines(view.buffer.read(cx).doc().text(), first)
    }

    pub(super) fn render_tab(
        &self,
        index: usize,
        item: &Entity<EditorView>,
        options: &TabSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = self.index;
        let active = index == self.active;
        let marks = item.read(cx).tab;
        let buffer = item.read(cx).buffer.read(cx);
        let modified = buffer.is_modified();
        let read_only = matches!(
            buffer.read_only(),
            Some(
                ReadOnly::File | ReadOnly::Decoding(_) | ReadOnly::Requested | ReadOnly::Monitoring
            )
        );
        let label: SharedString = options.label(&tab_name(buffer, cx)).into();
        let prefix = if options.alternate_icons {
            // Notepad++'s alternate icons: a padlock, a pencil or a check mark.
            let icon = if read_only {
                Icon::default().data(LOCK_ICON)
            } else if modified {
                Icon::default().data(PENCIL_ICON)
            } else {
                Icon::new(IconName::Check)
            };
            icon.xsmall()
                .text_color(paint(if modified && !read_only {
                    ui().modified
                } else {
                    ui().faint
                }))
                .into_any_element()
        } else if read_only {
            // Read-only: an eye, as in "view only".
            Icon::new(IconName::Eye)
                .xsmall()
                .text_color(paint(ui().faint))
                .into_any_element()
        } else {
            div()
                .flex_none()
                .size(px(8.))
                .rounded_full()
                .when(modified, |dot| dot.bg(paint(ui().modified)))
                .into_any_element()
        };
        // Inactive tabs show their buttons only if the settings say so.
        let buttons = active || options.inactive_buttons;
        let suffix = if marks.pinned && options.pinning && buttons {
            let unpin = item.clone();
            Some(
                Button::new(("unpin-tab", index))
                    .ghost()
                    .xsmall()
                    .icon(Icon::default().data(PIN_ICON))
                    .debug_selector(move || format!("pane-{pane}-unpin-tab-{index}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.set_pinned(&unpin, false, cx);
                    }))
                    .into_any_element(),
            )
        } else if options.pinning && !options.only_pinned_button && !marks.pinned && buttons {
            let pin = item.clone();
            Some(
                Button::new(("pin-tab", index))
                    .ghost()
                    .xsmall()
                    .icon(Icon::default().data(PIN_ICON).text_color(paint(ui().faint)))
                    .debug_selector(move || format!("pane-{pane}-pin-tab-{index}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.set_pinned(&pin, true, cx);
                    }))
                    .into_any_element(),
            )
        } else if options.close_button && buttons {
            let close_item = item.clone();
            Some(
                Button::new(("close-tab", index))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .debug_selector(move || format!("pane-{pane}-close-tab-{index}"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(PaneEvent::CloseRequested(close_item.clone()));
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };
        let background = match marks.color.and_then(tab_color) {
            Some(color) => paint(color),
            None if active || !options.inactive_color => paint(ui().background),
            None => paint(ui().surface),
        };
        // The peeker in the Document Map, when it is open.
        let peek_on_map = {
            let app = crate::app_state::AppState::global(cx);
            app.settings.misc.peek_on_document_map
                && app.state.panels.open.iter().any(|id| id == "document-map")
        };
        let middle_item = item.clone();
        let clicked_item = item.clone();
        let double_click_close = options.double_click_close;
        let peek_item = item.downgrade();
        let tab = div()
            .id(("tab", index))
            .debug_selector(move || format!("pane-{pane}-tab-{index}"))
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .pl_2()
            .pr_1()
            .h(px(if options.reduce { 24. } else { 30. }))
            .when(options.reduce, |tab| tab.text_xs())
            .when(!options.reduce, |tab| tab.text_sm())
            .bg(background)
            .text_color(paint(if active { ui().text } else { ui().muted }))
            .border_color(paint(ui().border))
            .map(|tab| {
                // The active tab is marked by a line on the side towards its text.
                let line = paint(if active && options.active_bar {
                    ui().accent
                } else {
                    ui().border
                });
                if options.vertical {
                    tab.w_full().border_b_1().border_r_2().border_color(line)
                } else {
                    tab.max_w(px(240.))
                        .border_r_1()
                        .border_t_2()
                        .border_color(line)
                }
            })
            .when(!active && marks.color.is_none(), |tab| {
                tab.hover(|style| style.bg(paint(ui().hovered)))
            })
            .child(prefix)
            .child(div().flex_1().min_w(px(0.)).truncate().child(label.clone()))
            .children(suffix)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if double_click_close && event.click_count() == 2 {
                    cx.emit(PaneEvent::CloseRequested(clicked_item.clone()));
                } else if let Some(index) = this.index_of(&clicked_item) {
                    this.activate(index, window, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(PaneEvent::CloseRequested(middle_item.clone()));
                }),
            )
            .when(options.peeker && !active && peek_on_map, |tab| {
                let peeked = item.clone();
                tab.on_hover(cx.listener(move |_, hovered: &bool, _, cx| {
                    cx.emit(PaneEvent::Peek(hovered.then(|| peeked.clone())));
                }))
            })
            .when(options.peeker && !active && !peek_on_map, |tab| {
                tab.tooltip(move |_, cx| {
                    let lines = peek_item
                        .upgrade()
                        .map(|item| Self::peek(&item, cx))
                        .unwrap_or_default();
                    AnyView::from(cx.new(|_| Peek { lines }))
                })
            })
            .when(!options.lock, |tab| {
                let dragged = DraggedTab {
                    view: item.clone(),
                    label,
                };
                tab.on_drag(dragged, |tab, _, _, cx| cx.new(|_| tab.clone()))
                    .drag_over::<DraggedTab>(|style, _, _, _| style.bg(paint(ui().selected)))
                    .on_drop(cx.listener(move |_, tab: &DraggedTab, window, cx| {
                        cx.emit(PaneEvent::Dropped {
                            view: tab.view.clone(),
                            index,
                            clone: window.modifiers().secondary(),
                        });
                    }))
            });
        tab.context_menu(self.tab_menu(item, cx)).into_any_element()
    }

    /// The tabs: on one row that scrolls, on several rows, or in a column.
    pub(super) fn render_tabs(
        &self,
        options: &TabSettings,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tabs: Vec<AnyElement> = self
            .items
            .clone()
            .iter()
            .enumerate()
            .map(|(index, item)| self.render_tab(index, item, options, cx))
            .collect();
        let end = self.items.len();
        let pane = self.index;
        // The bar's empty part: a double click opens a new document, a dropped tab goes last.
        let rest = div()
            .id("tab-bar-rest")
            .debug_selector(move || format!("pane-{pane}-tab-bar-rest"))
            .flex_1()
            .min_w(px(24.))
            .min_h(px(if options.reduce { 24. } else { 30. }))
            .on_click(cx.listener(|_, event: &ClickEvent, _, cx| {
                if event.click_count() == 2 {
                    cx.emit(PaneEvent::Command {
                        view: None,
                        invocation: Invocation::new("file.new"),
                    });
                }
            }))
            .when(!options.lock, |rest| {
                rest.drag_over::<DraggedTab>(|style, _, _, _| style.bg(paint(ui().selected)))
                    .on_drop(cx.listener(move |_, tab: &DraggedTab, window, cx| {
                        cx.emit(PaneEvent::Dropped {
                            view: tab.view.clone(),
                            index: end,
                            clone: window.modifiers().secondary(),
                        });
                    }))
            });
        let bar = div()
            .id("tabs")
            .debug_selector(move || format!("pane-{pane}-tab-bar"))
            .flex()
            .flex_none()
            .bg(paint(ui().surface))
            .border_color(paint(ui().border));
        if options.vertical {
            bar.flex_col()
                .w(px(180.))
                .h_full()
                .border_r_1()
                .overflow_y_scroll()
                .track_scroll(&self.tab_scroll)
                .children(tabs)
                .child(rest)
        } else if options.multi_line {
            bar.flex_row()
                .flex_wrap()
                .w_full()
                .border_b_1()
                .children(tabs)
                .child(rest)
        } else {
            bar.flex_row()
                .w_full()
                .border_b_1()
                .overflow_x_scroll()
                .track_scroll(&self.tab_scroll)
                .children(tabs)
                .child(rest)
        }
    }
}

/// Up to [`PEEK_LINES`] lines of `text` from line `first`, tabs as spaces, each cut at
/// [`PEEK_COLUMNS`] characters with an ellipsis.
pub fn peek_lines(text: &Rope, first: usize) -> Vec<String> {
    let count = line_count(text);
    let first = first.min(count.saturating_sub(1));
    (first..count.min(first + PEEK_LINES))
        .map(|line| {
            let range = line_range(text, line);
            let mut shown = String::new();
            for (column, c) in text.slice(range).chars().enumerate() {
                if column == PEEK_COLUMNS {
                    shown.push('…');
                    break;
                }
                match c {
                    '\t' => shown.push_str("    "),
                    c if c.is_control() => shown.push(' '),
                    c => shown.push(c),
                }
            }
            shown
        })
        .collect()
}

/// The document peeker's view: the start of a document's text.
pub(super) struct Peek {
    lines: Vec<String>,
}

impl Render for Peek {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "tab-peek".into())
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(paint(ui().border))
            .bg(paint(ui().background))
            .text_color(paint(ui().text))
            .shadow_md()
            .font_family(crate::MONOSPACE)
            .text_xs()
            .children(
                self.lines
                    .iter()
                    .map(|line| div().h(px(16.)).child(line.clone())),
            )
    }
}

/// The name a tab shows: the document's, or for an untitled one with Preferences > New
/// Document > Use the first line as the tab's name, its first line (60 characters at most).
pub fn tab_name(buffer: &crate::buffer::Buffer, cx: &App) -> String {
    let name = buffer.display_name();
    if buffer.path().is_some()
        || !crate::app_state::AppState::global(cx)
            .settings
            .new_document
            .first_line_name
    {
        return name;
    }
    let first = buffer
        .doc()
        .text()
        .lines(birchpad_core::LINE_TYPE)
        .next()
        .map(|line| line.chars().take(60).collect::<String>())
        .unwrap_or_default();
    let first = first.trim();
    if first.is_empty() {
        name
    } else {
        first.to_owned()
    }
}
