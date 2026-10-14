//! What the editor asks of the application it is part of, through what it exposes rather than
//! the application's globals: the application sets [`EditorHooks`] (macro recording, links, the
//! right-click menu, user defined languages, spelling) and [`ActiveFindOptions`] (the find
//! panel's options, for smart highlighting). Without them the editor works on its own: nothing
//! is recorded or spelled, no user language is known, links do not open.

use std::ops::Range;
use std::rc::Rc;

use birchpad_commands::Invocation;
use birchpad_syntax::{UdlKind, UserLanguage};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Context, EntityId, Global, WeakEntity, Window};

use crate::EditorView;
use crate::buffer::Buffer;
use crate::theme::TextStyle;

/// Ranges of a buffer's text, shared with the frame that draws them.
pub type Ranges = Rc<Vec<Range<usize>>>;

/// The application's part in the editor.
#[derive(Clone, Copy)]
pub struct EditorHooks {
    /// A command ran in an editor (for the macro being recorded).
    pub ran: fn(&Invocation, &mut App),
    /// Text was typed in an editor (for the macro being recorded).
    pub typed: fn(&str, &mut App),
    /// Opens a link the user clicked, in the browser.
    pub open_link: fn(String, &mut App),
    /// The right-click menu of an editor.
    pub context_menu:
        fn(PopupMenu, &WeakEntity<EditorView>, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
    /// The user defined language of a file, by its name.
    pub user_language_for_file: fn(&str, &App) -> Option<Rc<UserLanguage>>,
    /// How a kind of text of a user defined language looks.
    pub user_language_style: fn(&UserLanguage, UdlKind) -> Option<TextStyle>,
    /// The misspelled words in `range` of a buffer (the view's id keys a cache).
    pub misspelled: fn(&Buffer, EntityId, Range<usize>, &App) -> Ranges,
}

impl Default for EditorHooks {
    fn default() -> Self {
        Self {
            ran: |_, _| {},
            typed: |_, _| {},
            open_link: |_, _| {},
            context_menu: |menu, _, _, _| menu,
            user_language_for_file: |_, _| None,
            user_language_style: |_, _| None,
            misspelled: |_, _, _, _| Rc::default(),
        }
    }
}

impl Global for EditorHooks {}

impl EditorHooks {
    /// The hooks in effect: the application's, or the defaults.
    pub fn get(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }
}

/// The find panel's options, for smart highlighting with `highlighting.smart.use-find-options`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActiveFindOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

impl Global for ActiveFindOptions {}
