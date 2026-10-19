//! Notepad++'s translations (`nativeLang` files: `localization/german.xml` in a Notepad++
//! folder), read from the user's own copy, never bundled (they are under the GPL): the names of
//! the main menus and the commands Birchpad shares with Notepad++, as texts by Birchpad's keys
//! (`menu.file`, `command.file.new`), which come before Birchpad's own translation.

use std::collections::HashMap;

/// The main menus, by Notepad++'s `menuId`, and Birchpad's key for each.
const MENUS: &[(&str, &str)] = &[
    ("file", "menu.file"),
    ("edit", "menu.edit"),
    ("search", "menu.search"),
    ("view", "menu.view"),
    ("encoding", "menu.encoding"),
    ("language", "menu.language"),
    ("settings", "menu.settings"),
    ("tools", "menu.tools"),
    ("macro", "menu.macro"),
    ("run", "menu.run"),
    ("plugins", "menu.plugins"),
    ("window", "menu.window"),
    ("?", "menu.help"),
];

/// Notepad++'s command ids (`IDM_*` of its `menuCmdID.h`) and Birchpad's command for each:
/// the File, Edit and Search commands both editors have.
pub const COMMANDS: &[(u32, &str)] = &[
    (41001, "file.new"),
    (41002, "file.open"),
    (41003, "file.close"),
    (41004, "file.close-all"),
    (41005, "file.close-others"),
    (41006, "file.save"),
    (41007, "file.save-all"),
    (41008, "file.save-as"),
    (41010, "file.print"),
    (41011, "file.exit"),
    (41012, "file.load-session"),
    (41013, "file.save-session"),
    (41014, "file.reload"),
    (41015, "file.save-copy-as"),
    (41016, "file.move-to-recycle-bin"),
    (41017, "file.rename"),
    (42001, "edit.cut"),
    (42002, "edit.copy"),
    (42003, "edit.undo"),
    (42004, "edit.redo"),
    (42005, "edit.paste"),
    (42006, "edit.delete"),
    (42007, "edit.select-all"),
    (42010, "edit.duplicate-line"),
    (42012, "edit.split-lines"),
    (42013, "edit.join-lines"),
    (43001, "search.find"),
    (43002, "search.find-next"),
    (43003, "search.replace"),
    (43004, "search.go-to"),
    (43009, "search.go-to-matching-brace"),
    (43010, "search.find-previous"),
    (43011, "search.incremental"),
    (43013, "search.find-in-files"),
];

/// The texts of a `nativeLang` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeLang {
    /// The language's name, as the file gives it (`Deutsch`).
    pub name: String,
    /// Texts by Birchpad's keys.
    pub texts: HashMap<String, String>,
}

/// A menu item's name without its access key (`&File` is `File`, `&&` an `&`) and without a
/// shortcut after a tab.
fn plain(name: &str) -> String {
    let name = name.split('\t').next().unwrap_or_default();
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}

/// Reads a Notepad++ `nativeLang` file.
pub fn parse_native_lang(xml: &str) -> Result<NativeLang, String> {
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let root = document
        .descendants()
        .find(|node| node.has_tag_name("Native-Langue"))
        .ok_or_else(|| "no <Native-Langue>".to_owned())?;
    let mut texts = HashMap::new();
    let main = root.descendants().find(|node| node.has_tag_name("Main"));
    for item in main
        .into_iter()
        .flat_map(|main| main.descendants())
        .filter(|node| node.has_tag_name("Item"))
    {
        let Some(name) = item
            .attribute("name")
            .map(plain)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        if let Some(menu) = item.attribute("menuId")
            && let Some((_, key)) = MENUS.iter().find(|(id, _)| *id == menu)
        {
            texts.insert((*key).to_owned(), name);
        } else if let Some(id) = item.attribute("id").and_then(|id| id.parse::<u32>().ok())
            && let Some((_, command)) = COMMANDS.iter().find(|(known, _)| *known == id)
        {
            texts.insert(format!("command.{command}"), name);
        }
    }
    Ok(NativeLang {
        name: root.attribute("name").unwrap_or_default().to_owned(),
        texts,
    })
}

#[cfg(test)]
mod tests {
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
}
