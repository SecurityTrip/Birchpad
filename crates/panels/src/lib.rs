//! Birchpad's side panels that show the documents and put text into them: Document List,
//! Function List, Document Map, the JSON viewer, Clipboard History and the Character Panel.
//!
//! A panel knows the editor (`birchpad-editor`), not the window it is docked in: what it needs
//! of the window (the active document, the documents open, showing one, closing one, putting
//! text into the active one) it asks through [`Host`], which the application gives each panel.
//! The docks the panels sit in, and the panels that work with the window's files and plugins
//! (Folder as Workspace, the project panels, a plugin's panel), are the application's.

#[macro_use]
extern crate birchpad_i18n;

pub mod character_panel;
pub mod clipboard_history;
pub mod document_list;
pub mod document_map;
pub mod function_list;
pub mod json_viewer;

pub use character_panel::CharacterPanel;
pub use clipboard_history::ClipboardHistory;
pub use document_list::DocumentList;
pub use document_map::DocumentMap;
pub use function_list::FunctionList;
pub use json_viewer::JsonViewer;

use birchpad_editor::EditorView;
use gpui_kit::{App, Entity, Window};

/// The window a panel is docked in, as far as a panel needs it.
pub trait Host {
    /// The view of the active document, if there is one.
    fn active_view(&self, cx: &App) -> Option<Entity<EditorView>>;
    /// The tab the document peeker shows in the Document Map, while it shows one.
    fn peeked(&self, cx: &App) -> Option<Entity<EditorView>>;
    /// Every view open, in the order of the tabs, each with whether it is in the second view.
    fn views(&self, cx: &App) -> Vec<(Entity<EditorView>, bool)>;
    /// Shows `view` in its pane.
    fn activate(&self, view: &Entity<EditorView>, window: &mut Window, cx: &mut App);
    /// Closes `view`, asking about unsaved changes first.
    fn close(&self, view: &Entity<EditorView>, window: &mut Window, cx: &mut App);
    /// Puts `text` into the active document as a paste would, and goes back to the text.
    fn insert_text(&self, text: &str, window: &mut Window, cx: &mut App);
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
        view: Option<&Entity<EditorView>>,
        cx: &mut gpui_kit::Context<T>,
    ) {
        let id = view.map(|view| view.entity_id());
        if self.followed.as_ref().map(|(followed, _)| *followed) == id {
            return;
        }
        self.followed =
            view.map(|view| (view.entity_id(), cx.observe(view, |_, _, cx| cx.notify())));
    }
}
