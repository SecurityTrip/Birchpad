use birchpad_core::Edit;
use birchpad_core::Range;
use birchpad_core::*;

fn type_text(
    doc: &mut Document,
    selection: &Selection,
    text: &str,
    grouping: UndoGrouping,
) -> Selection {
    let transaction = Transaction::replace_selections(doc.text(), selection, text).unwrap();
    doc.apply(&transaction, selection, grouping);
    transaction.selection().unwrap().clone()
}

#[test]
fn merged_typing_is_one_undo_step() {
    let mut doc = Document::new();
    let mut selection = Selection::point(0);
    for ch in ["h", "e", "y"] {
        selection = type_text(&mut doc, &selection, ch, UndoGrouping::MergeWithPrevious);
    }
    assert_eq!(doc.text(), "hey");
    assert_eq!(selection.primary(), Range::point(3));

    let undone = doc.undo().unwrap();
    assert_eq!(doc.text(), "");
    assert_eq!(undone.selection().unwrap().primary(), Range::point(0));
    assert!(doc.undo().is_none());
}

#[test]
fn undo_back_to_saved_state_is_unmodified() {
    let mut doc = Document::from_text(Rope::from_str("saved"));
    assert!(!doc.is_modified());

    let selection = type_text(&mut doc, &Selection::point(5), "!", UndoGrouping::NewStep);
    assert!(doc.is_modified());
    doc.mark_saved();
    assert!(!doc.is_modified());

    doc.undo();
    assert!(doc.is_modified());
    doc.redo();
    assert!(!doc.is_modified());

    // Typing more merges into the saved step, which must still count as a modification.
    type_text(&mut doc, &selection, "?", UndoGrouping::MergeWithPrevious);
    assert_eq!(doc.text(), "saved!?");
    assert!(doc.is_modified());
}

#[test]
fn restored_unsaved_text_stays_modified() {
    let mut doc = Document::from_text(Rope::from_str("from a backup"));
    doc.mark_unsaved();
    assert!(doc.is_modified());
    type_text(&mut doc, &Selection::point(0), "x", UndoGrouping::NewStep);
    doc.undo();
    assert!(
        doc.is_modified(),
        "undoing everything does not reach the saved file"
    );
    doc.mark_saved();
    assert!(!doc.is_modified());
}

#[test]
fn format_changes_are_undoable() {
    use birchpad_core::Encoding;

    let mut doc = Document::from_text(Rope::from_str("a\nb"));
    assert_eq!(doc.line_ending(), LineEnding::Lf);
    let utf16 = Format {
        encoding: Encoding::Utf16Le,
        bom: true,
        ..doc.format()
    };
    let convert = Transaction::set_format(doc.text().len(), utf16);
    doc.apply(&convert, &Selection::point(0), UndoGrouping::NewStep);
    assert_eq!(doc.format(), utf16);
    assert!(doc.is_modified());

    // Typing merges into the conversion step; undo restores both text and format.
    type_text(
        &mut doc,
        &Selection::point(3),
        "!",
        UndoGrouping::MergeWithPrevious,
    );
    assert_eq!(doc.text(), "a\nb!");
    doc.undo();
    assert_eq!(doc.text(), "a\nb");
    assert_eq!(doc.format().encoding, Encoding::Utf8);
    assert!(!doc.is_modified());
    doc.redo();
    assert_eq!(doc.format(), utf16);
}

#[test]
fn edits_during_a_save_keep_the_document_modified() {
    let mut doc = Document::new();
    let selection = type_text(&mut doc, &Selection::point(0), "a", UndoGrouping::NewStep);
    let written = doc.revision();
    // The user keeps typing while the file is being written.
    type_text(&mut doc, &selection, "b", UndoGrouping::MergeWithPrevious);
    doc.mark_saved_at(written);
    assert!(doc.is_modified());
    let mut doc = Document::new();
    type_text(&mut doc, &Selection::point(0), "a", UndoGrouping::NewStep);
    let written = doc.revision();
    doc.mark_saved_at(written);
    assert!(!doc.is_modified());
}

#[test]
fn eol_conversion_is_one_undo_step() {
    let mut doc = Document::from_text(Rope::from_str("a\r\nb\nc"));
    let convert = doc.convert_line_endings(LineEnding::Lf);
    doc.apply(&convert, &Selection::point(0), UndoGrouping::NewStep);
    assert_eq!(doc.text(), "a\nb\nc");
    assert_eq!(doc.line_ending(), LineEnding::Lf);
    doc.undo();
    assert_eq!(doc.text(), "a\r\nb\nc");
    assert_eq!(doc.line_ending(), LineEnding::CrLf);
    assert!(!doc.history().can_undo());
}

#[test]
fn line_ending_mode_falls_back_to_given_format() {
    let doc = Document::with_format(
        Rope::from_str("one line"),
        Format::new().with_line_ending(LineEnding::Cr),
    );
    assert_eq!(doc.line_ending(), LineEnding::Cr);
    let doc = Document::with_format(
        Rope::from_str("a\r\nb"),
        Format::new().with_line_ending(LineEnding::Cr),
    );
    assert_eq!(doc.line_ending(), LineEnding::CrLf);
}

#[test]
fn new_edit_after_undo_discards_redo() {
    let mut doc = Document::from_text(Rope::from_str("abc"));
    let transaction = Transaction::from_edits(doc.text(), [Edit::delete(0..1)]).unwrap();
    doc.apply(&transaction, &Selection::point(0), UndoGrouping::NewStep);
    doc.undo();
    assert!(doc.history().can_redo());

    let transaction = Transaction::from_edits(doc.text(), [Edit::insert(3, "d")]).unwrap();
    doc.apply(
        &transaction,
        &Selection::point(3),
        UndoGrouping::MergeWithPrevious,
    );
    assert!(!doc.history().can_redo());
    assert_eq!(doc.text(), "abcd");
}
