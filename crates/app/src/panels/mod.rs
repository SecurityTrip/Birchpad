//! Side panels, docked as in Notepad++: Folder as Workspace, the project panels, Document
//! List, Clipboard History and the Character Panel on the left; Function List and Document Map
//! on the right.
//!
//! Each side is a dock beside the documents, with a splitter. A dock with several open panels
//! shows them as tabs; the close button closes the panel shown. The View and Edit menus open
//! and close each panel, with a check mark on those open. The open panels are remembered in
//! `state.toml` and come back on the next start.

pub mod character_panel;
pub mod clipboard_history;
pub mod document_list;
pub mod document_map;
pub mod folder_workspace;
// Open Containing Folder > Terminal; tests only note it (`file_commands::Shell`).
#[cfg_attr(feature = "test-support", allow(unused_imports))]
pub use folder_workspace::open_terminal;
pub mod function_list;
pub mod json_viewer;
pub mod plugin_panel;
pub mod project;

use std::collections::HashMap;

use anyhow::{Result, bail};
use birchpad_commands::Invocation;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{IconName, Sizable};
use gpui_kit::component::{h_resizable, resizable_panel};
use gpui_kit::{AnyElement, AnyView, AppContext as _, Context, Window, div, prelude::*, px};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::Workspace;

pub use character_panel::CharacterPanel;
pub use clipboard_history::ClipboardHistory;
pub use document_list::DocumentList;
pub use document_map::DocumentMap;
pub use folder_workspace::FolderWorkspace;
pub use function_list::FunctionList;
pub use json_viewer::JsonViewer;
pub use plugin_panel::PluginPanel;
pub use project::ProjectPanel;

/// The project panels, as Notepad++'s Project Panel 1 to 3.
pub const PROJECT_PANELS: u8 = 3;

/// A side of the documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A panel that docks beside the documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    FolderWorkspace,
    /// Project Panel 1, 2 or 3.
    Project(u8),
    DocumentList,
    ClipboardHistory,
    CharacterPanel,
    FunctionList,
    DocumentMap,
    /// What a plugin or a script shows.
    PluginPanel,
    /// Plugins > JSON Viewer's tree.
    JsonViewer,
}

impl PanelKind {
    pub const ALL: [Self; 11] = [
        Self::FolderWorkspace,
        Self::Project(1),
        Self::Project(2),
        Self::Project(3),
        Self::DocumentList,
        Self::ClipboardHistory,
        Self::CharacterPanel,
        Self::FunctionList,
        Self::DocumentMap,
        Self::PluginPanel,
        Self::JsonViewer,
    ];

