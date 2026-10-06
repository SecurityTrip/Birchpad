//! The menu bar on Windows and Linux, drawn in the window (macOS has its native one), used with
//! the mouse or the keyboard as Windows menus are:
//! - Alt pressed and released alone, or F10, moves the focus to the menu bar; again, or Esc,
//!   gives it back.
//! - Alt+letter opens the menu that letter is the mnemonic of (underlined while Alt is held or
//!   the keyboard is in use).
//! - The arrows move between menus and items and into submenus, Enter or Space chooses, Esc
//!   closes one level; a letter chooses the item it is the mnemonic of, or moves between the
//!   items that share it.
//!
//! Mnemonics are picked automatically: the first letter of a label, or of a later word, or any
//! later letter or digit that no earlier item in the same menu has. Notepad++'s menus get the
//! same ones: File, Edit, Search, View, E*n*coding, Language.
//!
//! The menus are those installed for the window (`GlobalState::set_app_menus`); a chosen command
//! is dispatched from where the focus was before the menu bar took it. The application calls
//! [`init`] once, puts a [`MenuBar`] at the top of its window, and passes the window's root
//! element through [`route_input`].

use std::collections::HashSet;
use std::ops::Range;

use gpui_kit::component::global_state::GlobalState;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    Action, Anchor, AnchoredPositionMode, AnyElement, App, AsKeystroke as _, Bounds, Context,
    ElementId, Entity, FocusHandle, HighlightStyle, KeyBinding, KeyDownEvent, Modifiers,
    MouseButton, OwnedMenu, OwnedMenuItem, Pixels, ScrollHandle, SharedString, StyledText,
    Subscription, UnderlineStyle, Window, actions, anchored, canvas, deferred, div, point,
    prelude::*, px, rgb,
};

actions!(
    menu_bar,
    [
        /// F10: the menu bar takes the focus, or gives it back.
        ToggleMenuBar,
        /// Alt released without anything else since it was pressed.
        AltReleased,
        SelectPrevious,
        SelectNext,
        SelectFirst,
        SelectLast,
        /// Left: out of a submenu, or the previous menu.
        SelectLeft,
        /// Right: into a submenu, or the next menu.
        SelectRight,
        Choose,
        Close,
    ]
);

const CONTEXT: &str = "MenuBar";
const ITEM_HEIGHT: Pixels = px(26.);
const BORDER: u32 = 0xd0d7de;
const SELECTED: u32 = 0xddf4ff;
const HOVERED: u32 = 0xeaeef2;
const MUTED: u32 = 0x8c959f;

/// Binds the menu bar's keys. Nothing on macOS, which has the native menu bar.
pub fn init(cx: &mut App) {
    if cfg!(target_os = "macos") {
        return;
    }
    cx.bind_keys([
        KeyBinding::new("alt", AltReleased, None),
        KeyBinding::new("f10", ToggleMenuBar, None),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(CONTEXT)),
        KeyBinding::new("left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("enter", Choose, Some(CONTEXT)),
        KeyBinding::new("space", Choose, Some(CONTEXT)),
        KeyBinding::new("escape", Close, Some(CONTEXT)),
    ]);
}

/// Routes to the menu bar what it needs from the whole window, wherever the focus is: Alt
/// pressed and released alone, F10, Alt+letter, and mouse clicks (Alt released after a click,
/// as in a rectangular selection, is not a tap). `root` is the window's root element.
pub fn route_input<E: InteractiveElement>(root: E, menu_bar: Entity<MenuBar>) -> E {
    let [modifiers, mouse, alt, toggle, keys] = std::array::from_fn(|_| menu_bar.clone());
    root.on_modifiers_changed(move |event, window, cx| {
        modifiers.update(cx, |bar, cx| {
            bar.modifiers_changed(event.modifiers, window, cx)
        });
    })
    .capture_any_mouse_down(move |_, _, cx| mouse.update(cx, |bar, _| bar.mouse_down()))
    .on_action(move |_: &AltReleased, window, cx| {
        alt.update(cx, |bar, cx| bar.alt_released(window, cx));
    })
    .on_action(move |_: &ToggleMenuBar, window, cx| {
        toggle.update(cx, |bar, cx| bar.toggle(window, cx));
    })
    .on_key_down(move |event: &KeyDownEvent, window, cx| {
        if event.keystroke.modifiers != Modifiers::alt() {
            return;
        }
        let mut chars = event.keystroke.key.chars();
        if let (Some(key), None) = (chars.next(), chars.next())
            && keys.update(cx, |bar, cx| bar.open_by_mnemonic(key, window, cx))
        {
            cx.stop_propagation();
        }
    })
}

/// The letter that opens or chooses an item from the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mnemonic {
    /// Where it is in the label (bytes).
    range: Range<usize>,
    /// Lowercase.
    key: char,
}

struct Item {
    label: SharedString,
    mnemonic: Option<Mnemonic>,
    kind: Kind,
}

enum Kind {
    Command {
        action: Box<dyn Action>,
        checked: bool,
        disabled: bool,
    },
    Separator,
    Submenu(Vec<Item>),
}

impl Item {
    fn is_separator(&self) -> bool {
        matches!(self.kind, Kind::Separator)
    }
}

struct Title {
    label: SharedString,
    mnemonic: Option<Mnemonic>,
    items: Vec<Item>,
}

/// What the menu bar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Closed,
    /// A menu's title is selected, the menu closed (from the keyboard).
    Selected(usize),
    /// A menu is open. `path` has the selected item of the menu and of each open submenu; a
    /// submenu is open when its item is selected and followed by another level.
    Open {
        menu: usize,
        path: Vec<Option<usize>>,
    },
}

