//! A pane: a row of tabs and the editor view of the active one.
//!
//! The workspace has one pane for now; split view (phase 2) adds a second one, which may show
//! the same buffer in another view.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Icon, IconName, Sizable};
use gpui_kit::{
    Context, Entity, EventEmitter, ScrollHandle, Subscription, Window, div, prelude::*, px, rgb,
};

use crate::buffer::{BufferEvent, ReadOnly};
use crate::editor::EditorView;

pub(crate) enum PaneEvent {
    /// Another tab became active, or the active tab's state changed.
    ActiveItemChanged,
    /// The user clicked a tab's close button.
    CloseRequested(Entity<EditorView>),
}

pub(crate) struct Pane {
    items: Vec<Entity<EditorView>>,
    active: usize,
    tab_scroll: ScrollHandle,
    subscriptions: Vec<(Entity<EditorView>, Subscription)>,
}

impl EventEmitter<PaneEvent> for Pane {}

impl Pane {
    pub(crate) fn new() -> Self {
        Self {
            items: Vec::new(),
            active: 0,
            tab_scroll: ScrollHandle::new(),
            subscriptions: Vec::new(),
        }
    }

    pub(crate) fn items(&self) -> &[Entity<EditorView>] {
        &self.items
    }

    pub(crate) fn active_item(&self) -> Option<&Entity<EditorView>> {
        self.items.get(self.active)
    }

    pub(crate) fn index_of(&self, item: &Entity<EditorView>) -> Option<usize> {
        self.items.iter().position(|existing| existing == item)
    }

    /// Adds a tab after the active one and activates it.
    pub(crate) fn add(
        &mut self,
        item: Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = item.read(cx).buffer.clone();
        let subscription = cx.subscribe(&buffer, |_, _, event, cx| {
            if matches!(event, BufferEvent::StateChanged) {
                cx.emit(PaneEvent::ActiveItemChanged);
                cx.notify();
            }
        });
        self.subscriptions.push((item.clone(), subscription));
        let index = if self.items.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.items.insert(index, item);
        self.activate(index, window, cx);
    }

    pub(crate) fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        self.active = index;
        self.tab_scroll.scroll_to_item(index);
        let focus = item.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        cx.emit(PaneEvent::ActiveItemChanged);
        cx.notify();
    }

    /// Activates the next (`delta = 1`) or previous (`-1`) tab, wrapping around.
    pub(crate) fn cycle(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.items.len() as isize;
        if len == 0 {
            return;
        }
        let index = (self.active as isize + delta).rem_euclid(len) as usize;
        self.activate(index, window, cx);
    }

    /// Removes a tab. If it was active, its right neighbor (or the new last tab) becomes active.
    pub(crate) fn remove(
        &mut self,
        item: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(item) else {
            return;
        };
        self.items.remove(index);
        self.subscriptions.retain(|(view, _)| view != item);
        if self.items.is_empty() {
            self.active = 0;
            cx.emit(PaneEvent::ActiveItemChanged);
            cx.notify();
            return;
        }
        let next = if index < self.active {
            self.active - 1
        } else {
            self.active
        };
        self.activate(next.min(self.items.len() - 1), window, cx);
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = self.items.iter().enumerate().map(|(index, item)| {
            let buffer = item.read(cx).buffer.read(cx);
            let modified = buffer.is_modified();
            let read_only = matches!(
                buffer.read_only(),
                Some(ReadOnly::File | ReadOnly::Decoding(_) | ReadOnly::Requested)
            );
            let label = buffer.display_name();
            let close_item = item.clone();
            Tab::new()
                .label(label)
                .prefix(if read_only {
                    // Read-only: an eye, as in "view only".
                    Icon::new(IconName::Eye)
                        .xsmall()
                        .ml_1()
                        .text_color(rgb(0x8c959f))
                        .into_any_element()
                } else {
                    div()
                        .ml_2()
                        .size(px(8.))
                        .rounded_full()
                        .when(modified, |dot| dot.bg(rgb(0xd1242f)))
                        .into_any_element()
                })
                .suffix(
                    Button::new(("close-tab", index))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(PaneEvent::CloseRequested(close_item.clone()));
                        })),
                )
        });
        TabBar::new("tabs")
            .track_scroll(&self.tab_scroll)
            .selected_index(self.active)
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                this.activate(*index, window, cx);
            }))
            .children(tabs)
    }
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_item().cloned();
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.render_tabs(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .when_some(active, |this, item| this.child(item)),
            )
    }
}