    /// The name kept in `state.toml`.
    pub fn id(self) -> &'static str {
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
            Self::PluginPanel => "plugin-panel",
            Self::JsonViewer => "json-viewer",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    pub fn title(self) -> &'static str {
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
            Self::PluginPanel => "Plugin Panel",
            Self::JsonViewer => "JSON Viewer",
        }
    }

    pub fn side(self) -> Side {
        match self {
            Self::FunctionList | Self::DocumentMap | Self::PluginPanel | Self::JsonViewer => {
                Side::Right
            }
            _ => Side::Left,
        }
    }

    /// The title in the interface's language: `panel.<id>`.
    pub fn translated_title(self) -> String {
        crate::i18n::get_or(&format!("panel.{}", self.id()), self.title())
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
            Self::PluginPanel => "panel.plugin",
            Self::JsonViewer => "panel.json-viewer",
        }
    }

    /// The command that opens and closes the panel.
    pub fn invocation(self) -> Invocation {
        match self {
            Self::Project(panel) => {
                Invocation::with_args(self.command(), serde_json::json!({ "panel": panel }))
            }
            _ => Invocation::new(self.command()),
        }
    }

    /// The panel `invocation` opens and closes, if it is one of their commands.
    pub fn of_invocation(invocation: &Invocation) -> Option<Self> {
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

pub fn register_commands(registry: &mut CommandRegistry) {
    for kind in PanelKind::ALL {
        if let PanelKind::Project(_) = kind {
            continue;
        }
        registry.workspace(kind.command(), move |this, (), window, cx| {
            this.toggle_panel(kind, window, cx);
            Ok(())
        });
    }
    registry.workspace("file.open-folder-as-workspace", |this, (), window, cx| {
        this.open_folder_as_workspace(window, cx);
        Ok(())
    });
    registry.workspace("panel.project", |this, args: ProjectArgs, window, cx| {
        let kind = project_panel(args.panel)?;
        this.toggle_panel(kind, window, cx);
        Ok(())
    });
}

pub fn project_panel(panel: u8) -> Result<PanelKind> {
    if !(1..=PROJECT_PANELS).contains(&panel) {
        bail!(
            "{}",
            t!(
                "panels.there-is-no-project-panel-they-are-numbered-1-to",
                panel = panel,
                PROJECT_PANELS = PROJECT_PANELS
            )
        );
    }
    Ok(PanelKind::Project(panel))
}

/// The open panels and the dock each is shown in.
#[derive(Default)]
pub struct Docks {
    /// Open panels, in the order they were opened.
    pub open: Vec<PanelKind>,
    /// The panel each side shows (left, right).
    shown: [Option<PanelKind>; 2],
    pub(crate) views: HashMap<PanelKind, AnyView>,
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

impl Docks {
    pub fn is_open(&self, kind: PanelKind) -> bool {
        self.open.contains(&kind)
    }

    /// The open panels of `side`, in the order they were opened.
    pub fn on_side(&self, side: Side) -> Vec<PanelKind> {
        self.open
            .iter()
            .copied()
            .filter(|kind| kind.side() == side)
            .collect()
    }

    pub fn shown(&self, side: Side) -> Option<PanelKind> {
        self.shown[side_index(side)]
    }

    pub fn view(&self, kind: PanelKind) -> Option<&AnyView> {
        self.views.get(&kind)
    }
}

impl Workspace {
    /// The View and Edit menu commands: opens the panel, or closes it if it is open.
    pub fn toggle_panel(&mut self, kind: PanelKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.docks.is_open(kind) {
            self.close_panel(kind, cx);
        } else {
            self.open_panel(kind, window, cx);
        }
    }

    /// Opens `kind`, or brings it forward in its dock.
    pub fn open_panel(&mut self, kind: PanelKind, window: &mut Window, cx: &mut Context<Self>) {
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

    pub fn close_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
        if !self.docks.is_open(kind) {
            return;
        }
        self.docks.open.retain(|open| *open != kind);
        let side = kind.side();
        if self.docks.shown(side) == Some(kind) {
            // The panel opened last on that side takes its place.
            self.docks.shown[side_index(side)] = self.docks.on_side(side).last().copied();
        }
        self.panels_changed(cx);
    }

    /// Shows `kind` in its dock: a click on its tab.
    pub fn show_panel(&mut self, kind: PanelKind, cx: &mut Context<Self>) {
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
        if remembers_panels(cx) {
            AppState::update_state(cx, |state, _| state.panels.open = open);
        }
        self.refresh_menus(cx);
        cx.notify();
    }

    /// Opens the panels that were open when Birchpad last quit.
    pub fn restore_panels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !remembers_panels(cx) {
            return;
        }
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
            PanelKind::FolderWorkspace => cx
                .new(|cx| FolderWorkspace::new(workspace, window, cx))
                .into(),
            PanelKind::ClipboardHistory => cx
                .new(|cx| ClipboardHistory::new(workspace, window, cx))
                .into(),
            PanelKind::CharacterPanel => cx
                .new(|cx| CharacterPanel::new(workspace, window, cx))
                .into(),
            PanelKind::PluginPanel => cx.new(|cx| PluginPanel::new(workspace, window, cx)).into(),
            PanelKind::JsonViewer => cx.new(|cx| JsonViewer::new(workspace, window, cx)).into(),
            PanelKind::Project(panel) => cx
                .new(|cx| ProjectPanel::new(panel, workspace, window, cx))
                .into(),
        }
    }

    /// The documents and search results, with the docks on either side.
    pub fn render_with_docks(&self, center: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let left = self.render_dock(Side::Left, cx);
        let right = self.render_dock(Side::Right, cx);
        if left.is_none() && right.is_none() {
            return center;
        }
        // One group per layout, so that each keeps its own sizes.
        let id = format!("docks-{}-{}", left.is_some(), right.is_some());
        let dock = |element: AnyElement| {
            resizable_panel()
                .size(px(300.))
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
                .children(
                    open.iter()
                        .map(|kind| Tab::new().label(kind.translated_title())),
                )
                .into_any_element()
        } else {
            div()
                .px_2()
                .flex_1()
                .child(shown.translated_title())
                .into_any_element()
        };
        let close = Button::new(("close-panel", side_index(side)))
            .small()
            .ghost()
            .icon(IconName::Close)
            .on_click(cx.listener(move |this, _, _, cx| this.close_panel(shown, cx)));
        let border = crate::theme::paint(crate::theme::ui().border);
        Some(
            div()
                .id(("dock", side_index(side)))
                .size_full()
                .flex()
                .flex_col()
                .bg(crate::theme::paint(crate::theme::ui().background))
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
                        .bg(crate::theme::paint(crate::theme::ui().surface))
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .pl_1()
                                .overflow_hidden()
                                .child(header),
                        )
                        .child(close),
                )
                .child(div().flex_1().min_h(px(0.)).child(view))
                .into_any_element(),
        )
    }
}

