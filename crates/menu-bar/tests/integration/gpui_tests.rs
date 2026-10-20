use gpui_kit::{
    IntoElement, Menu, MenuItem, Render, TestAppContext, VisualTestContext, WindowOptions,
};

use birchpad_menu_bar::*;
use gpui_kit::Bounds;
use gpui_kit::Context;
use gpui_kit::Entity;
use gpui_kit::FocusHandle;
use gpui_kit::Modifiers;
use gpui_kit::MouseButton;
use gpui_kit::OwnedMenu;
use gpui_kit::Pixels;
use gpui_kit::Window;
use gpui_kit::actions;
use gpui_kit::component::global_state::GlobalState;
use gpui_kit::div;
use gpui_kit::point;
use gpui_kit::prelude::*;
use gpui_kit::px;

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
    bar.read_with(cx, |bar, _| bar.state().clone())
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

/// The bounds of the menu bar's first title and of the open menu, after a click on the
/// title in a window `height` high.
fn open_file_in_window(
    height: f32,
    cx: &mut VisualTestContext,
) -> (Bounds<Pixels>, Bounds<Pixels>) {
    cx.simulate_resize(gpui_kit::size(px(640.), px(height)));
    cx.run_until_parked();
    let title = cx.debug_bounds("menu-title-0").expect("the title is drawn");
    cx.simulate_click(title.center(), Modifiers::none());
    let popup = cx.debug_bounds("menu-level-0").expect("the menu is drawn");
    (title, popup)
}

#[gpui_kit::test]
fn a_menu_in_a_tall_window_shows_all_its_items(cx: &mut TestAppContext) {
    let (root, cx) = open_window(cx);
    let (title, popup) = open_file_in_window(600., cx);
    assert!(popup.top() >= title.bottom());
    let last = cx.debug_bounds("menu-item-0-4").expect("Exit is drawn");
    assert!(last.bottom() <= popup.bottom(), "Exit is inside the menu");
    assert_eq!(state(&root, cx), open(0, &[None]));
}

#[gpui_kit::test]
fn a_menu_in_a_low_window_stays_below_the_bar(cx: &mut TestAppContext) {
    let (root, cx) = open_window(cx);
    let (title, popup) = open_file_in_window(110., cx);
    assert!(
        popup.top() >= title.bottom(),
        "the menu {popup:?} does not cover its title {title:?}"
    );
    assert!(popup.bottom() <= px(110.) - WINDOW_MARGIN + px(1.));
    // The click that opened it chose nothing, and the items scroll into view.
    assert_eq!(state(&root, cx), open(0, &[None]));
    assert!(chosen(&root, cx).is_empty());
    cx.simulate_keystrokes("up enter");
    assert_eq!(state(&root, cx), State::Closed, "Exit, scrolled to");
}

#[gpui_kit::test]
fn a_menu_in_a_window_lower_than_an_item_keeps_one_row(cx: &mut TestAppContext) {
    let (root, cx) = open_window(cx);
    let (title, popup) = open_file_in_window(40., cx);
    assert!(popup.size.height <= ITEM_HEIGHT + px(1.));
    assert!(popup.size.height > px(0.));
    assert!(title.bottom() <= px(40.));
    assert_eq!(state(&root, cx), open(0, &[None]));
    assert!(chosen(&root, cx).is_empty());
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
