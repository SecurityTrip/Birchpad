//! The two panes of split view and moving documents between them.

use gpui_kit::{App, AppContext as _, Context, Entity, Window};

use crate::editor::EditorView;
use crate::pane::Pane;

use super::Workspace;

impl Workspace {
    pub fn active_pane(&self) -> &Entity<Pane> {
        &self.panes[self.active_pane]
    }

    /// The main and the second view.
    pub fn panes(&self) -> &[Entity<Pane>; 2] {
        &self.panes
    }

    pub fn active_pane_index(&self) -> usize {
        self.active_pane
    }

    /// The pane showing `view`: 0 for the main view, 1 for the second.
    pub fn pane_of(&self, view: &Entity<EditorView>, cx: &App) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.read(cx).index_of(view).is_some())
    }

    /// Whether both views are shown.
    pub fn is_split(&self, cx: &App) -> bool {
        self.panes
            .iter()
            .all(|pane| !pane.read(cx).items().is_empty())
    }

    /// Follows the focus into a view (a click in the other pane makes it the active one) and its
    /// scrolling.
    pub fn track_view(
        &mut self,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = view.read(cx).focus_handle.clone();
        let focused_view = view.downgrade();
        let focused = cx.on_focus(&focus, window, move |this, _, cx| {
            let Some(view) = focused_view.upgrade() else {
                return;
            };
            if let Some(index) = this.pane_of(&view, cx)
                && index != this.active_pane
            {
                this.active_pane = index;
                this.refresh_menus(cx);
                cx.notify();
            }
        });
        let events = cx.subscribe_in(view, window, Self::on_editor_event);
        // Wherever its caret goes, for Go Back.
        let changes = cx.observe(view, |this, _, cx| this.note_place(cx));
        // Its typing and its selection, for the plugins that hear about them.
        let watched = cx.observe_in(view, window, |this, _, window, cx| {
            this.watch_for_plugins(window, cx);
        });
        self.view_subscriptions
            .insert(view.entity_id(), [focused, events, changes, watched]);
    }

    /// View > Move/Clone Current Document.
    pub(super) fn send_to_other_view(
        &mut self,
        clone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.active_view(cx) {
            let target = 1 - self.active_pane;
            self.send_to_pane(&view, target, None, clone, window, cx);
        }
    }

    /// Shows `view`'s document in pane `target`, at `index` or after its active tab: moves the
    /// tab there, or adds another view of the document at the same place. If that pane already
    /// shows the document, its view is activated instead (and a moved tab closes).
    pub(super) fn send_to_pane(
        &mut self,
        view: &Entity<EditorView>,
        target: usize,
        index: Option<usize>,
        clone: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = view.read(cx).buffer.clone();
        let source = self.pane_of(view, cx);
        let existing = self.panes[target]
            .read(cx)
            .items()
            .iter()
            .find(|other| other.read(cx).buffer == buffer)
            .cloned();
        if let Some(existing) = existing {
            if !clone && source != Some(target) {
                self.close_view(view, window, cx);
            }
            self.activate_view(&existing, window, cx);
            return;
        }
        let item = if clone {
            let state = view.read(cx).view_state(cx);
            let copy = cx.new(|cx| {
                let mut copy = EditorView::new(buffer, window, cx);
                copy.restore(state, cx);
                copy
            });
            self.track_view(&copy, window, cx);
            copy
        } else {
            if let Some(source) = source {
                self.panes[source].update(cx, |pane, cx| pane.remove(view, window, cx));
            }
            view.clone()
        };
        self.active_pane = target;
        self.panes[target].update(cx, |pane, cx| match index {
            Some(index) => pane.insert(index, item, window, cx),
            None => pane.add(item, window, cx),
        });
        self.refresh_menus(cx);
        cx.notify();
    }

    pub fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.active_pane().read(cx).active_item().cloned()
    }

    pub fn all_views(&self, cx: &App) -> Vec<Entity<EditorView>> {
        self.panes
            .iter()
            .flat_map(|pane| pane.read(cx).items().to_vec())
            .collect()
    }
}
