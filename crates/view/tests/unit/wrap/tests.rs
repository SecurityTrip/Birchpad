use super::*;

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
