use super::*;

/// Text and selection from a string with carets marked `|` and selections `[...]`.
pub(crate) fn parse(marked: &str) -> (Rope, Selection) {
    let mut text = String::new();
    let mut ranges = Vec::new();
    let mut anchor = None;
    for ch in marked.chars() {
        match ch {
            '|' => ranges.push(Span::point(text.len())),
            '[' => anchor = Some(text.len()),
            ']' => ranges.push(Span::new(anchor.take().unwrap(), text.len())),
            _ => text.push(ch),
        }
    }
    if ranges.is_empty() {
        ranges.push(Span::point(0));
    }
    (Rope::from_str(&text), Selection::new(ranges, 0))
}

/// The text after `transaction`, with its selection marked like `parse` reads it.
pub(crate) fn show(text: &Rope, selection: &Selection, transaction: Option<Transaction>) -> String {
    let mut text = text.clone();
    let mut selection = selection.clone();
    if let Some(transaction) = transaction {
        transaction.changes().apply(&mut text);
        selection = transaction
            .selection()
            .cloned()
            .unwrap_or_else(|| selection.map(transaction.changes()));
    }
    let mut marks: Vec<(usize, char)> = Vec::new();
    for range in selection.iter() {
        if range.is_empty() {
            marks.push((range.head, '|'));
        } else {
            marks.push((range.from(), '['));
            marks.push((range.to(), ']'));
        }
    }
    marks.sort_by_key(|&(pos, mark)| (pos, mark != ']'));
    let mut out = text.to_string();
    for (pos, mark) in marks.into_iter().rev() {
        out.insert(pos, mark);
    }
    out
}

/// Runs `op` on marked text and shows the result.
pub(crate) fn run(
    marked: &str,
    op: impl FnOnce(&Rope, &Selection) -> Option<Transaction>,
) -> String {
    let (text, selection) = parse(marked);
    let transaction = op(&text, &selection);
    show(&text, &selection, transaction)
}
