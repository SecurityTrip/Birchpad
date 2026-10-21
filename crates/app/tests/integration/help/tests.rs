use birchpad_commands::Invocation;
use birchpad_config::UpdateMode;
use gpui_kit::TestAppContext;

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::help::*;
use gpui_kit::component::WindowExt as _;

#[gpui_kit::test]
fn about_lists_build_paths_and_policies(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().policies =
            vec![("updates.mode".to_owned(), "\"off\"".to_owned())];
        cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Off;
    });
    let text = cx.update(|_, cx| about_text(cx));
    assert!(
        text.starts_with(&format!("Birchpad\nVersion:  {VERSION}\nBuild:    ")),
        "{text}"
    );
    assert!(
        text.contains("\nInstalled:not by Birchpad's installer\n"),
        "{text}"
    );
    assert!(text.contains("\nUpdates:  turned off\n"), "{text}");
    assert!(
        text.ends_with("\nPolicies: updates.mode = \"off\""),
        "{text}"
    );

    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("help.about"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

fn run(
    workspace: &gpui_kit::Entity<birchpad::workspace::Workspace>,
    id: &str,
    cx: &mut gpui_kit::VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new(id), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_web_pages_open_in_the_browser(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    for (id, page) in [
        ("help.home-page", HOME_PAGE),
        ("help.manual", MANUAL),
        ("help.forum", FORUM),
    ] {
        run(&workspace, id, cx);
        assert_eq!(cx.opened_url().as_deref(), Some(page), "{id}");
    }
    assert!(MANUAL.starts_with(HOME_PAGE) && FORUM.starts_with(HOME_PAGE));
}

#[gpui_kit::test]
fn command_line_arguments_list_every_option(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let rows = command_line_rows();
    assert_eq!(rows.len(), birchpad_cli::OPTIONS.len());
    assert_eq!(rows[0], ("-n<line>", "Go to the line (from 1)".to_owned()));
    birchpad_i18n::set_language("ru");
    assert_eq!(command_line_rows()[0].1, "Перейти к строке (с 1)");
    birchpad_i18n::set_language("en");
    run(&workspace, "help.command-line", cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn debug_info_adds_the_system_and_the_program(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let text = cx.update(|_, cx| debug_info(cx));
    assert!(text.starts_with("Birchpad\nVersion:  "), "{text}");
    assert!(
        text.contains(&format!(
            "\nOS:       {} {}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        )),
        "{text}"
    );
    assert!(text.contains("\nProgram:  "), "{text}");
    assert!(
        text.contains("\nProxy:    the environment's or the system's\n"),
        "{text}"
    );
    assert!(text.ends_with("\nPlugins:  none"), "{text}");
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.updates.proxy = " http://proxy:8080 ".into();
    });
    let text = cx.update(|_, cx| debug_info(cx));
    assert!(text.contains("\nProxy:    http://proxy:8080\n"), "{text}");
    run(&workspace, "help.debug-info", cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

fn proxy_dialog() -> gpui_kit::Entity<ProxyDialog> {
    PROXY
        .with(|proxy| proxy.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("the proxy dialog is open")
}

fn set_proxy(text: &str, cx: &mut gpui_kit::VisualTestContext) -> bool {
    let dialog = proxy_dialog();
    cx.update(|window, cx| {
        dialog.update(cx, |dialog, cx| {
            dialog
                .input
                .update(cx, |input, cx| input.set_value(text, window, cx));
            dialog.apply(cx)
        })
    })
}

#[gpui_kit::test]
fn the_updater_proxy_is_written_to_the_settings(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path.clone()));
    run(&workspace, "help.updater-proxy", cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(set_proxy("http://proxy.example.com:8080", cx));
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("proxy = \"http://proxy.example.com:8080\""),
        "{written}"
    );
    assert_eq!(
        cx.update(|_, cx| AppState::global(cx).settings.updates.proxy.clone()),
        "http://proxy.example.com:8080"
    );
    // Not a proxy: refused, said, and the settings keep the one before.
    assert!(!set_proxy("ftp://proxy:21", cx));
    let error = proxy_dialog().read_with(cx, |dialog, _| dialog.error.clone());
    assert!(error.is_some_and(|error| error.contains("ftp://proxy:21")));
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("http://proxy.example.com:8080")
    );
    // Empty: back to the environment's or the system's.
    assert!(set_proxy("  ", cx));
    assert!(!std::fs::read_to_string(&path).unwrap().contains("proxy"));
}

#[gpui_kit::test]
fn a_proxy_set_by_policy_cannot_be_changed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().policies = vec![(
            "updates.proxy".to_owned(),
            "\"http://corp:3128\"".to_owned(),
        )];
    });
    run(&workspace, "help.updater-proxy", cx);
    let error = proxy_dialog().read_with(cx, |dialog, _| dialog.error.clone());
    assert!(error.is_some(), "the dialog says why");
}
