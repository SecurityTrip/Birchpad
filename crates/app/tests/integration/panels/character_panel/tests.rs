use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::workspace::{active_text, open_workspace};
use birchpad::app_state::AppState;
use birchpad::panels::PanelKind;
use birchpad::panels::character_panel::*;
use birchpad::workspace::Workspace;
use birchpad_core::Encoding;

const CYRILLIC: Encoding = Encoding::Legacy("windows-1251");

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
