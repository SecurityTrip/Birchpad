use gpui_kit::{TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::preferences::*;
use birchpad::workspace::Workspace;
use gpui_kit::AppContext as _;
use gpui_kit::Entity;
use gpui_kit::component::WindowExt as _;
use toml::Value;

fn with_settings_file(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, "# mine\n[editor]\ntab-width = 4 # keep\n").unwrap();
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path));
    (workspace, cx, dir)
}

#[gpui_kit::test]
fn a_change_is_written_and_applied(cx: &mut TestAppContext) {
    let (_, cx, dir) = with_settings_file(cx);
    cx.update(|_, cx| {
        set("editor.tab-width", Some(Value::Integer(2)), cx).unwrap();
        assert_eq!(AppState::global(cx).settings.editor.tab_width, 2);
        set(
            "files.ansi-encoding",
            Some(Value::String("windows-1251".into())),
            cx,
        )
        .unwrap();
        assert_eq!(
            AppState::global(cx).ansi,
            birchpad_core::Encoding::Legacy("windows-1251")
        );
        set("files.ansi-encoding", None, cx).unwrap();
        assert_eq!(AppState::global(cx).settings.files.ansi_encoding, None);
    });
    let text = std::fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(
        text.starts_with("# mine\n[editor]\ntab-width = 2 # keep\n"),
        "{text}"
    );
}

#[gpui_kit::test]
fn refused_changes_leave_everything(cx: &mut TestAppContext) {
    let (_, cx, dir) = with_settings_file(cx);
    let path = dir.path().join("settings.toml");
    let before = std::fs::read_to_string(&path).unwrap();
    cx.update(|_, cx| {
        assert!(set("editor.tab-width", Some(Value::String("four".into())), cx).is_err());
        assert!(set("no.such", Some(Value::Integer(1)), cx).is_err());
        // A setting locked by policy.
        cx.global_mut::<AppState>()
            .policies
            .push(("updates.mode".into(), "\"off\"".into()));
        let error = set("updates.mode", Some(Value::String("auto".into())), cx).unwrap_err();
        assert!(error.to_string().contains("administrator"));
        assert_eq!(AppState::global(cx).settings.editor.tab_width, 4);
        // No settings file to write to.
        cx.global_mut::<AppState>().paths.user_settings = None;
        assert!(set("editor.tab-width", Some(Value::Integer(2)), cx).is_err());
    });
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[gpui_kit::test]
fn the_dialog_applies_fields_and_shows_errors(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = with_settings_file(cx);
    let weak = workspace.downgrade();
    let preferences = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| Preferences::new(weak, window, cx))
    });
    let type_into = |key: &'static str, text: &str, cx: &mut VisualTestContext| {
        let text = text.to_owned();
        preferences.update_in(cx, |this, window, cx| {
            let input = this.inputs[key].clone();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
            let entry = *ENTRIES.iter().find(|entry| entry.key == key).unwrap();
            this.field_changed(entry, cx);
        });
        cx.run_until_parked();
    };
    type_into("editor.tab-width", "8", cx);
    cx.update(|_, cx| assert_eq!(AppState::global(cx).settings.editor.tab_width, 8));
    type_into("editor.tab-width", "99", cx);
    assert!(preferences.read_with(cx, |this, _| this.errors.contains_key("editor.tab-width")));
    cx.update(|_, cx| assert_eq!(AppState::global(cx).settings.editor.tab_width, 8));
    type_into("editor.tab-width", "3", cx);
    assert!(preferences.read_with(cx, |this, _| this.errors.is_empty()));
    preferences.update(cx, |this, cx| {
        this.apply("editor.line-numbers", Ok(Some(Value::Boolean(false))), cx);
        this.select_section(5, cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!AppState::global(cx).settings.editor.line_numbers));
}

#[gpui_kit::test]
fn the_command_opens_the_dialog(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(
                &birchpad_commands::Invocation::new("settings.preferences"),
                window,
                cx,
            )
            .unwrap();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}
