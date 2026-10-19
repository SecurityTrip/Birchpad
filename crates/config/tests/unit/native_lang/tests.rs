use super::*;

const GERMAN: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <Native-Langue name="Deutsch" filename="german.xml">
        <Menu>
            <Main>
                <Entries>
                    <Item menuId="file" name="&amp;Datei"/>
                    <Item menuId="edit" name="&amp;Bearbeiten"/>
                    <Item menuId="?" name="&amp;?"/>
                    <Item menuId="unknown" name="Unbekannt"/>
                </Entries>
                <Commands>
                    <Item id="41001" name="&amp;Neu"/>
                    <Item id="41006" name="&amp;Speichern&#x09;Strg+S"/>
                    <Item id="42001" name="&amp;Ausschneiden &amp;&amp; mehr"/>
                    <Item id="99999" name="Nicht in Birchpad"/>
                    <Item id="41002" name=""/>
                </Commands>
            </Main>
        </Menu>
    </Native-Langue>
</NotepadPlus>"#;

#[test]
fn menus_and_commands_by_birchpads_keys() {
    let german = parse_native_lang(GERMAN).unwrap();
    assert_eq!(german.name, "Deutsch");
    let text = |key: &str| german.texts.get(key).map(String::as_str);
    assert_eq!(text("menu.file"), Some("Datei"));
    assert_eq!(text("menu.help"), Some("?"));
    assert_eq!(text("command.file.new"), Some("Neu"));
    assert_eq!(
        text("command.file.save"),
        Some("Speichern"),
        "the shortcut cut off"
    );
    assert_eq!(text("command.edit.cut"), Some("Ausschneiden & mehr"));
    assert_eq!(text("command.file.open"), None, "an empty name");
    assert_eq!(german.texts.len(), 6, "unknown menus and commands left out");
}

#[test]
fn what_is_not_a_native_lang_file() {
    assert!(parse_native_lang("<NotepadPlus/>").is_err());
    assert!(parse_native_lang("<broken").is_err());
    let empty = parse_native_lang("<NotepadPlus><Native-Langue/></NotepadPlus>").unwrap();
    assert_eq!(empty, NativeLang::default());
}

#[test]
fn access_keys_and_shortcuts_are_left_out() {
    assert_eq!(plain("&Open...\tCtrl+O"), "Open...");
    assert_eq!(plain("Save && Close"), "Save & Close");
    assert_eq!(plain("&"), "");
    assert_eq!(plain(""), "");
}
