use birchpad_core::{Range, Selection};
use gpui_kit::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
    VisualTestContext, point,
};

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{document_start, open_workspace};

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

/// The smart highlights of the last frame.
fn highlights(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<ByteRange<usize>> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| {
        view.smart_highlight
            .as_ref()
            .map(|smart| smart.matches.iter().map(|(range, _)| range).collect())
            .unwrap_or_default()
    })
}

fn select(workspace: &Entity<Workspace>, range: ByteRange<usize>, cx: &mut VisualTestContext) {
    let view = view(workspace, cx);
    view.update(cx, |view, cx| {
        view.selection = Selection::single(Range::new(range.start, range.end));
        cx.notify();
    });
    cx.run_until_parked();
}

fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut birchpad_config::Settings)) {
    cx.update(|window, cx| {
        change(&mut cx.global_mut::<AppState>().settings);
        window.refresh();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn double_clicking_a_word_highlights_its_occurrences(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("cat dog cat\ncatalog Cat cat");
    cx.simulate_keystrokes(document_start());
    // Double click inside the first "cat".
    let view = view(&workspace, cx);
    let position = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("a frame was drawn");
        let row = &layout.rows[0];
        point(
            row.x_for(1).unwrap(),
            row.y + layout.metrics.line_height / 2.,
        )
    });
    let (button, modifiers) = (MouseButton::Left, Modifiers::none());
    for click_count in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position,
            modifiers,
            button,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position,
            modifiers,
            button,
            click_count,
        });
    }
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.selection.primary()),
        Range::new(0, 3)
    );
    // Whole words, ignoring case, by default.
    assert_eq!(highlights(&workspace, cx), [0..3, 8..11, 20..23, 24..27]);

    // Typing replaces the selection: nothing is selected, nothing highlighted.
    cx.simulate_input("x");
    assert!(highlights(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn settings_choose_what_is_highlighted(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("cat dog cat\ncatalog Cat cat");
    // Part of a word: only with whole word off, then anywhere.
    select(&workspace, 12..15, cx);
    assert!(highlights(&workspace, cx).is_empty());
    settings(cx, |s| s.highlighting.smart.whole_word = false);
    assert_eq!(
        highlights(&workspace, cx),
        [0..3, 8..11, 12..15, 20..23, 24..27]
    );
    settings(cx, |s| s.highlighting.smart.match_case = true);
    assert_eq!(highlights(&workspace, cx), [0..3, 8..11, 12..15, 24..27]);
    // Text over two lines is never highlighted.
    select(&workspace, 8..15, cx);
    assert!(highlights(&workspace, cx).is_empty());

    // The find panel's options instead (match case and whole word off by default).
    select(&workspace, 20..23, cx);
    settings(cx, |s| s.highlighting.smart.use_find_options = true);
    assert_eq!(
        highlights(&workspace, cx),
        [0..3, 8..11, 12..15, 20..23, 24..27]
    );

    settings(cx, |s| s.highlighting.smart.enabled = false);
    assert!(highlights(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn large_files_are_not_smart_highlighted(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let text = "word other word\n".repeat(10);
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| view.insert(&text, crate::LastEdit::None, cx));
    select(&workspace, 0..4, cx);
    assert_eq!(highlights(&workspace, cx).len(), 20);
    settings(cx, |s| s.files.large_file_limit_mb = 0);
    assert!(highlights(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn selections_up_to_the_longest_token_are_highlighted(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |s| s.highlighting.smart.whole_word = false);
    let token = "y".repeat(MAX_TOKEN + 1);
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| {
        view.insert(&token, crate::LastEdit::None, cx)
    });
    let query = |range: ByteRange<usize>, cx: &mut VisualTestContext| {
        view.update(cx, |view, cx| {
            view.selection = Selection::single(Range::new(range.start, range.end));
            let text = view.text(cx).clone();
            view.smart_highlight_query(&text, cx)
                .map(|query| query.pattern.len())
        })
    };
    assert_eq!(
        query(0..MAX_TOKEN, cx),
        Some(MAX_TOKEN),
        "the longest token"
    );
    assert_eq!(query(0..MAX_TOKEN + 1, cx), None, "a byte more");
    assert_eq!(query(3..3, cx), None, "nothing selected");
    assert_eq!(query(3..4, cx), Some(1));
}

#[gpui_kit::test]
fn a_search_is_reused_while_it_covers_the_shown_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let text = "word other word
"
    .repeat(20_000);
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| view.insert(&text, crate::LastEdit::None, cx));
    select(&workspace, 0..4, cx);
    let rope = view.read_with(cx, |view, cx| view.text(cx).clone());
    let finish = |shown: &[ByteRange<usize>], cx: &mut VisualTestContext| {
        // A frame searches for at most its budget; the rest is left to the next ones.
        let mut frames = 0;
        while view.update(cx, |view, cx| view.update_smart_highlight(shown, &rope, cx)) {
            frames += 1;
            assert!(frames < 10_000, "the search ends");
        }
        view.read_with(cx, |view, _| {
            let smart = view.smart_highlight.as_ref().unwrap();
            (smart.matches.iter().count(), smart.scans.len())
        })
    };
    // The whole text shown at once: two words a line.
    assert_eq!(finish(&[0..rope.len()], cx), (40_000, 1));
    // Less of it is answered from the same result, without a new search.
    assert_eq!(finish(&[16..32], cx), (40_000, 1));
    // An edit is a new revision: searched again, around the shown text only.
    view.update(cx, |view, cx| {
        view.selection = Selection::single(Range::new(rope.len(), rope.len()));
        view.insert("word", crate::LastEdit::None, cx);
        view.selection = Selection::single(Range::new(0, 4));
    });
    let rope = view.read_with(cx, |view, cx| view.text(cx).clone());
    let (found, scans) = view.update(cx, |view, cx| {
        while view.update_smart_highlight(&[160_000..160_016], &rope, cx) {}
        let smart = view.smart_highlight.as_ref().unwrap();
        (smart.matches.iter().count(), smart.scans.len())
    });
    assert_eq!(scans, 1);
    assert!((1..40_000).contains(&found), "{found}");
}

#[test]
fn search_ranges_keep_to_characters() {
    // Two-byte characters: a margin never ends inside one.
    let text = Rope::from_str(&"é".repeat(MARGIN));
    let ranges = search_ranges(&[MARGIN + 1..MARGIN + 3], &text);
    assert_eq!(ranges.len(), 1);
    let range = ranges[0].clone();
    assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
    assert_eq!(range, 0..text.len());
    // Nothing shown, nothing searched.
    assert!(search_ranges(&[], &text).is_empty());
    // Ranges that only touch once extended are merged.
    let text = Rope::from_str(&"x".repeat(10 * MARGIN));
    assert_eq!(
        search_ranges(&[3 * MARGIN..3 * MARGIN, 5 * MARGIN..5 * MARGIN], &text),
        [2 * MARGIN..6 * MARGIN]
    );
    assert_eq!(
        search_ranges(
            &[3 * MARGIN..3 * MARGIN, 5 * MARGIN + 1..5 * MARGIN + 1],
            &text
        ),
        [2 * MARGIN..4 * MARGIN, 4 * MARGIN + 1..6 * MARGIN + 1]
    );
}

#[test]
fn search_ranges_extend_and_merge() {
    let text = Rope::from_str(&"x".repeat(100_000));
    assert_eq!(
        search_ranges(&[50_000..50_010, 60_000..60_010, 99_000..99_500], &text),
        [50_000 - MARGIN..60_010 + MARGIN, 99_000 - MARGIN..100_000]
    );
    assert_eq!(search_ranges(&[10..20], &text), [0..20 + MARGIN]);
}

/// `cargo test -p birchpad-editor --release smart_highlighting_cost -- --ignored --nocapture`
#[gpui_kit::test]
#[ignore = "performance check; run in release"]
fn smart_highlighting_cost(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let line = "let value = compute(value, other_value) + value * 2; // value again value\n";
    let text = line.repeat(10_000);
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| view.insert(&text, crate::LastEdit::None, cx));
    // Scroll to the middle and select a word that occurs six times per line.
    let middle = text.len() / 2;
    let start = middle - middle % line.len() + 4;
    select(&workspace, start..start + 5, cx);
    let shown = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        let first = layout.rows.first().unwrap().row.range.start;
        let last = layout.rows.last().unwrap().row.range.end;
        vec![first..last]
    });
    let found = highlights(&workspace, cx).len();
    let rope = view.read_with(cx, |view, cx| view.text(cx).clone());

    let rounds = 1000;
    let started = Instant::now();
    for _ in 0..rounds {
        view.update(cx, |view, cx| {
            view.smart_highlight = None;
            view.update_smart_highlight(&shown, &rope, cx);
        });
    }
    let cold = started.elapsed() / rounds;
    let started = Instant::now();
    for _ in 0..rounds {
        view.update(cx, |view, cx| {
            view.update_smart_highlight(&shown, &rope, cx)
        });
    }
    let cached = started.elapsed() / rounds;

    let frame = |cx: &mut VisualTestContext| {
        let started = Instant::now();
        for _ in 0..200 {
            view.update(cx, |_, cx| cx.notify());
            cx.run_until_parked();
        }
        started.elapsed() / 200
    };
    let with_smart = frame(cx);
    settings(cx, |s| s.highlighting.smart.enabled = false);
    let without = frame(cx);
    eprintln!("{} bytes shown, {found} highlights", shown[0].len());
    let within = birchpad_core::timing::within;
    within(
        "smart highlighting, a search",
        cold,
        Duration::from_millis(10),
    );
    within(
        "smart highlighting, cached",
        cached,
        Duration::from_millis(1),
    );
    within(
        "a frame with smart highlighting",
        with_smart,
        Duration::from_millis(50),
    );
    within("a frame without", without, Duration::from_millis(50));
}