pub struct MenuBar {
    titles: Vec<Title>,
    state: State,
    focus_handle: FocusHandle,
    /// Where the focus was before the menu bar took it: commands go there, and it gets the focus
    /// back.
    previous_focus: Option<FocusHandle>,
    /// Whether the keyboard is in use, which shows mnemonics.
    keyboard: bool,
    /// Alt alone is held: mnemonics are shown.
    alt_held: bool,
    /// Alt went down while the window was active and nothing else happened since, so releasing
    /// it toggles the menu bar. A click with Alt (a rectangular selection) or switching windows
    /// with Alt+Tab does not.
    alt_armed: bool,
    bounds: Bounds<Pixels>,
    scroll_handles: Vec<ScrollHandle>,
    _subscriptions: Vec<Subscription>,
}

impl MenuBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            // Something else took the focus.
            cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
                this.state = State::Closed;
                this.keyboard = false;
                this.previous_focus = None;
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.alt_armed = false;
                    this.alt_held = false;
                    this.close(window, cx);
                }
            }),
        ];
        let mut this = Self {
            titles: Vec::new(),
            state: State::Closed,
            focus_handle,
            previous_focus: None,
            keyboard: false,
            alt_held: false,
            alt_armed: false,
            bounds: Bounds::default(),
            scroll_handles: Vec::new(),
            _subscriptions: subscriptions,
        };
        this.reload(cx);
        this
    }

    /// Takes the menus installed for the window again (after recent files or check marks
    /// changed), keeping what is open where it still exists.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let menus: Vec<OwnedMenu> = GlobalState::global(cx).app_menus().to_vec();
        let labels: Vec<&str> = menus.iter().map(|menu| menu.name.as_ref()).collect();
        self.titles = mnemonics(&labels)
            .into_iter()
            .zip(&menus)
            .map(|(mnemonic, menu)| Title {
                label: menu.name.clone(),
                mnemonic,
                items: items(&menu.items),
            })
            .collect();
        self.state = match std::mem::replace(&mut self.state, State::Closed) {
            State::Selected(menu) if menu < self.titles.len() => State::Selected(menu),
            State::Open { menu, path } if menu < self.titles.len() => {
                let mut items = self.titles[menu].items.as_slice();
                let mut kept = Vec::new();
                for (level, selected) in path.iter().enumerate() {
                    let valid = selected.is_none_or(|index| {
                        items.get(index).is_some_and(|item| !item.is_separator())
                    });
                    if !valid {
                        kept.push(None);
                        break;
                    }
                    kept.push(*selected);
                    match selected.map(|index| &items[index].kind) {
                        Some(Kind::Submenu(submenu)) if level + 1 < path.len() => items = submenu,
                        _ => break,
                    }
                }
                State::Open { menu, path: kept }
            }
            _ => State::Closed,
        };
        cx.notify();
    }

    pub fn is_active(&self) -> bool {
        self.state != State::Closed
    }

    /// The window saw the modifiers change.
    fn modifiers_changed(&mut self, modifiers: Modifiers, window: &Window, cx: &mut Context<Self>) {
        let alt_alone = modifiers == Modifiers::alt();
        if alt_alone && !self.alt_held {
            self.alt_armed = window.is_window_active();
        } else if !alt_alone && modifiers.modified() {
            self.alt_armed = false;
        }
        if self.alt_held != alt_alone {
            self.alt_held = alt_alone;
            cx.notify();
        }
    }

    /// The window saw a mouse button go down.
    fn mouse_down(&mut self) {
        self.alt_armed = false;
    }

    /// Alt was released alone.
    fn alt_released(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.alt_armed) {
            self.toggle(window, cx);
        }
    }

    /// F10, or Alt alone: the menu bar takes the focus, or gives it back.
    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_active() {
            self.close(window, cx);
        } else if !self.titles.is_empty() {
            self.take_focus(window, cx);
            self.keyboard = true;
            self.state = State::Selected(0);
            cx.notify();
        }
    }

    /// Alt+letter while the menu bar does not have the focus: opens the menu with that
    /// mnemonic. Returns whether there is one.
    fn open_by_mnemonic(&mut self, key: char, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.title_with_mnemonic(key) else {
            return false;
        };
        self.keyboard = true;
        self.open(menu, Some(First), window, cx);
        true
    }

    fn title_with_mnemonic(&self, key: char) -> Option<usize> {
        let key = key.to_lowercase().next()?;
        self.titles.iter().position(|title| {
            title
                .mnemonic
                .as_ref()
                .is_some_and(|mnemonic| mnemonic.key == key)
        })
    }

    fn take_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) {
            self.previous_focus = window.focused(cx);
            self.focus_handle.focus(window, cx);
        }
    }

    /// Closes the menus and gives the focus back.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state == State::Closed && self.previous_focus.is_none() {
            return;
        }
        self.state = State::Closed;
        self.keyboard = false;
        if let Some(previous) = self.previous_focus.take()
            && self.focus_handle.is_focused(window)
        {
            previous.focus(window, cx);
        }
        cx.notify();
    }

    fn open(
        &mut self,
        menu: usize,
        select: Option<Select>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.take_focus(window, cx);
        let selected = select.and_then(|select| select.index(&self.titles[menu].items));
        self.state = State::Open {
            menu,
            path: vec![selected],
        };
        self.scroll_to_selection();
        cx.notify();
    }

    /// The items of the open menu at `level` of `path`.
    fn level_items<'a>(&'a self, menu: usize, path: &[Option<usize>], level: usize) -> &'a [Item] {
        let mut items = self.titles[menu].items.as_slice();
        for selected in &path[..level] {
            match selected.map(|index| &items[index].kind) {
                Some(Kind::Submenu(submenu)) => items = submenu,
                _ => return &[],
            }
        }
        items
    }

    fn scroll_to_selection(&mut self) {
        let State::Open { path, .. } = &self.state else {
            return;
        };
        while self.scroll_handles.len() < path.len() {
            self.scroll_handles.push(ScrollHandle::new());
        }
        if let (Some(Some(index)), Some(handle)) =
            (path.last(), self.scroll_handles.get(path.len() - 1))
        {
            handle.scroll_to_item(*index);
        }
    }

    /// Moves the selection in the innermost open menu.
    fn select(&mut self, select: Select, cx: &mut Context<Self>) {
        let State::Open { menu, path } = &self.state else {
            return;
        };
        let level = path.len() - 1;
        let items = self.level_items(*menu, path, level);
        let current = path[level];
        let next = match select {
            Select::Next => step(items, current, 1),
            Select::Previous => step(items, current, -1),
            First | Last => select.index(items),
        };
        if let State::Open { path, .. } = &mut self.state {
            path[level] = next;
        }
        self.scroll_to_selection();
        cx.notify();
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        match self.state {
            State::Selected(menu) => self.open(menu, Some(Last), window, cx),
            _ => self.select(Select::Previous, cx),
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        match self.state {
            State::Selected(menu) => self.open(menu, Some(First), window, cx),
            _ => self.select(Select::Next, cx),
        }
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select(First, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        self.select(Last, cx);
    }

    /// The menu `delta` places away from `menu`, around the ends.
    fn neighbor(&self, menu: usize, delta: isize) -> usize {
        (menu as isize + delta).rem_euclid(self.titles.len() as isize) as usize
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        match self.state.clone() {
            State::Closed => {}
            State::Selected(menu) => {
                self.state = State::Selected(self.neighbor(menu, -1));
                cx.notify();
            }
            State::Open { mut path, menu } if path.len() > 1 => {
                path.pop();
                self.state = State::Open { menu, path };
                cx.notify();
            }
            State::Open { menu, .. } => {
                let previous = self.neighbor(menu, -1);
                self.open(previous, Some(First), window, cx);
            }
        }
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        match self.state.clone() {
            State::Closed => {}
            State::Selected(menu) => {
                self.state = State::Selected(self.neighbor(menu, 1));
                cx.notify();
            }
            State::Open { menu, .. } => {
                if !self.enter_submenu(cx) {
                    let next = self.neighbor(menu, 1);
                    self.open(next, Some(First), window, cx);
                }
            }
        }
    }

    /// Opens the submenu selected in the innermost open menu. Returns whether there is one.
    fn enter_submenu(&mut self, cx: &mut Context<Self>) -> bool {
        let State::Open { menu, path } = &self.state else {
            return false;
        };
        let level = path.len() - 1;
        let Some(index) = path[level] else {
            return false;
        };
        let Kind::Submenu(submenu) = &self.level_items(*menu, path, level)[index].kind else {
            return false;
        };
        let first = First.index(submenu);
        if let State::Open { path, .. } = &mut self.state {
            path.push(first);
        }
        self.scroll_to_selection();
        cx.notify();
        true
    }

    fn choose(&mut self, _: &Choose, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard = true;
        match &self.state {
            State::Closed => {}
            State::Selected(menu) => self.open(*menu, Some(First), window, cx),
            State::Open { .. } => self.choose_selected(window, cx),
        }
    }

    /// Runs the command selected in the innermost open menu, or opens its submenu.
    fn choose_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let State::Open { menu, path } = &self.state else {
            return;
        };
        let level = path.len() - 1;
        let Some(index) = path[level] else {
            return;
        };
        match &self.level_items(*menu, path, level)[index].kind {
            Kind::Command {
                action,
                disabled: false,
                ..
            } => {
                let action = action.boxed_clone();
                self.close(window, cx);
                // Dispatched from the focus just given back.
                window.dispatch_action(action, cx);
            }
            Kind::Submenu(_) => {
                self.enter_submenu(cx);
            }
            Kind::Command { .. } | Kind::Separator => {}
        }
    }

    fn close_level(&mut self, _: &Close, window: &mut Window, cx: &mut Context<Self>) {
        match self.state.clone() {
            State::Closed => {}
            State::Selected(_) => self.close(window, cx),
            State::Open { menu, mut path } if path.len() > 1 => {
                path.pop();
                self.state = State::Open { menu, path };
                cx.notify();
            }
            State::Open { menu, .. } => {
                self.state = State::Selected(menu);
                self.keyboard = true;
                cx.notify();
            }
        }
    }

    fn toggle_action(&mut self, _: &ToggleMenuBar, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle(window, cx);
    }

    fn alt_released_action(
        &mut self,
        _: &AltReleased,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.alt_released(window, cx);
    }

    /// Letters choose by mnemonic.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.modified() && modifiers != Modifiers::alt() {
            return;
        }
        let mut chars = event.keystroke.key.chars();
        let (Some(key), None) = (chars.next(), chars.next()) else {
            return;
        };
        let Some(key) = key.to_lowercase().next() else {
            return;
        };
        cx.stop_propagation();
        self.keyboard = true;
        let (menu, path) = match &self.state {
            State::Closed => return,
            State::Selected(_) => {
                if let Some(menu) = self.title_with_mnemonic(key) {
                    self.open(menu, Some(First), window, cx);
                }
                return;
            }
            State::Open { path, .. } if modifiers == Modifiers::alt() && path.len() == 1 => {
                // Alt+letter in an open menu opens another menu, as from the editor.
                if let Some(menu) = self.title_with_mnemonic(key) {
                    self.open(menu, Some(First), window, cx);
                }
                return;
            }
            State::Open { menu, path } => (*menu, path.clone()),
        };
        let level = path.len() - 1;
        let items = self.level_items(menu, &path, level);
        let matching: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.mnemonic.as_ref().is_some_and(|m| m.key == key))
            .map(|(index, _)| index)
            .collect();
        let next = match matching.as_slice() {
            [] => return,
            [only] => Some(*only),
            several => {
                // Several items share the letter: move to the next one.
                let after = path[level].map_or(0, |current| current + 1);
                several
                    .iter()
                    .find(|&&index| index >= after)
                    .or(several.first())
                    .copied()
            }
        };
        if let State::Open { path, .. } = &mut self.state {
            path[level] = next;
        }
        if matching.len() == 1 {
            self.choose_selected(window, cx);
        } else {
            self.scroll_to_selection();
            cx.notify();
        }
    }

    fn click_title(&mut self, menu: usize, window: &mut Window, cx: &mut Context<Self>) {
        let open_here = matches!(&self.state, State::Open { menu: open, .. } if *open == menu);
        if open_here {
            self.close(window, cx);
        } else {
            self.keyboard = false;
            self.open(menu, None, window, cx);
        }
    }

    fn hover_title(&mut self, menu: usize, cx: &mut Context<Self>) {
        if matches!(&self.state, State::Open { menu: open, .. } if *open != menu) {
            self.state = State::Open {
                menu,
                path: vec![None],
            };
            cx.notify();
        }
    }

    fn hover_item(&mut self, level: usize, index: usize, cx: &mut Context<Self>) {
        let State::Open { menu, path } = &self.state else {
            return;
        };
        if level >= path.len() {
            return;
        }
        let submenu = matches!(
            self.level_items(*menu, path, level)[index].kind,
            Kind::Submenu(_)
        );
        if let State::Open { path, .. } = &mut self.state {
            if path[level] == Some(index) && path.len() > level + 1 {
                return;
            }
            path.truncate(level + 1);
            path[level] = Some(index);
            if submenu {
                path.push(None);
            }
        }
        cx.notify();
    }

    fn click_item(
        &mut self,
        level: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let State::Open { path, .. } = &mut self.state else {
            return;
        };
        if level >= path.len() {
            return;
        }
        path.truncate(level + 1);
        path[level] = Some(index);
        self.choose_selected(window, cx);
    }

    fn render_popup(
        &self,
        menu: usize,
        path: &[Option<usize>],
        level: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = self.level_items(menu, path, level);
        let show_mnemonics = self.keyboard || self.alt_held;
        let context = self.previous_focus.clone();
        let max_height = (window.viewport_size().height - px(16.)).max(ITEM_HEIGHT * 3.);
        let rows: Vec<_> =
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let selected = path[level] == Some(index);
                    let id = ElementId::Name(format!("menu-item-{level}-{index}").into());
                    let selector = move || format!("menu-item-{level}-{index}");
                    if item.is_separator() {
                        return div()
                            .id(id)
                            .debug_selector(selector)
                            .flex_none()
                            .h(px(1.))
                            .my_1()
                            .mx_2()
                            .bg(rgb(BORDER))
                            .into_any_element();
                    }
                    let (checked, disabled, shortcut, submenu) = match &item.kind {
                        Kind::Command {
                            action,
                            checked,
                            disabled,
                        } => (
                            *checked,
                            *disabled,
                            shortcut(
                                action.as_ref(),
                                context.as_ref(),
                                &self.focus_handle,
                                window,
                            ),
                            false,
                        ),
                        Kind::Submenu(_) => (false, false, None, true),
                        Kind::Separator => unreachable!(),
                    };
                    let open_submenu = selected && submenu && path.len() > level + 1;
                    div()
                        .id(id)
                        .debug_selector(selector)
                        .relative()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(ITEM_HEIGHT)
                        .mx_1()
                        .pl_1()
                        .pr_2()
                        .gap_2()
                        .rounded(px(4.))
                        .when(selected, |row| row.bg(rgb(SELECTED)))
                        .when(disabled, |row| row.text_color(rgb(MUTED)))
                        .child(
                            div()
                                .w(px(16.))
                                .flex_none()
                                .flex()
                                .justify_center()
                                .child(if checked { "✓" } else { "" }),
                        )
                        .child(div().flex_1().whitespace_nowrap().child(label(
                            &item.label,
                            item.mnemonic.as_ref(),
                            show_mnemonics,
                        )))
                        .when_some(shortcut, |row, shortcut| {
                            row.child(div().pl_6().text_color(rgb(MUTED)).child(shortcut))
                        })
                        .when(submenu, |row| {
                            row.child(div().text_color(rgb(MUTED)).child("›"))
                        })
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered {
                                this.hover_item(level, index, cx);
                            }
                        }))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.click_item(level, index, window, cx);
                            }),
                        )
                        .when(open_submenu, |row| {
                            row.child(
                                div().absolute().left_full().top(px(-5.)).child(
                                    deferred(anchored().snap_to_window_with_margin(px(8.)).child(
                                        self.render_popup(menu, path, level + 1, window, cx),
                                    ))
                                    .with_priority(3 + level),
                                ),
                            )
                        })
                        .into_any_element()
                })
                .collect();
        let scroll = self.scroll_handles.get(level).cloned().unwrap_or_default();
        div()
            .id(ElementId::Name(format!("menu-level-{level}").into()))
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(220.))
            .max_h(max_height)
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .py_1()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(6.))
            .shadow_md()
            .text_sm()
            .text_color(rgb(0x1f2328))
            .children(rows)
            .into_any_element()
    }
}

