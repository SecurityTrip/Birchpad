use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::panels::PanelKind;
use crate::workspace::tests::{active_text, open_workspace};

const WESTERN: Encoding = Encoding::Legacy("windows-1252");
const CYRILLIC: Encoding = Encoding::Legacy("windows-1251");

#[test]
fn ascii_and_controls() {
    let rows = rows(WESTERN);
    assert_eq!(rows.len(), 256);
    assert_eq!((rows[0].shown.as_str(), rows[0].ch), ("NUL", Some('\0')));
    assert_eq!(rows[31].shown, "US");
    assert_eq!((rows[32].shown.as_str(), rows[32].name), (" ", None));
    assert_eq!(rows[38].html_name().as_deref(), Some("&amp;"));
    assert_eq!(rows[65].html_decimal().as_deref(), Some("&#65;"));
    assert_eq!(rows[65].html_hex().as_deref(), Some("&#x41;"));
    assert_eq!(
        (rows[127].shown.as_str(), rows[127].ch),
        ("DEL", Some('\u{7F}'))
    );
    assert_eq!(rows[127].html_name(), None);
}

#[test]
fn the_upper_half_follows_the_code_page() {
    let western = rows(WESTERN);
    assert_eq!(western[0x80].shown, "€");
    assert_eq!(western[0x80].html_name().as_deref(), Some("&euro;"));
    assert_eq!(western[0x80].html_hex().as_deref(), Some("&#x20AC;"));
    assert_eq!(western[0xA0].html_name().as_deref(), Some("&nbsp;"));
    assert_eq!(western[0xFF].html_name().as_deref(), Some("&yuml;"));
    let cyrillic = rows(CYRILLIC);
    assert_eq!(cyrillic[0xC0].shown, "А");
    assert_eq!(cyrillic[0xC0].html_name(), None, "HTML 4 names no Cyrillic");
    assert_eq!(cyrillic[0xC0].html_decimal().as_deref(), Some("&#1040;"));
    // A value the code page leaves undefined has nothing to insert.
    let greek = rows(Encoding::Legacy("windows-1253"));
    let undefined = &greek[0xAA];
    assert_eq!((undefined.ch, undefined.shown.as_str()), (None, ""));
    assert_eq!(undefined.html_decimal(), None);
}

#[test]
fn entity_names_cover_latin_1_and_windows_1252() {
    assert_eq!(entity_name('"'), Some("quot"));
    assert_eq!(entity_name('<'), Some("lt"));
    assert_eq!(entity_name('\u{A0}'), Some("nbsp"));
    assert_eq!(entity_name('\u{FF}'), Some("yuml"));
    assert_eq!(entity_name('\u{178}'), Some("Yuml"));
    for nameless in ['a', '\u{9F}', '\u{100}', 'Ж', '\''] {
        assert_eq!(entity_name(nameless), None, "{nameless:?}");
    }
}

fn double_click(selector: &'static str, cx: &mut VisualTestContext) {
    let at = cx.debug_bounds(selector).expect("drawn").center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: at,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: at,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    cx.run_until_parked();
}

fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<CharacterPanel> {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_panel(PanelKind::CharacterPanel, window, cx);
        let view = workspace.docks.view(PanelKind::CharacterPanel).unwrap();
        view.clone().downcast::<CharacterPanel>().unwrap()
    })
}

#[gpui_kit::test]
fn double_clicks_insert_characters_and_entities(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let panel = panel(&workspace, cx);
    cx.run_until_parked();
    // Every column fits in the dock as it opens.
    let last = cx.debug_bounds("character-0-5").expect("drawn");
    let dock = cx.debug_bounds("character-panel").expect("drawn");
    assert!(last.right() <= dock.right(), "{last:?} in {dock:?}");
    // The first rows are on screen: !, ", # are 33 to 35.
    double_click("character-33-2", cx);
    double_click("character-34-3", cx);
    double_click("character-35-4", cx);
    double_click("character-35-5", cx);
    assert_eq!(active_text(&workspace, cx), "!&quot;&#35;&#x23;");
    // A row without an entity name inserts nothing from that column.
    double_click("character-33-3", cx);
    assert_eq!(active_text(&workspace, cx), "!&quot;&#35;&#x23;");
    // A single click only selects.
    let at = cx.debug_bounds("character-36-2").expect("drawn").center();
    cx.simulate_click(at, Modifiers::none());
    assert_eq!(panel.read_with(cx, |panel, _| panel.selected), Some(36));
    assert_eq!(active_text(&workspace, cx).len(), 18);
}

#[gpui_kit::test]
fn the_code_page_is_the_documents_or_the_ansi_one(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let panel = panel(&workspace, cx);
    let ansi = cx.update(|_, cx| AppState::global(cx).ansi);
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.encoding(cx)),
        ansi,
        "UTF-8"
    );
    workspace.update_in(cx, |workspace, window, cx| {
        let invocation = birchpad_commands::Invocation::with_args(
            "encoding.convert-to",
            serde_json::json!({ "encoding": "windows-1251" }),
        );
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.encoding(cx)),
        CYRILLIC
    );
    let first = panel.update(cx, |panel, cx| panel.current_rows(cx)[0xC0].shown.clone());
    assert_eq!(first, "А");
}
