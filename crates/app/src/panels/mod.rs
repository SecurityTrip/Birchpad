//! Side panels, docked as in Notepad++: Folder as Workspace, the project panels, Document
//! List, Clipboard History and the Character Panel on the left; Function List and Document Map
//! on the right.
//!
//! Each side is a dock beside the documents, with a splitter. A dock with several open panels
//! shows them as tabs; the close button closes the panel shown. The View and Edit menus open
//! and close each panel, with a check mark on those open. The open panels are remembered in
//! `state.toml` and come back on the next start.

mod document_list;
mod document_map;
mod function_list;

use std::collections::HashMap;

use anyhow::{Result, bail};
use birchpad_commands::Invocation;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{IconName, Sizable};
use gpui_kit::component::{h_resizable, resizable_panel};
use gpui_kit::{AnyElement, AnyView, AppContext as _, Context, Window, div, prelude::*, px, rgb};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::workspace::Workspace;

pub(crate) use document_list::DocumentList;
pub(crate) use document_map::DocumentMap;
pub(crate) use function_list::FunctionList;

/// The project panels, as Notepad++'s Project Panel 1 to 3.
pub(crate) const PROJECT_PANELS: u8 = 3;

/// A side of the documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

/// A panel that docks beside the documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PanelKind {
    FolderWorkspace,
    /// Project Panel 1, 2 or 3.
    Project(u8),
    DocumentList,
    ClipboardHistory,
    CharacterPanel,
    FunctionList,
    DocumentMap,
}

impl PanelKind {
    pub(crate) const ALL: [Self; 9] = [
        Self::FolderWorkspace,
        Self::Project(1),
        Self::Project(2),
        Self::Project(3),
        Self::DocumentList,
        Self::ClipboardHistory,
        Self::CharacterPanel,
        Self::FunctionList,
        Self::DocumentMap,
    ];

    /// The name kept in `state.toml`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::FolderWorkspace => "folder-as-workspace",
            Self::Project(1) => "project-1",
            Self::Project(2) => "project-2",
            Self::Project(_) => "project-3",
            Self::DocumentList => "document-list",
            Self::ClipboardHistory => "clipboard-history",
            Self::CharacterPanel => "character-panel",
            Self::FunctionList => "function-list",
            Self::DocumentMap => "document-map",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::FolderWorkspace => "Folder as Workspace",
            Self::Project(1) => "Project Panel 1",
            Self::Project(2) => "Project Panel 2",
            Self::Project(_) => "Project Panel 3",
            Self::DocumentList => "Document List",
            Self::ClipboardHistory => "Clipboard History",
            Self::CharacterPanel => "Character Panel",
            Self::FunctionList => "Function List",
            Self::DocumentMap => "Document Map",
        }
    }

    pub(crate) fn side(self) -> Side {
        match self {
            Self::FunctionList | Self::DocumentMap => Side::Right,
            _ => Side::Left,
        }
    }

    /// The id of the command that opens and closes the panel.
    fn command(self) -> &'static str {
        match self {
            Self::FolderWorkspace => "panel.folder-as-workspace",
            Self::Project(_) => "panel.project",
            Self::DocumentList => "panel.document-list",
            Self::ClipboardHistory => "panel.clipboard-history",
            Self::CharacterPanel => "panel.character-panel",
            Self::FunctionList => "panel.function-list",
            Self::DocumentMap => "panel.document-map",
        }
    }

    /// The command that opens and closes the panel.
    pub(crate) fn invocation(self) -> Invocation {
        match self {
            Self::Project(panel) => {
                Invocation::with_args(self.command(), serde_json::json!({ "panel": panel }))
            }
            _ => Invocation::new(self.command()),
        }
    }

    /// The panel `invocation` opens and closes, if it is one of their commands.
    pub(crate) fn of_invocation(invocation: &Invocation) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.invocation() == *invocation)
    }
}