/// How to choose an item when a menu opens or the selection moves.
#[derive(Clone, Copy)]
enum Select {
    Next,
    Previous,
    First,
    Last,
}
use Select::{First, Last};

impl Select {
    fn index(self, items: &[Item]) -> Option<usize> {
        match self {
            Select::First | Select::Next => step(items, None, 1),
            Select::Last | Select::Previous => step(items, None, -1),
        }
    }
}

/// The next item from `current` in `direction`, around the ends, skipping separators.
fn step(items: &[Item], current: Option<usize>, direction: isize) -> Option<usize> {
    let len = items.len() as isize;
    let start = current.map_or(if direction > 0 { -1 } else { len }, |index| index as isize);
    (1..=len)
        .map(|offset| (start + direction * offset).rem_euclid(len) as usize)
        .find(|&index| !items[index].is_separator())
}

fn items(menu_items: &[OwnedMenuItem]) -> Vec<Item> {
    let labels: Vec<&str> = menu_items
        .iter()
        .map(|item| match item {
            OwnedMenuItem::Action { name, .. } => name.as_str(),
            OwnedMenuItem::Submenu(menu) => menu.name.as_ref(),
            OwnedMenuItem::Separator | OwnedMenuItem::SystemMenu(_) => "",
        })
        .collect();
    mnemonics(&labels)
        .into_iter()
        .zip(menu_items)
        .filter_map(|(mnemonic, item)| {
            let (label, kind) = match item {
                OwnedMenuItem::Action {
                    name,
                    action,
                    checked,
                    disabled,
                    ..
                } => (
                    SharedString::from(name.clone()),
                    Kind::Command {
                        action: action.boxed_clone(),
                        checked: *checked,
                        disabled: *disabled,
                    },
                ),
                OwnedMenuItem::Submenu(menu) => {
                    (menu.name.clone(), Kind::Submenu(items(&menu.items)))
                }
                OwnedMenuItem::Separator => (SharedString::default(), Kind::Separator),
                OwnedMenuItem::SystemMenu(_) => return None,
            };
            Some(Item {
                label,
                mnemonic,
                kind,
            })
        })
        .collect()
}

