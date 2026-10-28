//! A pane: a bar of tabs and the editor view of the active one.
//!
//! The workspace has two panes, Notepad++'s main and second views; a pane without tabs is
//! hidden. Both may show the same buffer, each in its own view ("Clone to Other View"). Tabs are
//! dragged to another place in their pane or to the other pane, and closed with their close
//! button or the middle mouse button, as in Notepad++. While the other pane is hidden, a tab
//! dragged to the far side of the text opens it there: split view by dragging.
//!
//! As in Notepad++, pinned tabs stay at the start of the bar, tabs may have one of five colors,
//! a tab's context menu acts on its document, and hovering over a tab that is not active shows
//! the start of its text (the document peeker). Preferences > Tab Bar puts the tabs on one row
//! that scrolls, on several rows, or in a column at the left. Ctrl+Tab's document switcher lists
//! the pane's documents, most recently used first, while Ctrl is held.

mod commands;
mod switcher;
mod tabs;

pub use commands::{Step, is_checked, register_commands};
use switcher::Switcher;
pub use tabs::{PEEK_COLUMNS, PEEK_LINES, peek_lines, tab_name};

use birchpad_commands::Invocation;
use std::collections::HashMap;

use birchpad_config::{PreviewMode, SplitOrientation};
use gpui_kit::component::{h_resizable, resizable_panel};
use gpui_kit::{
    AnyElement, App, Bounds, Context, DragMoveEvent, Entity, EntityId, EventEmitter, Pixels, Point,
    ScrollHandle, SharedString, Subscription, WeakEntity, Window, div, prelude::*, px, relative,
};

use crate::app_state::AppState;
use crate::buffer::BufferEvent;
use crate::editor::EditorView;
use crate::preview::Preview;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

pub use birchpad_editor::{TAB_COLORS, TabMarks};

pub enum PaneEvent {
    /// Another tab became active, or the active tab's state changed.
    ActiveItemChanged,
    /// The user clicked a tab's close button, or the tab with the middle mouse button.
    CloseRequested(Entity<EditorView>),
    /// A tab was dropped on this pane, to go at `index`. It may come from the other pane; with
    /// `clone` (Ctrl held), a new view of its document goes there and the tab stays.
    Dropped {
        view: Entity<EditorView>,
        index: usize,
        clone: bool,
    },
    /// A tab was dropped on the far side of this pane's text while the other pane is hidden:
    /// the other pane opens with it, split `orientation`. With `clone`, with a new view of its
    /// document.
    Split {
        view: Entity<EditorView>,
        orientation: SplitOrientation,
        clone: bool,
    },
    /// A command of a tab's context menu, for the document of `view` (activated first), or of
    /// the empty part of the bar.
    Command {
        view: Option<Entity<EditorView>>,
        invocation: Invocation,
    },
    /// The mouse went over an inactive tab (`Some`) or left it, for the Document Map to show
    /// it (MISC > Document Peeker in the Document Map).
    Peek(Option<Entity<EditorView>>),
}

/// A tab being dragged.
#[derive(Clone)]
pub struct DraggedTab {
    pub view: Entity<EditorView>,
    label: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(paint(ui().border))
            .bg(paint(ui().background))
            .shadow_md()
            .text_sm()
            .child(self.label.clone())
    }
}

pub struct Pane {
    /// 0 for the main view, 1 for the second one.
    index: usize,
    /// The other pane.
    other: WeakEntity<Pane>,
    items: Vec<Entity<EditorView>>,
    active: usize,
    /// The tabs, most recently active first, for the document switcher.
    recent: Vec<EntityId>,
    switcher: Option<Switcher>,
    tab_scroll: ScrollHandle,
    subscriptions: Vec<(Entity<EditorView>, Subscription)>,
    /// How the panes would split if the tab being dragged were dropped where it is.
    split_zone: Option<SplitOrientation>,
    /// Post-It and Distraction Free show the text alone.
    tabs_hidden: bool,
    /// The previews of the documents that have one (Markdown), by their views.
    previews: HashMap<EntityId, Entity<Preview>>,
    /// The documents shown as bytes (View in HEX), by their views.
    hex_editors: HashMap<EntityId, Entity<crate::hex_editor::HexEditor>>,
    workspace: WeakEntity<Workspace>,
}