/// `{ "panel": 1..3 }`.
#[derive(Deserialize)]
struct ProjectArgs {
    panel: u8,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    for kind in PanelKind::ALL {
        if let PanelKind::Project(_) = kind {
            continue;
        }
        registry.workspace(kind.command(), move |this, (), window, cx| {
            this.toggle_panel(kind, window, cx);
            Ok(())
        });
    }
    registry.workspace("panel.project", |this, args: ProjectArgs, window, cx| {
        let kind = project_panel(args.panel)?;
        this.toggle_panel(kind, window, cx);
        Ok(())
    });
}

fn project_panel(panel: u8) -> Result<PanelKind> {
    if !(1..=PROJECT_PANELS).contains(&panel) {
        bail!("there is no project panel {panel}; they are numbered 1 to {PROJECT_PANELS}");
    }
    Ok(PanelKind::Project(panel))
}

/// The open panels and the dock each is shown in.
#[derive(Default)]
pub(crate) struct Docks {
    /// Open panels, in the order they were opened.
    open: Vec<PanelKind>,
    /// The panel each side shows (left, right).
    shown: [Option<PanelKind>; 2],
    views: HashMap<PanelKind, AnyView>,
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

impl Docks {
    pub(crate) fn is_open(&self, kind: PanelKind) -> bool {
        self.open.contains(&kind)
    }

    /// The open panels of `side`, in the order they were opened.
    pub(crate) fn on_side(&self, side: Side) -> Vec<PanelKind> {
        self.open
            .iter()
            .copied()
            .filter(|kind| kind.side() == side)
            .collect()
    }

    pub(crate) fn shown(&self, side: Side) -> Option<PanelKind> {
        self.shown[side_index(side)]
    }

    pub(crate) fn view(&self, kind: PanelKind) -> Option<&AnyView> {
        self.views.get(&kind)
    }
}

impl Workspace {
    /// The View and Edit menu commands: opens the panel, or closes it if it is open.
    pub(crate) fn toggle_panel(
        &mut self,
        kind: PanelKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.docks.is_open(kind) {
            self.close_panel(kind, cx);
        } else {
            self.open_panel(kind, window, cx);
        }
    }

    /// Opens `kind`, or brings it forward in its dock.
    pub(crate) fn open_panel(
        &mut self,
        kind: PanelKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.docks.views.contains_key(&kind) {
            let view = self.new_panel(kind, window, cx);
            self.docks.views.insert(kind, view);
        }
        if !self.docks.is_open(kind) {
            self.docks.open.push(kind);
        }
        self.docks.shown[side_index(kind.side())] = Some(kind);
        self.panels_changed(cx);
    }

    pub(crate) fn close_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        self.docks.open.retain(|open| *open != kind);
        let side = kind.side();
        if self.docks.shown(side) == Some(kind) {
            // The panel opened last on that side takes its place.
            self.docks.shown[side_index(side)] = self.docks.on_side(side).last().copied();
        }
        self.panels_changed(cx);
    }