/// A mnemonic for each label (an empty label is a separator): the first letter of the label,
/// of a later word, or any later letter or digit, that no earlier label has.
fn mnemonics(labels: &[&str]) -> Vec<Option<Mnemonic>> {
    let mut used = HashSet::new();
    labels
        .iter()
        .map(|label| {
            let starts_word = |index: usize| {
                label[..index]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !before.is_alphanumeric())
            };
            let candidates = label.char_indices().filter(|(_, ch)| ch.is_alphanumeric());
            let (words, rest): (Vec<_>, Vec<_>) =
                candidates.partition(|&(index, _)| starts_word(index));
            let (index, ch) = words.into_iter().chain(rest).find(|(_, ch)| {
                ch.to_lowercase()
                    .next()
                    .is_some_and(|key| !used.contains(&key))
            })?;
            let key = ch.to_lowercase().next()?;
            used.insert(key);
            Some(Mnemonic {
                range: index..index + ch.len_utf8(),
                key,
            })
        })
        .collect()
}

/// A label with its mnemonic underlined when `show` is set.
fn label(text: &SharedString, mnemonic: Option<&Mnemonic>, show: bool) -> StyledText {
    let styled = StyledText::new(text.clone());
    match mnemonic {
        Some(mnemonic) if show => styled.with_highlights([(
            mnemonic.range.clone(),
            HighlightStyle {
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    color: None,
                    wavy: false,
                }),
                ..HighlightStyle::default()
            },
        )]),
        _ => styled,
    }
}

