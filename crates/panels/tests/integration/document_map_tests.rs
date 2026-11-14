#![allow(clippy::single_range_in_vec_init, reason = "expected bars written out")]

use birchpad_panels::document_map::*;

const RED: u32 = 0xff0000;
const BLUE: u32 = 0x0000ff;
const TEXT: u32 = 0x1f2328;

#[test]
fn a_short_document_is_all_shown_from_the_top() {
    assert_eq!(first_map_line(10, 0..10, 100), 0);
    assert_eq!(first_map_line(100, 50..60, 100), 0, "exactly as many lines");
    assert_eq!(first_map_line(0, 0..0, 100), 0, "an empty map");
}

#[test]
fn a_long_map_scrolls_in_step_with_the_view() {
    // 1000 lines, 100 on the map, 50 in the view: the map scrolls 900 lines while the view
    // scrolls 950.
    assert_eq!(first_map_line(1000, 0..50, 100), 0);
    assert_eq!(first_map_line(1000, 950..1000, 100), 900, "at the end");
    assert_eq!(first_map_line(1000, 475..525, 100), 450, "halfway");
    // Past the end, a view taller than the document, an empty view.
    assert_eq!(first_map_line(1000, 2000..2050, 100), 900);
    assert_eq!(first_map_line(1000, 0..5000, 100), 0);
    assert_eq!(first_map_line(1000, 500..500, 100), 450);
    // A map of one line.
    assert_eq!(first_map_line(3, 2..3, 1), 2);
}

#[test]
fn bars_cover_the_runs_that_are_not_blank() {
    assert_eq!(
        line_bars("ab  cd", &[], 4, 100),
        [(0..2, TEXT), (4..6, TEXT)]
    );
    assert!(line_bars("", &[], 4, 100).is_empty());
    assert!(line_bars(" \t  ", &[], 4, 100).is_empty(), "blank");
    // A tab goes to the next stop; a wide character takes two columns.
    assert_eq!(line_bars("\tx", &[], 4, 100), [(4..5, TEXT)]);
    assert_eq!(
        line_bars("ab\tc", &[], 4, 100),
        [(0..2, TEXT), (4..5, TEXT)]
    );
    assert_eq!(line_bars("日本", &[], 4, 100), [(0..4, TEXT)]);
}

#[test]
fn bars_take_the_colors_of_the_highlights() {
    // "fn main" with "fn" red and "main" blue; the space between splits nothing more.
    let spans = [(0..2, RED), (3..7, BLUE)];
    assert_eq!(
        line_bars("fn main()", &spans, 4, 100),
        [(0..2, RED), (3..7, BLUE), (7..9, TEXT)]
    );
    // Adjacent runs of one color are one bar; a highlight over part of a word splits it.
    assert_eq!(
        line_bars("abcd", &[(1..3, RED)], 4, 100),
        [(0..1, TEXT), (1..3, RED), (3..4, TEXT)]
    );
    assert_eq!(
        line_bars("ab", &[(0..1, RED), (1..2, RED)], 4, 100),
        [(0..2, RED)]
    );
    // Highlights past the end of the line are ignored.
    assert_eq!(line_bars("a", &[(5..9, RED)], 4, 100), [(0..1, TEXT)]);
}

#[test]
fn bars_stop_at_the_edge_of_the_map() {
    assert_eq!(line_bars("abcdef", &[], 4, 4), [(0..4, TEXT)]);
    assert_eq!(
        line_bars("ab cdef", &[], 4, 4),
        [(0..2, TEXT), (3..4, TEXT)]
    );
    assert_eq!(
        line_bars("日本", &[], 4, 3),
        [(0..3, TEXT)],
        "a wide character cut in half"
    );
    assert!(line_bars("abc", &[], 4, 0).is_empty());
}

// --- In the window -------------------------------------------------------------------------