    /// Shows `kind` in its dock: a click on its tab.
    fn show_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        if self.docks.is_open(kind) {
            self.docks.shown[side_index(kind.side())] = Some(kind);
            cx.notify();
        }
    }

    fn panels_changed(&mut self, cx: &mut Context<Self>) {
        let open: Vec<String> = self
            .docks
            .open
            .iter()
            .map(|kind| kind.id().to_owned())
            .collect();
        AppState::update_state(cx, |state, _| state.panels.open = open);
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Opens the panels that were open when Birchpad last quit.
    pub(crate) fn restore_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = AppState::global(cx).state.panels.open.clone();
        for kind in ids.iter().filter_map(|id| PanelKind::from_id(id)) {
            self.open_panel(kind, window, cx);
        }
    }

    fn new_panel(
        &mut self,
        kind: PanelKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let workspace = cx.entity().downgrade();
        match kind {
            PanelKind::DocumentList => cx.new(|cx| DocumentList::new(workspace, window, cx)).into(),
            PanelKind::FunctionList => cx.new(|cx| FunctionList::new(workspace, window, cx)).into(),
            PanelKind::DocumentMap => cx.new(|cx| DocumentMap::new(workspace, window, cx)).into(),
            // Panels still to come show their name.
            _ => cx.new(|_| Placeholder(kind.title())).into(),
        }
    }

    /// The documents and search results, with the docks on either side.
    pub(crate) fn render_with_docks(
        &self,
        center: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let left = self.render_dock(Side::Left, cx);
        let right = self.render_dock(Side::Right, cx);
        if left.is_none() && right.is_none() {
            return center;
        }
        // One group per layout, so that each keeps its own sizes.
        let id = format!("docks-{}-{}", left.is_some(), right.is_some());
        let dock = |element: AnyElement| {
            resizable_panel()
                .size(px(260.))
                .size_range(px(120.)..px(1200.))
                .child(element)
        };
        h_resizable(id)
            .when_some(left, |group, left| group.child(dock(left)))
            .child(resizable_panel().child(center))
            .when_some(right, |group, right| group.child(dock(right)))
            .into_any_element()
    }

    fn render_dock(&self, side: Side, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown = self.docks.shown(side)?;
        let view = self.docks.view(shown)?.clone();
        let open = self.docks.on_side(side);
        let header = if open.len() > 1 {
            let selected = open.iter().position(|kind| *kind == shown).unwrap_or(0);
            let kinds = open.clone();
            TabBar::new(("dock-tabs", side_index(side)))
                .small()
                .underline()
                .selected_index(selected)
                .on_click(cx.listener(move |this, index: &usize, _, cx| {
                    if let Some(kind) = kinds.get(*index) {
                        this.show_panel(*kind, cx);
                    }
                }))
                .children(open.iter().map(|kind| Tab::new().label(kind.title())))
                .into_any_element()
        } else {
            div()
                .px_2()
                .flex_1()
                .child(shown.title())
                .into_any_element()
        };
        let close = Button::new(("close-panel", side_index(side)))
            .small()
            .ghost()
            .icon(IconName::Close)
            .on_click(cx.listener(move |this, _, _, cx| this.close_panel(shown, cx)));
        let border = rgb(0xd0d7de);
        Some(
            div()
                .id(("dock", side_index(side)))
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(0xffffff))
                .text_size(px(13.))
                .map(|dock| match side {
                    Side::Left => dock.border_r_1().border_color(border),
                    Side::Right => dock.border_l_1().border_color(border),
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .flex_none()
                        .h(px(30.))
                        .bg(rgb(0xf6f8fa))
                        .border_b_1()
                        .border_color(border)
                        .child(div().flex_1().min_w(px(0.)).overflow_hidden().child(header))
                        .child(close),
                )
                .child(div().flex_1().min_h(px(0.)).child(view))
                .into_any_element(),
        )
    }
}

/// Redraws a panel whenever the active document's view changes: its caret, its scroll, its
/// text. Panels call [`FollowView::follow`] with the active view each time they draw.
#[derive(Default)]
pub(crate) struct FollowView {
    followed: Option<(gpui_kit::EntityId, gpui_kit::Subscription)>,
}

impl FollowView {
    pub(crate) fn follow<T: 'static>(
        &mut self,
        view: Option<&gpui_kit::Entity<crate::editor::EditorView>>,
        cx: &mut Context<T>,
    ) {
        let id = view.map(|view| view.entity_id());
        if self.followed.as_ref().map(|(followed, _)| *followed) == id {
            return;
        }
        self.followed =
            view.map(|view| (view.entity_id(), cx.observe(view, |_, _, cx| cx.notify())));
    }
}

/// A panel not written yet.
struct Placeholder(&'static str);

impl Render for Placeholder {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p_2().text_color(rgb(0x57606a)).child(self.0)
    }
}

/// Whether a panel's menu item shows a check mark.
pub(crate) fn is_checked(docks: &Docks, invocation: &Invocation) -> Option<bool> {
    PanelKind::of_invocation(invocation).map(|kind| docks.is_open(kind))
}

#[cfg(test)]
mod tests;
