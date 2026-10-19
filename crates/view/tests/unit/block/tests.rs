use super::*;
use crate::LayoutConfig;

fn ranges(text: &str, block: Block) -> Vec<Range> {
    let rope = Rope::from_str(text);
    let mut display = DisplayMap::new(&rope, LayoutConfig::default());
    block.selection(&rope, &mut display).ranges().to_vec()
}

#[test]
fn a_rectangle_reaches_into_virtual_space() {
    let text = "abcdef\nab\n\nabcdef";
    let block = Block::new(BlockPoint::new(0, 1), BlockPoint::new(3, 4));
    assert_eq!(
        ranges(text, block),
        [
            Range::new(1, 4),
            Range::new(8, 9).with_virtual(0, 2),
            Range::new(10, 10).with_virtual(1, 4),
            Range::new(12, 15),
        ]
    );
}

#[test]
fn a_rectangle_selected_leftwards_is_backward() {
    let block = Block::new(BlockPoint::new(1, 3), BlockPoint::new(0, 1));
    let rope = Rope::from_str("abcd\nx");
    let mut display = DisplayMap::new(&rope, LayoutConfig::default());
    let selection = block.selection(&rope, &mut display);
    assert_eq!(
        selection.ranges(),
        [Range::new(3, 1), Range::new(6, 6).with_virtual(2, 0)]
    );
    assert_eq!(selection.primary_index(), 0, "the caret's line is primary");
}

#[test]
fn edges_follow_cells_across_tabs_and_wide_characters() {
    // Columns: "\t" 0-4, "x" 4; "日本" 0-2, 2-4, "y" 4.
    let text = "\tx\n日本y";
    let block = Block::new(BlockPoint::new(0, 4), BlockPoint::new(1, 5));
    assert_eq!(ranges(text, block), [Range::new(1, 2), Range::new(9, 10)]);
    // An edge inside a wide character rounds to its nearer side.
    let block = Block::new(BlockPoint::new(1, 1), BlockPoint::new(1, 3));
    assert_eq!(ranges(text, block), [Range::new(6, 9)]);
}

#[test]
fn a_thin_rectangle_is_a_caret_on_each_line() {
    let thin = Block::new(BlockPoint::new(0, 2), BlockPoint::new(2, 2));
    assert!(thin.is_thin());
    assert!(!Block::new(BlockPoint::new(0, 2), BlockPoint::new(2, 3)).is_thin());
    assert_eq!(
        ranges("abcd\nab\nabcd", thin),
        [Range::point(2), Range::point(7), Range::point(10)]
    );
}

#[test]
fn a_rectangle_past_the_last_line_stops_there() {
    let text = "ab\ncd";
    // From the last line down past it: only the lines that exist.
    let block = Block::new(BlockPoint::new(1, 0), BlockPoint::new(5, 1));
    assert_eq!(ranges(text, block), [Range::new(3, 4)]);
    // Entirely below the text: a caret at its end.
    let block = Block::new(BlockPoint::new(3, 0), BlockPoint::new(4, 1));
    assert_eq!(ranges(text, block), [Range::point(5)]);
    // In an empty text: the one empty line, with virtual space.
    let block = Block::new(BlockPoint::new(0, 0), BlockPoint::new(0, 2));
    assert_eq!(ranges("", block), [Range::new(0, 0).with_virtual(0, 2)]);
}

#[test]
fn hidden_lines_are_skipped() {
    let rope = Rope::from_str("a\nb\nc\nd");
    let mut display = DisplayMap::new(&rope, LayoutConfig::default());
    display.set_hidden(std::iter::once(1..3).collect());
    let block = Block::new(BlockPoint::new(0, 0), BlockPoint::new(3, 1));
    let selection = block.selection(&rope, &mut display);
    assert_eq!(selection.ranges(), [Range::new(0, 1), Range::new(6, 7)]);
    assert_eq!(selection.primary(), Range::new(6, 7));
    assert_eq!(display.block_point(&rope, 7, 2), BlockPoint::new(3, 3));
}