impl EventEmitter<PaneEvent> for Pane {}

impl Pane {
    /// The main (`index` 0) or the second view.
    pub fn new(index: usize) -> Self {
        Self {
            index,
            other: WeakEntity::new_invalid(),
            items: Vec::new(),
            active: 0,
            recent: Vec::new(),
            switcher: None,
            tab_scroll: ScrollHandle::new(),
            subscriptions: Vec::new(),
            split_zone: None,
            tabs_hidden: false,
            previews: HashMap::new(),
            hex_editors: HashMap::new(),
            workspace: WeakEntity::new_invalid(),
        }
    }

    /// The hex view of `item`, while it shows.
    pub fn hex_editor(
        &self,
        item: &Entity<EditorView>,
    ) -> Option<&Entity<crate::hex_editor::HexEditor>> {
        self.hex_editors.get(&item.entity_id())
    }

    /// View in HEX: shows `item`'s bytes in place of its text, or its text again. The caret
    /// goes with it, and so does the focus.
    pub fn toggle_hex(
        &mut self,
        item: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = item.entity_id();
        if self.hex_editors.remove(&id).is_some() {
            let focus = item.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        } else {
            let item = item.clone();
            let editor = cx.new(|cx| crate::hex_editor::HexEditor::new(item, window, cx));
            let focus = gpui_kit::Focusable::focus_handle(editor.read(cx), cx);
            self.hex_editors.insert(id, editor);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Hides the tab bar (Post-It, Distraction Free) or shows it again.
    pub fn set_tabs_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.tabs_hidden = hidden;
        cx.notify();
    }

    pub fn set_other(&mut self, other: WeakEntity<Pane>) {
        self.other = other;
    }

    pub fn set_workspace(&mut self, workspace: WeakEntity<Workspace>) {
        self.workspace = workspace;
    }

    /// The preview of `item`, if its document has one, made when first asked for.
    pub fn preview(
        &mut self,
        item: &Entity<EditorView>,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Preview>> {
        let kind = crate::preview::kind_of(item.read(cx).buffer.read(cx))?;
        let id = item.entity_id();
        if let Some(preview) = self.previews.get(&id)
            && preview.read(cx).kind == kind
        {
            return Some(preview.clone());
        }
        // A new preview, or one of another kind: a document saved as `.puml` from Markdown.
        let mode = self.previews.get(&id).map(|preview| preview.read(cx).mode);
        let workspace = self.workspace.clone();
        let preview = cx.new(|cx| {
            let mut preview = Preview::new(item, kind, workspace, cx);
            preview.mode = mode.unwrap_or(preview.mode);
            preview
        });
        self.previews.insert(id, preview.clone());
        Some(preview)
    }

    /// How the active document is shown, if it has a preview.
    pub fn preview_mode(&self, cx: &App) -> Option<PreviewMode> {
        let item = self.active_item()?;
        crate::preview::kind_of(item.read(cx).buffer.read(cx))?;
        Some(
            self.previews
                .get(&item.entity_id())
                .map_or(AppState::global(cx).state.preview, |preview| {
                    preview.read(cx).mode
                }),
        )
    }

    /// Shows the active document as `mode` (View > Preview), remembered for documents opened
    /// later; false if it has no preview.
    pub fn set_preview_mode(&mut self, mode: PreviewMode, cx: &mut Context<Self>) -> bool {
        let Some(item) = self.active_item().cloned() else {
            return false;
        };
        let Some(preview) = self.preview(&item, cx) else {
            return false;
        };
        preview.update(cx, |preview, _| preview.mode = mode);
        crate::preview::remember(mode, cx);
        cx.notify();
        true
    }

    /// The active document: its text, with its preview beside it or in its place.
    fn render_item(&mut self, item: Entity<EditorView>, cx: &mut Context<Self>) -> AnyElement {
        if let Some(hex) = self.hex_editors.get(&item.entity_id()) {
            return hex.clone().into_any_element();
        }
        let Some(preview) = self.preview(&item, cx) else {
            return item.into_any_element();
        };
        let mode = preview.read(cx).mode;
        let pane = cx.entity().downgrade();
        let workspace = self.workspace.clone();
        let switch = crate::preview::render_switch(mode, move |mode, _, cx| {
            if let Some(pane) = pane.upgrade() {
                pane.update(cx, |pane, cx| pane.set_preview_mode(mode, cx));
            }
            if let Some(workspace) = workspace.upgrade() {
                workspace.update(cx, |workspace, cx| workspace.refresh_menus(cx));
            }
        });
        let body = match mode {
            PreviewMode::Editor => item.into_any_element(),
            PreviewMode::Preview => preview.into_any_element(),
            PreviewMode::Split => h_resizable(SharedString::from(format!(
                "preview-split-{}",
                item.entity_id()
            )))
            .child(resizable_panel().child(item))
            .child(resizable_panel().child(preview))
            .into_any_element(),
        };
        div()
            .relative()
            .size_full()
            .child(body)
            .child(switch)
            .into_any_element()
    }

    /// How a tab dropped at `position` in the text (`bounds`) would split the panes: on the far
    /// third towards where the other view opens (right or below the main view, left or above
    /// the second one), while that view is hidden.
    fn split_zone_at(
        &self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) -> Option<SplitOrientation> {
        let other_hidden = self
            .other
            .upgrade()
            .is_some_and(|other| other.read(cx).items.is_empty());
        if !other_hidden || self.items.is_empty() || !bounds.contains(&position) {
            return None;
        }
        let (width, height) = (bounds.size.width / 3., bounds.size.height / 3.);
        let (beside, below) = if self.index == 0 {
            (
                position.x > bounds.right() - width,
                position.y > bounds.bottom() - height,
            )
        } else {
            (
                position.x < bounds.left() + width,
                position.y < bounds.top() + height,
            )
        };
        if beside {
            Some(SplitOrientation::SideBySide)
        } else if below {
            Some(SplitOrientation::Stacked)
        } else {
            None
        }
    }

    /// The half of the text where the other view would open.
    fn render_split_zone(&self, orientation: SplitOrientation) -> impl IntoElement {
        let zone = div().absolute().bg(paint(ui().accent.with_alpha(0x33)));
        match (orientation, self.index) {
            (SplitOrientation::SideBySide, 0) => zone.top_0().right_0().h_full().w(relative(0.5)),
            (SplitOrientation::SideBySide, _) => zone.top_0().left_0().h_full().w(relative(0.5)),
            (SplitOrientation::Stacked, 0) => zone.bottom_0().left_0().w_full().h(relative(0.5)),
            (SplitOrientation::Stacked, _) => zone.top_0().left_0().w_full().h(relative(0.5)),
        }
    }

    pub fn items(&self) -> &[Entity<EditorView>] {
        &self.items
    }

    pub fn active_item(&self) -> Option<&Entity<EditorView>> {
        self.items.get(self.active)
    }

    pub fn index_of(&self, item: &Entity<EditorView>) -> Option<usize> {
        self.items.iter().position(|existing| existing == item)
    }

    /// How many tabs at the start of the bar are pinned.
    fn pinned_count(&self, cx: &App) -> usize {
        self.items
            .iter()
            .take_while(|item| item.read(cx).tab.pinned)
            .count()
    }

    /// `index` moved into the group `item` belongs to: pinned tabs come before the others.
    /// `item` is not among the tabs.
    fn place_in_group(&self, item: &Entity<EditorView>, index: usize, cx: &App) -> usize {
        let pinned = self.pinned_count(cx);
        if item.read(cx).tab.pinned {
            index.min(pinned)
        } else {
            index.clamp(pinned, self.items.len())
        }
    }

    /// Adds a tab after the active one and activates it.
    pub fn add(&mut self, item: Entity<EditorView>, window: &mut Window, cx: &mut Context<Self>) {
        let index = if self.items.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.insert(index, item, window, cx);
    }

    /// Puts a tab at `index` (at the end if past it, after the pinned tabs unless it is pinned)
    /// and activates it.
    pub fn insert(
        &mut self,
        index: usize,
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
        let index = self.place_in_group(&item, index.min(self.items.len()), cx);
        self.items.insert(index, item);
        if let Some(switcher) = &mut self.switcher {
            switcher.order.push(self.items[index].clone());
        }
        self.activate(index, window, cx);
    }

    /// Moves a tab of this pane so that it ends up at `to` (counted before the move), within
    /// its group, and activates it.
    pub fn move_item(
        &mut self,
        item: &Entity<EditorView>,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.index_of(item) else {
            return;
        };
        let item = self.items.remove(from);
        // Dropping on a tab right of the old place: the tabs between shifted left.
        let to = if to > from { to - 1 } else { to }.min(self.items.len());
        let to = self.place_in_group(&item, to, cx);
        self.items.insert(to, item);
        self.activate(to, window, cx);
    }

    /// View > Tab > Move Tab Forward, Backward, to Start, to End: the active tab, within its
    /// group.
    pub fn move_active(&mut self, step: Step, cx: &mut Context<Self>) {
        let Some(item) = self.active_item().cloned() else {
            return;
        };
        let pinned = self.pinned_count(cx);
        let group = if item.read(cx).tab.pinned {
            0..pinned
        } else {
            pinned..self.items.len()
        };
        let to = match step {
            Step::Forward => (self.active + 1).min(group.end - 1),
            Step::Backward => self.active.saturating_sub(1).max(group.start),
            Step::Start => group.start,
            Step::End => group.end - 1,
        };
        if to == self.active {
            return;
        }
        let item = self.items.remove(self.active);
        self.items.insert(to, item);
        self.active = to;
        self.tab_scroll.scroll_to_item(to);
        cx.emit(PaneEvent::ActiveItemChanged);
        cx.notify();
    }

    /// Puts the tabs in the order of `order` (Window > Sort By, Sort Tabs); tabs it leaves out
    /// follow in their own order, and pinned tabs still come first.
    pub fn reorder(&mut self, order: &[Entity<EditorView>], cx: &mut Context<Self>) {
        let active = self.active_item().cloned();
        let mut items: Vec<Entity<EditorView>> = Vec::with_capacity(self.items.len());
        for item in order.iter().chain(self.items.iter()) {
            if self.items.contains(item) && !items.contains(item) {
                items.push(item.clone());
            }
        }
        let (pinned, others): (Vec<_>, Vec<_>) =
            items.into_iter().partition(|item| item.read(cx).tab.pinned);
        self.items = pinned.into_iter().chain(others).collect();
        if let Some(index) = active.and_then(|active| self.index_of(&active)) {
            self.active = index;
            self.tab_scroll.scroll_to_item(index);
        }
        cx.emit(PaneEvent::ActiveItemChanged);
        cx.notify();
    }

    /// Pins or unpins a tab: it goes to the end of the pinned tabs, or to the start of the
    /// others.
    pub fn set_pinned(&mut self, item: &Entity<EditorView>, pinned: bool, cx: &mut Context<Self>) {
        let Some(from) = self.index_of(item) else {
            return;
        };
        let active = self.active_item().cloned();
        let item = self.items.remove(from);
        item.update(cx, |view, _| view.tab.pinned = pinned);
        let to = self.pinned_count(cx);
        self.items.insert(to, item);
        if let Some(index) = active.and_then(|active| self.index_of(&active)) {
            self.active = index;
        }
        cx.emit(PaneEvent::ActiveItemChanged);
        cx.notify();
    }

    /// View > Tab > Apply Color (1 to 5) or Remove Color.
    pub fn set_color(
        &mut self,
        item: &Entity<EditorView>,
        color: Option<u8>,
        cx: &mut Context<Self>,
    ) {
        if self.index_of(item).is_none() {
            return;
        }
        item.update(cx, |view, _| view.tab.color = color);
        cx.emit(PaneEvent::ActiveItemChanged);
        cx.notify();
    }

    /// Activates the tab at `index`, if there is one, and ends the document switcher.
    pub fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.items.len() {
            return;
        }
        self.switcher = None;
        self.show(index, window, cx);
        self.note_recent();
    }

    /// Activates the tab at `index` without changing the order of recent tabs.
    fn show(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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

    /// The active tab becomes the most recent one.
    fn note_recent(&mut self) {
        let Some(id) = self.active_item().map(|item| item.entity_id()) else {
            return;
        };
        self.recent.retain(|known| *known != id);
        self.recent.insert(0, id);
    }

    /// Removes a tab. If it was active, its right neighbor (or the new last tab) becomes active.
    pub fn remove(
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
        self.recent.retain(|id| *id != item.entity_id());
        self.hex_editors.remove(&item.entity_id());
        if let Some(switcher) = &mut self.switcher {
            if let Some(position) = switcher.order.iter().position(|view| view == item) {
                switcher.order.remove(position);
                if position < switcher.selected {
                    switcher.selected -= 1;
                }
            }
            if switcher.order.is_empty() {
                self.switcher = None;
            } else {
                switcher.selected = switcher.selected.min(switcher.order.len() - 1);
            }
        }
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
        let next = next.min(self.items.len() - 1);
        if self.switcher.is_some() {
            self.show(next, window, cx);
        } else {
            self.activate(next, window, cx);
        }
    }
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_item().cloned();
        let items = &self.items;
        self.previews
            .retain(|id, _| items.iter().any(|item| item.entity_id() == *id));
        let end = self.items.len();
        let split_zone = self.split_zone.filter(|_| cx.has_active_drag());
        let index = self.index;
        let options = AppState::global(cx).settings.tabs.clone();
        let content = div()
            .id("pane-content")
            .debug_selector(move || format!("pane-content-{index}"))
            .relative()
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            // Dropped on the text: the tab goes last, or to the other view on its far side.
            .drag_over::<DraggedTab>(|style, _, _, _| style.bg(paint(ui().selected)))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    let zone = this.split_zone_at(event.event.position, event.bounds, cx);
                    if zone != this.split_zone {
                        this.split_zone = zone;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(move |this, tab: &DraggedTab, window, cx| {
                let clone = window.modifiers().secondary();
                let view = tab.view.clone();
                match this.split_zone.take() {
                    Some(orientation) => cx.emit(PaneEvent::Split {
                        view,
                        orientation,
                        // Moving the only tab would leave nothing to split: it is cloned.
                        clone: clone || this.items.len() == 1,
                    }),
                    None => cx.emit(PaneEvent::Dropped {
                        view,
                        index: end,
                        clone,
                    }),
                }
                cx.notify();
            }))
            .when_some(active, |this, item| this.child(self.render_item(item, cx)))
            .when_some(split_zone, |this, orientation| {
                this.child(self.render_split_zone(orientation))
            })
            .children(self.render_switcher(cx));
        div()
            .flex()
            .when(options.vertical, |pane| pane.flex_row())
            .when(!options.vertical, |pane| pane.flex_col())
            .size_full()
            .when(!self.tabs_hidden, |pane| {
                pane.child(self.render_tabs(&options, cx))
            })
            .child(content)
    }
}