impl Workspace {
    /// Adds `folder` to Folder as Workspace, opening the panel.
    pub fn add_workspace_folder(
        &mut self,
        folder: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_panel(PanelKind::FolderWorkspace, window, cx);
        if let Some(panel) = self
            .docks
            .view(PanelKind::FolderWorkspace)
            .and_then(|view| view.clone().downcast::<FolderWorkspace>().ok())
        {
            panel.update(cx, |panel, cx| panel.add_root(folder.to_owned(), cx));
        }
    }

    /// Puts `text` into the active document as a paste would, and goes back to the text
    /// (Clipboard History, Character Panel).
    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let invocation =
            Invocation::with_args("edit.insert-text", serde_json::json!({ "text": text }));
        if let Err(error) = self.dispatch(&invocation, window, cx) {
            crate::workspace::report_error(&error, window, cx);
        }
        if let Some(view) = self.active_view(cx) {
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
    }

    /// File > Open Folder as Workspace...: the folder dialog.
    fn open_folder_as_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(folder) = crate::path_dialog::ask_folder(start, cx).await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.add_workspace_folder(&folder, window, cx);
            })
            .ok();
        })
        .detach();
    }
}

/// Redraws a panel whenever the active document's view changes: its caret, its scroll, its
/// text. Panels call [`FollowView::follow`] with the active view each time they draw.
#[derive(Default)]
pub struct FollowView {
    followed: Option<(gpui_kit::EntityId, gpui_kit::Subscription)>,
}

impl FollowView {
    pub fn follow<T: 'static>(
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

/// Whether a panel's menu item shows a check mark.
pub fn is_checked(docks: &Docks, invocation: &Invocation) -> Option<bool> {
    PanelKind::of_invocation(invocation).map(|kind| docks.is_open(kind))
}

/// Without a session (`-nosession`), panels are remembered only if Preferences >
/// Multi-Instance says so.
fn remembers_panels(cx: &gpui_kit::App) -> bool {
    let app = AppState::global(cx);
    !app.no_session || app.settings.instances.nosession_panels
}
