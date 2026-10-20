use birchpad_menu_bar::*;

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
