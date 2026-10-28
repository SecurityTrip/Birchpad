use std::cell::RefCell;
use std::rc::Rc;

use birchpad_quick_pick::{OPENED, PAGE, PickItem, QuickPick, highlight_ranges, open};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
    Window, WindowOptions, div,
};

/// The window's root: nothing but room for the dialog.
struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn items(texts: &[&str]) -> Vec<PickItem> {
    texts.iter().map(|text| PickItem::new(*text, "")).collect()
}

/// Opens the list of `texts` in a window, the first `recent` recently used; also returns what
/// picking picked.
fn open_list<'a>(
    texts: &[&str],
    recent: usize,
    cx: &'a mut TestAppContext,
) -> (
    Entity<QuickPick>,
    Rc<RefCell<Vec<usize>>>,
    &'a mut VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let (window, _) = cx
        .update(|cx| gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Blank)))
        .expect("open the test window");
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    let picked = Rc::new(RefCell::new(Vec::new()));
    let on_pick = {
        let picked = picked.clone();
        Rc::new(move |index, _: &mut Window, _: &mut gpui_kit::App| picked.borrow_mut().push(index))
    };
    let items = items(texts);
    let pick = cx.update(|window, cx| {
        open(
            "Pick".into(),
            "Type".into(),
            "Nothing matches".into(),
            items,
            recent,
            on_pick,
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (pick, picked, cx)
}

fn type_query(pick: &Entity<QuickPick>, text: &str, cx: &mut VisualTestContext) {
    let input = pick.read_with(cx, |pick, _| pick.input.clone());
    input.update_in(cx, |input, window, cx| input.set_value(text, window, cx));
    pick.update(cx, |pick, cx| pick.refilter(cx));
    cx.run_until_parked();
}

fn shown(pick: &Entity<QuickPick>, cx: &mut VisualTestContext) -> Vec<String> {
    pick.read_with(cx, |pick, _| {
        pick.shown
            .iter()
            .map(|(index, _)| pick.items()[*index].text.clone())
            .collect()
    })
}

fn selected(pick: &Entity<QuickPick>, cx: &mut VisualTestContext) -> Option<String> {
    pick.read_with(cx, |pick, _| {
        pick.selected_item().map(|item| item.text.clone())
    })
}

#[gpui_kit::test]
fn every_item_is_shown_until_something_is_typed(cx: &mut TestAppContext) {
    let (pick, _, cx) = open_list(&["Save", "Open", "Close"], 0, cx);
    assert_eq!(shown(&pick, cx), ["Save", "Open", "Close"]);
    assert_eq!(selected(&pick, cx).as_deref(), Some("Save"));
    let opened = OPENED
        .with(|opened| opened.borrow().clone())
        .and_then(|pick| pick.upgrade());
    assert_eq!(
        opened,
        Some(pick.clone()),
        "tests can find the list last opened"
    );

    type_query(&pick, "os", cx);
    assert_eq!(shown(&pick, cx), ["Close"]);
    assert_eq!(selected(&pick, cx).as_deref(), Some("Close"));
}

#[gpui_kit::test]
fn nothing_is_picked_when_nothing_matches(cx: &mut TestAppContext) {
    let (pick, picked, cx) = open_list(&["Save", "Open"], 0, cx);
    type_query(&pick, "xyz", cx);
    assert!(shown(&pick, cx).is_empty());
    assert_eq!(selected(&pick, cx), None);
    assert_eq!(
        pick.read_with(cx, |pick, _| pick.empty().to_owned()),
        "Nothing matches"
    );
    // Moving and picking do nothing.
    pick.update(cx, |pick, cx| pick.move_by(1, cx));
    pick.update_in(cx, |pick, window, cx| pick.pick(window, cx));
    cx.run_until_parked();
    assert!(picked.borrow().is_empty());
}

#[gpui_kit::test]
fn picking_closes_the_list_and_gives_the_item_s_index(cx: &mut TestAppContext) {
    let (pick, picked, cx) = open_list(&["Save", "Open", "Close"], 0, cx);
    type_query(&pick, "op", cx);
    pick.update_in(cx, |pick, window, cx| pick.pick(window, cx));
    cx.run_until_parked();
    // The index in the items, not in what is shown.
    assert_eq!(*picked.borrow(), [1]);
}

#[gpui_kit::test]
fn moving_stops_at_the_first_and_the_last_row(cx: &mut TestAppContext) {
    let texts: Vec<String> = (0..30).map(|n| format!("item {n:02}")).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let (pick, _, cx) = open_list(&texts, 0, cx);
    let row = |cx: &mut VisualTestContext| pick.read_with(cx, |pick, _| pick.selected);

    pick.update(cx, |pick, cx| pick.move_by(-1, cx));
    assert_eq!(row(cx), 0, "above the first row");
    pick.update(cx, |pick, cx| pick.move_by(PAGE as isize, cx));
    assert_eq!(row(cx), PAGE);
    pick.update(cx, |pick, cx| pick.move_by(1000, cx));
    assert_eq!(row(cx), 29, "past the last row");
    pick.update(cx, |pick, cx| pick.move_by(-1, cx));
    assert_eq!(row(cx), 28);
}

#[gpui_kit::test]
fn recently_used_items_come_first(cx: &mut TestAppContext) {
    let (pick, _, cx) = open_list(&["Find Next", "Find", "Replace"], 1, cx);
    type_query(&pick, "find", cx);
    assert_eq!(shown(&pick, cx), ["Find Next", "Find"]);
}

#[gpui_kit::test]
fn without_recent_items_the_best_match_comes_first(cx: &mut TestAppContext) {
    let (pick, _, cx) = open_list(&["Find Next", "Find", "Replace"], 0, cx);
    type_query(&pick, "find", cx);
    assert_eq!(shown(&pick, cx), ["Find", "Find Next"]);
}

#[gpui_kit::test]
fn new_items_keep_the_query_and_the_chosen_item(cx: &mut TestAppContext) {
    let (pick, _, cx) = open_list(&["a.txt", "b.txt"], 0, cx);
    type_query(&pick, "txt", cx);
    pick.update(cx, |pick, cx| pick.move_by(1, cx));
    assert_eq!(selected(&pick, cx).as_deref(), Some("b.txt"));
    pick.update(cx, |pick, cx| {
        pick.set_items(items(&["0.txt", "a.txt", "b.txt"]), 0, cx)
    });
    assert_eq!(shown(&pick, cx), ["0.txt", "a.txt", "b.txt"]);
    assert_eq!(selected(&pick, cx).as_deref(), Some("b.txt"));
    // The chosen item gone: the first row.
    pick.update(cx, |pick, cx| pick.set_items(items(&["c.txt"]), 0, cx));
    assert_eq!(selected(&pick, cx).as_deref(), Some("c.txt"));
}

#[test]
fn highlights_cover_whole_characters() {
    assert_eq!(highlight_ranges("Ёж s", &[0, 2, 5]), [0..2, 2..4, 5..6]);
    // Past the end or inside a character: left out.
    assert_eq!(
        highlight_ranges("Ёж", &[1, 4, 100]),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert!(highlight_ranges("", &[]).is_empty());
}
