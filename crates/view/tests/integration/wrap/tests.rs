use birchpad_core::Rope;
use birchpad_view::*;

fn rows(text: &str, width: usize) -> Vec<String> {
    let rope = Rope::from_str(text);
    let starts = wrap_line(&rope, 0..rope.len(), width, 4);
    let mut out = Vec::new();
    for (i, &(start, _)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(rope.len(), |&(s, _)| s);
        out.push(rope.slice(start..end).to_string());
    }
    out
}

#[test]
fn breaks_after_spaces() {
    assert_eq!(rows("the quick brown fox", 10), ["the quick ", "brown fox"]);
    assert_eq!(rows("short", 10), ["short"]);
    assert_eq!(rows("", 10), [""]);
}

#[test]
fn breaks_long_words_anywhere() {
    assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(rows("ab cdefghij", 4), ["ab ", "cdef", "ghij"]);
}

#[test]
fn wide_characters_take_two_cells_and_may_break() {
    assert_eq!(rows("日本語の文章", 5), ["日本", "語の", "文章"]);
    assert_eq!(rows("ab日本", 5), ["ab日", "本"]);
}

#[test]
fn tabs_count_to_their_stop_and_hang() {
    // "\t" takes 4 cells and "a" 1; the second tab hangs at the end of the first row, and
    // "b" starts the second row at column 8.
    assert_eq!(rows("\ta\tb", 5), ["\ta\t", "b"]);
    let rope = Rope::from_str("\ta\tb");
    let starts = wrap_line(&rope, 0..rope.len(), 5, 4);
    assert_eq!(starts, [(0, 0), (3, 8)]);
    assert_eq!(rows("ab      cd", 4), ["ab      ", "cd"]);
}

#[test]
fn row_count_fast_path_agrees() {
    for text in ["", "a b c", "日本語の文章", "\t\tx", "word word word word"] {
        let rope = Rope::from_str(text);
        for width in 1..12 {
            assert_eq!(
                row_count(&rope, 0..rope.len(), width, 4),
                wrap_line(&rope, 0..rope.len(), width, 4).len(),
                "{text:?} at {width}"
            );
        }
    }
}

#[test]
fn every_row_has_a_character_even_when_too_narrow() {
    assert_eq!(rows("日本", 1), ["日", "本"]);
}

#[test]
fn wrap_indent_is_the_line_s_indentation_at_most_half_the_width() {
    let line = |text: &str| Rope::from_str(text);
    let indent = |text: &str, mode| wrap_indent(line(text).slice(..), 20, 4, mode);
    assert_eq!(indent("    abc", WrapIndent::None), 0);
    assert_eq!(indent("    abc", WrapIndent::Aligned), 4);
    assert_eq!(indent("\tabc", WrapIndent::Aligned), 4);
    assert_eq!(indent("  abc", WrapIndent::Indent), 6);
    assert_eq!(indent("abc", WrapIndent::Aligned), 0);
    assert_eq!(indent("", WrapIndent::Indent), 4);
    // Never more than half the width.
    assert_eq!(indent(&" ".repeat(30), WrapIndent::Aligned), 10);
}

/// The rows of `text` in a display map wrapping at `width` with `mode`.
fn indented_rows(text: &str, width: usize, mode: WrapIndent) -> Vec<String> {
    let rope = Rope::from_str(text);
    let mut map = DisplayMap::new(
        &rope,
        LayoutConfig {
            tab_width: 4,
            wrap_width: Some(width),
            wrap_indent: mode,
            ..LayoutConfig::default()
        },
    );
    (0..map.row_count(&rope))
        .map(|index| {
            let row = map.row(&rope, index);
            rope.slice(row.range).to_string()
        })
        .collect()
}

#[test]
fn indented_rows_after_the_first_hold_less() {
    let text = "    aaa bbb ccc ddd";
    assert_eq!(
        indented_rows(text, 12, WrapIndent::None),
        ["    aaa bbb ", "ccc ddd"]
    );
    // Rows after the first start four cells in: eight cells left.
    assert_eq!(
        indented_rows(text, 12, WrapIndent::Aligned),
        ["    aaa bbb ", "ccc ddd"]
    );
    assert_eq!(
        indented_rows("    aaaa bbbb cccc dddd", 12, WrapIndent::Aligned),
        ["    aaaa ", "bbbb ", "cccc ", "dddd"]
    );
    // A line without indentation wraps as before.
    assert_eq!(
        indented_rows("aaa bbb ccc ddd", 8, WrapIndent::Aligned),
        indented_rows("aaa bbb ccc ddd", 8, WrapIndent::None)
    );
}

#[test]
fn labels_wrap_a_line_short_in_bytes() {
    // Three no-break spaces are six bytes, three cells, or twelve by label.
    let rope = Rope::from_str("\u{A0}\u{A0}\u{A0}");
    let labels = Grid {
        tab_width: 4,
        labels: NonPrintingLabel::Abbreviation,
    };
    assert_eq!(row_count(&rope, 0..rope.len(), 8, 4), 1);
    assert_eq!(row_count(&rope, 0..rope.len(), 8, labels), 2);
    assert_eq!(
        wrap_line(&rope, 0..rope.len(), 8, labels),
        vec![(0, 0), (4, 8)]
    );
    // Exactly as wide as the labels: one row.
    assert_eq!(row_count(&rope, 0..rope.len(), 12, labels), 1);
    // A label wider than the width still makes a row of its own.
    assert_eq!(row_count(&rope, 0..rope.len(), 1, labels), 3);
}
