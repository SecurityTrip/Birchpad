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

use birchpad_commands::Invocation;
use std::collections::HashMap;

use birchpad_config::{PreviewMode, SplitOrientation, TabSettings};
use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_range};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, IconName, Sizable, h_resizable, resizable_panel};
use gpui_kit::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, DragMoveEvent, Entity, EntityId,
    EventEmitter, MouseButton, Pixels, Point, ScrollHandle, SharedString, Subscription, WeakEntity,
    Window, div, prelude::*, px, relative,
};
use serde_json::json;

use crate::app_state::AppState;
use crate::buffer::{BufferEvent, ReadOnly};
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::preview::Preview;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

pub use birchpad_editor::{TAB_COLORS, TabMarks};

/// The theme's color `number` (1 to 5) for a tab.
fn tab_color(number: u8) -> Option<birchpad_theme::Color> {
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
const PIN_ICON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 2h5M6.5 2v4L4 9h8L9.5 6V2M8 9v5"/></svg>"#;

/// How many lines the document peeker shows, and how many characters of each.
pub const PEEK_LINES: usize = 12;
pub const PEEK_COLUMNS: usize = 72;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.go-to-tab", |this, args: IndexArgs, window, cx| {
        if args.index == 0 {
            anyhow::bail!("{}", t!("pane.tabs-are-counted-from-1"));
        }
        this.active_pane()
            .update(cx, |pane, cx| pane.activate(args.index - 1, window, cx));
        Ok(())
    });
    registry.workspace("view.last-tab", |this, (), window, cx| {
        this.active_pane().update(cx, |pane, cx| {
            if let Some(last) = pane.items.len().checked_sub(1) {
                pane.activate(last, window, cx);
            }
        });
        Ok(())
    });
    for (id, step) in [
        ("view.move-tab-forward", Step::Forward),
        ("view.move-tab-backward", Step::Backward),
        ("view.move-tab-to-start", Step::Start),
        ("view.move-tab-to-end", Step::End),
    ] {
        registry.workspace(id, move |this, (), _, cx| {
            this.active_pane()
                .update(cx, |pane, cx| pane.move_active(step, cx));
            Ok(())
        });
    }
    registry.workspace("view.pin-tab", |this, (), _, cx| {
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            let pinned = !view.read(cx).tab.pinned;
            pane.update(cx, |pane, cx| pane.set_pinned(&view, pinned, cx));
        }
        Ok(())
    });
    registry.workspace("view.tab-color", |this, args: ColorArgs, _, cx| {
        if !(1..=TAB_COLORS).contains(&args.color) {
            anyhow::bail!(
                "{}",
                t!(
                    "pane.there-is-no-tab-color",
                    color = args.color,
                    last = TAB_COLORS
                )
            );
        }
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            pane.update(cx, |pane, cx| pane.set_color(&view, Some(args.color), cx));
        }
        Ok(())
    });
    registry.workspace("view.remove-tab-color", |this, (), _, cx| {
        let pane = this.active_pane().clone();
        if let Some(view) = this.active_view(cx) {
            pane.update(cx, |pane, cx| pane.set_color(&view, None, cx));
        }
        Ok(())
    });
    registry.workspace(
        "view.switch-document",
        |this, args: SwitchArgs, window, cx| {
            this.active_pane()
                .update(cx, |pane, cx| pane.switch(args.backward, window, cx));
            Ok(())
        },
    );
}

#[derive(serde::Deserialize)]
struct IndexArgs {
    index: usize,
}

#[derive(serde::Deserialize)]
struct ColorArgs {
    color: u8,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct SwitchArgs {
    backward: bool,
}

/// Where View > Tab > Move Tab moves the active tab, within its group (pinned or not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Forward,
    Backward,
    Start,
    End,
}

/// Whether View > Tab's switches are checked for the active tab.
pub fn is_checked(invocation: &Invocation, marks: Option<TabMarks>) -> Option<bool> {
    let marks = marks.unwrap_or_default();
    match invocation.command.as_str() {
        "view.pin-tab" => Some(marks.pinned),
        "view.tab-color" => Some(
            invocation
                .args
                .get("color")
                .and_then(|color| color.as_u64())
                .is_some_and(|color| marks.color.map(u64::from) == Some(color)),
        ),
        _ => None,
    }
}

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

/// The document switcher while Ctrl is held: the pane's documents in the order it goes through
/// them, and the one it shows.
struct Switcher {
    order: Vec<Entity<EditorView>>,
    selected: usize,
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
            workspace: WeakEntity::new_invalid(),
        }
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
    fn switch_order(&self, recent_first: bool) -> Vec<Entity<EditorView>> {
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

    /// The tab's context menu: commands for its document, as Notepad++'s.
    fn tab_menu(
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
    fn peek(item: &Entity<EditorView>, cx: &App) -> Vec<String> {
        let view = item.read(cx);
        let first = view.visible_lines().map_or(0, |lines| lines.start);
        peek_lines(view.buffer.read(cx).doc().text(), first)
    }

    fn render_tab(
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
        let label: SharedString = buffer.display_name().into();
        let prefix = if read_only {
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
        let suffix = if marks.pinned {
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
        } else if options.close_button {
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
            None if active => paint(ui().background),
            None => paint(ui().surface),
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
                let line = paint(if active { ui().accent } else { ui().border });
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
            .when(options.peeker && !active, |tab| {
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
    fn render_tabs(&self, options: &TabSettings, cx: &mut Context<Self>) -> impl IntoElement {
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

    /// The document switcher's list, over the text.
    fn render_switcher(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
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
struct Peek {
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

impl Workspace {
    /// Runs a command of a tab's context menu on its document.
    pub fn run_tab_command(
        &mut self,
        view: Option<&Entity<EditorView>>,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = view {
            self.activate_view(view, window, cx);
        }
        if let Err(error) = self.dispatch(invocation, window, cx) {
            crate::workspace::report_error(&error, window, cx);
        }
    }
}
