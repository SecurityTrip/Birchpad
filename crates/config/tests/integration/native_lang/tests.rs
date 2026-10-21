use birchpad_config::*;

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
    let name_of = |name: &str| {
        let xml = format!(
            r#"<NotepadPlus><Native-Langue name="x"><Menu><Main><Commands>
                <Item id="41001" name="{name}"/>
            </Commands></Main></Menu></Native-Langue></NotepadPlus>"#
        );
        parse_native_lang(&xml)
            .unwrap()
            .texts
            .get("command.file.new")
            .cloned()
    };
    assert_eq!(
        name_of("&amp;Open...&#x09;Ctrl+O").as_deref(),
        Some("Open...")
    );
    assert_eq!(
        name_of("Save &amp;&amp; Close").as_deref(),
        Some("Save & Close")
    );
    // Nothing but an access key, or nothing at all: no text.
    assert_eq!(name_of("&amp;"), None);
    assert_eq!(name_of(""), None);
}
