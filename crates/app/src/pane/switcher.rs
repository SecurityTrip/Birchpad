//! Ctrl+Tab's document switcher: the pane's documents, most recently used first, while Ctrl is
//! held.

use gpui_kit::{Context, Entity, Window, div, prelude::*, px, relative};

use crate::app_state::AppState;
use crate::editor::EditorView;
use crate::theme::{paint, ui};

use super::Pane;

/// The document switcher while Ctrl is held: the pane's documents in the order it goes through
/// them, and the one it shows.
pub(super) struct Switcher {
    pub(super) order: Vec<Entity<EditorView>>,
    pub(super) selected: usize,
}

impl Pane {
    /// Activates the next (`delta = 1`) or previous (`-1`) tab, wrapping around.
    pub fn cycle(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.items.len() as isize;
        if len == 0 {
            return;
        }
        let index = (self.active as isize + delta).rem_euclid(len) as usize;
        self.activate(index, window, cx);
    }

    /// The order the document switcher goes through the tabs, the active one first: most
    /// recently used, or the tabs' own order.
    pub(super) fn switch_order(&self, recent_first: bool) -> Vec<Entity<EditorView>> {
        let mut order = self.items.clone();
        if !recent_first {
            let active = self.active.min(order.len().saturating_sub(1));
            order.rotate_left(active);
            return order;
        }
        let rank = |item: &Entity<EditorView>| {
            self.recent
                .iter()
                .position(|id| *id == item.entity_id())
                .unwrap_or(usize::MAX)
        };
        // Stable: tabs never active keep their order, after the others.
        order.sort_by_key(rank);
        if let Some(active) = self.active_item()
            && let Some(position) = order.iter().position(|item| item == active)
        {
            let active = order.remove(position);
            order.insert(0, active);
        }
        order
    }

    /// Ctrl+Tab (Ctrl+Shift+Tab with `backward`): the next document of the switcher, which
    /// shows while Ctrl is held. Without the switcher (Preferences > Tab Bar), the next tab.
    pub fn switch(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let settings = AppState::global(cx).settings.tabs.clone();
        if !settings.switcher {
            self.cycle(if backward { -1 } else { 1 }, window, cx);
            return;
        }
        if self.items.is_empty() {
            return;
        }
        let switcher = match self.switcher.take() {
            Some(switcher) => switcher,
            None => Switcher {
                order: self.switch_order(settings.switcher_mru),
                selected: 0,
            },
        };
        let len = switcher.order.len();
        let selected = if backward {
            (switcher.selected + len - 1) % len
        } else {
            (switcher.selected + 1) % len
        };
        let target = switcher.order[selected].clone();
        self.switcher = Some(Switcher {
            selected,
            ..switcher
        });
        if let Some(index) = self.index_of(&target) {
            self.show(index, window, cx);
        }
        cx.notify();
    }

    /// Ctrl was released: the document the switcher shows stays, as the most recent one.
    /// Whether the switcher was open.
    pub fn end_switch(&mut self, cx: &mut Context<Self>) -> bool {
        if self.switcher.take().is_none() {
            return false;
        }
        self.note_recent();
        cx.notify();
        true
    }
}

impl Pane {
    /// The document switcher's list, over the text.
    pub(super) fn render_switcher(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let switcher = self.switcher.as_ref()?;
        let pane = self.index;
        let rows = switcher.order.iter().enumerate().map(|(row, item)| {
            let name = item.read(cx).buffer.read(cx).display_name();
            let selected = row == switcher.selected;
            let target = item.clone();
            div()
                .id(("switcher-row", row))
                .debug_selector(move || format!("pane-{pane}-switcher-{row}"))
                .px_3()
                .py_1()
                .truncate()
                .when(selected, |row| row.bg(paint(ui().selected)))
                .hover(|style| style.bg(paint(ui().hovered)))
                .child(name)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(index) = this.index_of(&target) {
                        this.activate(index, window, cx);
                    }
                }))
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_start()
                .justify_center()
                .pt_8()
                .child(
                    div()
                        .id("switcher")
                        .debug_selector(move || format!("pane-{pane}-switcher"))
                        .occlude()
                        .w(px(320.))
                        .max_h(relative(0.8))
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(paint(ui().border))
                        .bg(paint(ui().background))
                        .text_color(paint(ui().text))
                        .text_sm()
                        .shadow_lg()
                        .children(rows),
                ),
        )
    }
}