/// The key binding of a menu command, as seen from where the command will go.
fn shortcut(
    action: &dyn Action,
    context: Option<&FocusHandle>,
    menu_bar: &FocusHandle,
    window: &Window,
) -> Option<String> {
    let binding = context
        .into_iter()
        .chain([menu_bar])
        .find_map(|handle| window.highest_precedence_binding_for_action_in(action, handle))
        .or_else(|| window.highest_precedence_binding_for_action(action))?;
    let keys: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|keystroke| Kbd::format(keystroke.as_keystroke()))
        .collect();
    Some(keys.join(" "))
}

impl Render for MenuBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_mnemonics = self.keyboard || self.alt_held;
        let (current, open) = match &self.state {
            State::Closed => (None, None),
            State::Selected(menu) => (Some(*menu), None),
            State::Open { menu, path } => (Some(*menu), Some(path.clone())),
        };
        let entity = cx.entity();
        let titles: Vec<_> = self
            .titles
            .iter()
            .enumerate()
            .map(|(index, title)| {
                let popup = match &open {
                    Some(path) if current == Some(index) => {
                        Some(self.render_popup(index, path, 0, window, cx))
                    }
                    _ => None,
                };
                div()
                    .id(ElementId::Name(format!("menu-{index}").into()))
                    .relative()
                    .child(
                        div()
                            .id(ElementId::Name(format!("menu-title-{index}").into()))
                            .debug_selector(move || format!("menu-title-{index}"))
                            .px_2()
                            .py(px(3.))
                            .rounded(px(4.))
                            .text_sm()
                            .when(current == Some(index), |title| title.bg(rgb(SELECTED)))
                            .when(current != Some(index), |title| {
                                title.hover(|style| style.bg(rgb(HOVERED)))
                            })
                            .child(label(&title.label, title.mnemonic.as_ref(), show_mnemonics))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.click_title(index, window, cx);
                                }),
                            )
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                if *hovered {
                                    this.hover_title(index, cx);
                                }
                            })),
                    )
                    .when_some(popup, |this, popup| {
                        this.child(
                            deferred(
                                anchored()
                                    .anchor(Anchor::TopLeft)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(div().mt_1().child(popup)),
                            )
                            .with_priority(2),
                        )
                    })
            })
            .collect();
        // Below the menu bar, a click outside the menus closes them and goes nowhere else.
        let viewport = window.viewport_size();
        let overlay_top = self.bounds.bottom();
        let overlay = open.is_some().then(|| {
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Window)
                    .position(point(px(0.), overlay_top))
                    .child(
                        div()
                            .id("menu-overlay")
                            .occlude()
                            .w(viewport.width)
                            .h((viewport.height - overlay_top).max(px(0.)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| this.close(window, cx)),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, _, window, cx| this.close(window, cx)),
                            ),
                    ),
            )
            .with_priority(1)
        });
        div()
            .id("menu-bar")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::choose))
            .on_action(cx.listener(Self::close_level))
            .on_action(cx.listener(Self::toggle_action))
            .on_action(cx.listener(Self::alt_released_action))
            .on_key_down(cx.listener(Self::key_down))
            .relative()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_x_1()
            .child(
                canvas(
                    move |bounds, _, cx| entity.update(cx, |this, _| this.bounds = bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(titles)
            .children(overlay)
    }
}

// macOS has the native menu bar.
#[cfg(all(test, not(target_os = "macos")))]
mod gpui_tests {
    use gpui_kit::{Menu, MenuItem, TestAppContext, VisualTestContext, WindowOptions};

    use super::*;

    actions!(
        menu_bar_tests,
        [
            New,
            Open,
            ClearRecent,
            Exit,
            Undo,
            Redo,
            Find,
            WordWrap,
            Utf8
        ]
    );

    /// A window as Birchpad has it: the menu bar above an editor, which gets the commands.
    struct TestWindow {
        menu_bar: Entity<MenuBar>,
        editor: FocusHandle,
        chosen: Vec<&'static str>,
    }

    impl Render for TestWindow {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            route_input(div().id("window"), self.menu_bar.clone())
                .size_full()
                .flex()
                .flex_col()
                .child(div().flex_none().h(px(30.)).child(self.menu_bar.clone()))
                .child(
                    div()
                        .id("editor")
                        .track_focus(&self.editor)
                        .flex_1()
                        .on_action(cx.listener(|this, _: &New, _, _| this.chosen.push("New")))
                        .on_action(cx.listener(|this, _: &Undo, _, _| this.chosen.push("Undo")))
                        .on_action(cx.listener(|this, _: &Utf8, _, _| this.chosen.push("UTF-8"))),
                )
        }
    }

    /// Notepad++'s first menus, in short.
    fn menus() -> Vec<OwnedMenu> {
        [
            Menu::new("File").items([
                MenuItem::action("New", New),
                MenuItem::action("Open...", Open),
                MenuItem::submenu(
                    Menu::new("Recent Files")
                        .items([MenuItem::action("(empty)", ClearRecent).disabled(true)]),
                ),
                MenuItem::separator(),
                MenuItem::action("Exit", Exit),
            ]),
            Menu::new("Edit").items([
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
            ]),
            Menu::new("Search").items([MenuItem::action("Find...", Find)]),
            Menu::new("View").items([MenuItem::action("Word Wrap", WordWrap)]),
            Menu::new("Encoding").items([MenuItem::action("UTF-8", Utf8)]),
        ]
        .into_iter()
        .map(Menu::owned)
        .collect()
    }

    fn open_window(cx: &mut TestAppContext) -> (Entity<TestWindow>, &mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            init(cx);
            GlobalState::global_mut(cx).set_app_menus(menus());
        });
        let (window, root) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let editor = cx.focus_handle();
                    editor.focus(window, cx);
                    TestWindow {
                        menu_bar: cx.new(|cx| MenuBar::new(window, cx)),
                        editor,
                        chosen: Vec::new(),
                    }
                })
            })
            .expect("open the test window")
        });
        let cx = VisualTestContext::from_window(window, cx).into_mut();
        // Active like the window the user works in: Alt only arms the menu bar then.
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        (root, cx)
    }

    fn state(root: &Entity<TestWindow>, cx: &mut VisualTestContext) -> State {
        let bar = root.read_with(cx, |root, _| root.menu_bar.clone());
        bar.read_with(cx, |bar, _| bar.state.clone())
    }

    fn open(menu: usize, path: &[Option<usize>]) -> State {
        State::Open {
            menu,
            path: path.to_vec(),
        }
    }

    fn chosen(root: &Entity<TestWindow>, cx: &mut VisualTestContext) -> Vec<&'static str> {
        root.read_with(cx, |root, _| root.chosen.clone())
    }

    fn editor_focused(root: &Entity<TestWindow>, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, cx| root.read(cx).editor.is_focused(window))
    }

    /// Alt pressed and released alone.
    fn tap_alt(cx: &mut VisualTestContext) {
        cx.simulate_modifiers_change(Modifiers::alt());
        cx.simulate_keystrokes("alt");
        cx.simulate_modifiers_change(Modifiers::none());
    }

    #[gpui_kit::test]
    fn alt_and_the_arrows_walk_the_menus(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        tap_alt(cx);
        assert_eq!(state(&root, cx), State::Selected(0));
        cx.simulate_keystrokes("right");
        assert_eq!(state(&root, cx), State::Selected(1));
        cx.simulate_keystrokes("down");
        assert_eq!(state(&root, cx), open(1, &[Some(0)]), "Edit, on Undo");
        cx.simulate_keystrokes("up");
        assert_eq!(state(&root, cx), open(1, &[Some(1)]), "around the ends");
        cx.simulate_keystrokes("down");
        assert_eq!(state(&root, cx), open(1, &[Some(0)]));
        cx.simulate_keystrokes("left");
        assert_eq!(state(&root, cx), open(0, &[Some(0)]), "File, on New");
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), State::Selected(0));
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), State::Closed);
        assert!(editor_focused(&root, cx));
        assert!(chosen(&root, cx).is_empty());

        // F10 does the same; Alt gives the focus back.
        cx.simulate_keystrokes("f10");
        assert_eq!(state(&root, cx), State::Selected(0));
        assert!(!editor_focused(&root, cx));
        tap_alt(cx);
        assert_eq!(state(&root, cx), State::Closed);
        assert!(editor_focused(&root, cx));
    }

    #[gpui_kit::test]
    fn mnemonics_open_menus_and_choose_commands(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        cx.simulate_keystrokes("alt-f");
        assert_eq!(state(&root, cx), open(0, &[Some(0)]));
        cx.simulate_keystrokes("n");
        assert_eq!(state(&root, cx), State::Closed);
        assert_eq!(chosen(&root, cx), ["New"], "File > New, sent to the editor");
        assert!(editor_focused(&root, cx));

        // Enter chooses the selected item.
        cx.simulate_keystrokes("alt-e enter");
        assert_eq!(chosen(&root, cx), ["New", "Undo"]);

        // Encoding is E&ncoding: E is Edit's.
        cx.simulate_keystrokes("alt-n");
        assert_eq!(state(&root, cx), open(4, &[Some(0)]));
        cx.simulate_keystrokes("space");
        assert_eq!(chosen(&root, cx), ["New", "Undo", "UTF-8"]);
        assert_eq!(state(&root, cx), State::Closed);
    }

    #[gpui_kit::test]
    fn submenus_open_with_their_letter_or_right(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        // File > Recent Files.
        cx.simulate_keystrokes("alt-f r");
        assert_eq!(state(&root, cx), open(0, &[Some(2), Some(0)]));
        cx.simulate_keystrokes("left");
        assert_eq!(state(&root, cx), open(0, &[Some(2)]));
        cx.simulate_keystrokes("right");
        assert_eq!(state(&root, cx), open(0, &[Some(2), Some(0)]));
        // "(empty)" has no submenu: Right goes to the next menu.
        cx.simulate_keystrokes("right");
        assert_eq!(state(&root, cx), open(1, &[Some(0)]));

        // A disabled item does nothing.
        cx.simulate_keystrokes("left down down right enter");
        assert_eq!(state(&root, cx), open(0, &[Some(2), Some(0)]));
        assert!(chosen(&root, cx).is_empty());
    }

    #[gpui_kit::test]
    fn the_ends_of_the_bar_and_of_menus_wrap_around(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        tap_alt(cx);
        // From the first title left to the last, and right back.
        cx.simulate_keystrokes("left");
        assert_eq!(state(&root, cx), State::Selected(4));
        cx.simulate_keystrokes("right");
        assert_eq!(state(&root, cx), State::Selected(0));
        // Up on a title opens its menu on the last item, down on the first.
        cx.simulate_keystrokes("up");
        assert_eq!(state(&root, cx), open(0, &[Some(4)]), "File, on Exit");
        cx.simulate_keystrokes("home");
        assert_eq!(state(&root, cx), open(0, &[Some(0)]));
        cx.simulate_keystrokes("end");
        assert_eq!(state(&root, cx), open(0, &[Some(4)]));
        // The separator before Exit is skipped both ways.
        cx.simulate_keystrokes("up");
        assert_eq!(state(&root, cx), open(0, &[Some(2)]), "Recent Files");
        cx.simulate_keystrokes("down");
        assert_eq!(state(&root, cx), open(0, &[Some(4)]));
        cx.simulate_keystrokes("down");
        assert_eq!(state(&root, cx), open(0, &[Some(0)]), "around the end");
        // Left on the first menu goes to the last; Enter on a title opens it.
        cx.simulate_keystrokes("left");
        assert_eq!(state(&root, cx), open(4, &[Some(0)]));
        cx.simulate_keystrokes("escape enter");
        assert_eq!(state(&root, cx), open(4, &[Some(0)]));
    }

    #[gpui_kit::test]
    fn escape_closes_one_level_at_a_time(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        cx.simulate_keystrokes("alt-f r");
        assert_eq!(state(&root, cx), open(0, &[Some(2), Some(0)]));
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), open(0, &[Some(2)]));
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), State::Selected(0));
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), State::Closed);
        assert!(editor_focused(&root, cx));
        // Escape with the menu bar closed belongs to the editor.
        cx.simulate_keystrokes("escape");
        assert_eq!(state(&root, cx), State::Closed);
    }

    #[gpui_kit::test]
    fn keys_without_a_meaning_change_nothing(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        cx.simulate_keystrokes("alt-f");
        // A letter no item has, a key with Ctrl, a named key that is no letter.
        cx.simulate_keystrokes("z ctrl-n f5");
        assert_eq!(state(&root, cx), open(0, &[Some(0)]));
        assert!(chosen(&root, cx).is_empty());
        // On a selected title, a letter of no menu does nothing either.
        cx.simulate_keystrokes("escape z");
        assert_eq!(state(&root, cx), State::Selected(0));
        // Alt with a letter of no menu, from the editor.
        cx.simulate_keystrokes("escape escape alt-z");
        assert_eq!(state(&root, cx), State::Closed);
    }

    #[gpui_kit::test]
    fn alt_letter_in_an_open_menu_opens_another_and_f10_closes(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        cx.simulate_keystrokes("alt-f alt-e");
        assert_eq!(state(&root, cx), open(1, &[Some(0)]));
        cx.simulate_keystrokes("f10");
        assert_eq!(state(&root, cx), State::Closed);
        assert!(editor_focused(&root, cx));
        // Alt tapped with a menu open closes it too.
        cx.simulate_keystrokes("alt-v");
        assert_eq!(state(&root, cx), open(3, &[Some(0)]));
        tap_alt(cx);
        assert_eq!(state(&root, cx), State::Closed);
    }

    #[gpui_kit::test]
    fn the_mouse_moves_between_menus_and_chooses(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        let center = |selector: &'static str, cx: &mut VisualTestContext| {
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"))
                .center()
        };
        let file = center("menu-title-0", cx);
        cx.simulate_click(file, Modifiers::none());
        assert_eq!(state(&root, cx), open(0, &[None]));
        // Hovering another title moves the open menu there; hovering an item selects it.
        let edit = center("menu-title-1", cx);
        cx.simulate_mouse_move(edit, None, Modifiers::none());
        assert_eq!(state(&root, cx), open(1, &[None]));
        let redo = center("menu-item-0-1", cx);
        cx.simulate_mouse_move(redo, None, Modifiers::none());
        assert_eq!(state(&root, cx), open(1, &[Some(1)]));
        // A click on the open menu's title closes it.
        cx.simulate_click(edit, Modifiers::none());
        assert_eq!(state(&root, cx), State::Closed);

        // A submenu opens on hover, and a click on a command chooses it.
        cx.simulate_click(file, Modifiers::none());
        let recent = center("menu-item-0-2", cx);
        cx.simulate_mouse_move(recent, None, Modifiers::none());
        assert_eq!(state(&root, cx), open(0, &[Some(2), None]));
        let new = center("menu-item-0-0", cx);
        cx.simulate_mouse_move(new, None, Modifiers::none());
        assert_eq!(state(&root, cx), open(0, &[Some(0)]), "the submenu closes");
        cx.simulate_click(new, Modifiers::none());
        assert_eq!(state(&root, cx), State::Closed);
        assert_eq!(chosen(&root, cx), ["New"]);

        // A click on a separator or a disabled item chooses nothing.
        cx.simulate_click(file, Modifiers::none());
        let separator = center("menu-item-0-3", cx);
        cx.simulate_click(separator, Modifiers::none());
        assert_eq!(chosen(&root, cx), ["New"]);
        assert!(matches!(state(&root, cx), State::Open { menu: 0, .. }));
    }

    #[gpui_kit::test]
    fn reloading_the_menus_keeps_what_still_exists(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        let bar = root.read_with(cx, |root, _| root.menu_bar.clone());
        let reload = |menus: Vec<OwnedMenu>, cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                GlobalState::global_mut(cx).set_app_menus(menus);
                bar.update(cx, |bar, cx| bar.reload(cx));
            });
        };
        // The same menus: the open submenu stays open.
        cx.simulate_keystrokes("alt-f r");
        reload(menus(), cx);
        assert_eq!(state(&root, cx), open(0, &[Some(2), Some(0)]));

        // File without Recent Files: the selection is gone, the menu stays open.
        let mut fewer = menus();
        fewer[0] = Menu::new("File")
            .items([MenuItem::action("New", New), MenuItem::action("Exit", Exit)])
            .owned();
        reload(fewer, cx);
        assert_eq!(state(&root, cx), open(0, &[None]));

        // A selected title past the new end closes the menu bar.
        cx.simulate_keystrokes("escape left");
        assert_eq!(state(&root, cx), State::Selected(4));
        reload(menus().into_iter().take(2).collect(), cx);
        assert_eq!(state(&root, cx), State::Closed);
    }

    #[gpui_kit::test]
    fn alt_with_a_click_is_not_a_tap(cx: &mut TestAppContext) {
        let (root, cx) = open_window(cx);
        let inside = point(px(200.), px(200.));
        cx.simulate_modifiers_change(Modifiers::alt());
        cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::alt());
        cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::alt());
        cx.simulate_keystrokes("alt");
        cx.simulate_modifiers_change(Modifiers::none());
        assert_eq!(state(&root, cx), State::Closed, "a rectangular selection");

        // The menu bar opens with the mouse and closes with a click outside it.
        let title = point(px(20.), px(15.));
        cx.simulate_click(title, Modifiers::none());
        assert_eq!(state(&root, cx), open(0, &[None]));
        cx.simulate_click(inside, Modifiers::none());
        assert_eq!(state(&root, cx), State::Closed);
        assert!(editor_focused(&root, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(labels: &[&str]) -> String {
        mnemonics(labels)
            .into_iter()
            .map(|mnemonic| mnemonic.map_or('-', |mnemonic| mnemonic.key))
            .collect()
    }

    #[test]
    fn mnemonics_prefer_first_letters_of_words() {
        // Notepad++'s menus: E&ncoding, as Edit has E.
        assert_eq!(
            keys(&[
                "File", "Edit", "Search", "View", "Encoding", "Language", "Help"
            ]),
            "fesvnlh"
        );
        assert_eq!(keys(&["Save", "Save As...", "Save All", "Sort"]), "savo");
        assert_eq!(keys(&["", "1: C:\\a.txt", "2: C:\\b.txt"]), "-12");
        assert_eq!(keys(&["a", "A", "!"]), "a--");
        let cyrillic = mnemonics(&["Жук"]);
        assert_eq!(
            cyrillic[0].as_ref().map(|m| (m.range.clone(), m.key)),
            Some((0..2, 'ж'))
        );
    }
}
