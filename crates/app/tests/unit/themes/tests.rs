use super::*;

const DARKISH: &str = "[editor]\nbackground = '#101010'\n";

#[test]
fn names_list_builtins_then_files() {
    let dir = tempfile::tempdir().unwrap();
    for file in [
        "zeta.toml",
        "Alpha.xml",
        "dark.toml",
        "notes.txt",
        ".toml",
        "beta.TOML",
    ] {
        std::fs::write(dir.path().join(file), "").unwrap();
    }
    std::fs::create_dir(dir.path().join("folder.toml")).unwrap();
    assert_eq!(
        names(Some(dir.path())),
        ["Default", "Dark", "Alpha", "beta", "zeta"]
    );
    // No folder, or one that does not exist: the built-in themes.
    assert_eq!(names(None), ["Default", "Dark"]);
    assert_eq!(
        names(Some(&dir.path().join("missing"))),
        ["Default", "Dark"]
    );
}

#[test]
fn load_prefers_files_then_builtins() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Mine.toml"), DARKISH).unwrap();
    std::fs::write(dir.path().join("Dark.toml"), "dark = false").unwrap();
    let mine = load(Some(dir.path()), "Mine").unwrap();
    assert!(mine.dark);
    assert_eq!(mine.name, "Mine");
    // A file named like a built-in theme replaces it.
    assert!(!load(Some(dir.path()), "Dark").unwrap().dark);
    assert!(load(None, "Dark").unwrap().dark);
    assert_eq!(load(None, "Default").unwrap(), *Theme::default_theme());
}

#[test]
fn load_reports_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Broken.toml"), "[editor]\ncaret = 'red'\n").unwrap();
    std::fs::write(dir.path().join("Old.xml"), "<Other/>").unwrap();
    let broken = load(Some(dir.path()), "Broken").unwrap_err();
    assert!(format!("{broken:#}").contains("Broken.toml"), "{broken:#}");
    assert!(load(Some(dir.path()), "Old").is_err());
    assert!(load(Some(dir.path()), "Nothing").is_err());
    assert!(load(None, "").is_err());
}

#[test]
fn notepad_themes_read_with_a_byte_order_mark() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
            dir.path().join("Np.xml"),
            "\u{feff}<NotepadPlus><GlobalStyles><WidgetStyle name=\"Default Style\" fgColor=\"FFFFFF\" bgColor=\"000000\"/></GlobalStyles></NotepadPlus>",
        )
        .unwrap();
    let theme = load(Some(dir.path()), "Np").unwrap();
    assert!(theme.dark);
    assert_eq!(theme.name, "Np");
}

#[test]
fn import_copies_themes_and_refuses_others() {
    let source = tempfile::tempdir().unwrap();
    let target = source.path().join("settings").join("themes");
    let good = source.path().join("Night.toml");
    std::fs::write(&good, DARKISH).unwrap();
    let bad = source.path().join("Bad.toml");
    std::fs::write(&bad, "[editor").unwrap();
    let other = source.path().join("readme.txt");
    std::fs::write(&other, "").unwrap();
    let missing = source.path().join("Missing.xml");
    let (imported, errors) = import(&[good, bad, other, missing], &target);
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].name, "Night");
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert_eq!(names(Some(&target)), ["Default", "Dark", "Night"]);
    // The same name in the other format replaces it.
    let xml = source.path().join("Night.xml");
    std::fs::write(&xml, "<NotepadPlus><GlobalStyles/></NotepadPlus>").unwrap();
    let (imported, errors) = import(&[xml], &target);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!imported[0].dark);
    assert!(!target.join("Night.toml").exists());
    assert!(!load(Some(&target), "Night").unwrap().dark);
    // Importing a file of the folder itself keeps it.
    let (imported, errors) = import(&[target.join("Night.xml")], &target);
    assert_eq!((imported.len(), errors.len()), (1, 0));
    assert!(target.join("Night.xml").is_file());
    // Nothing to import.
    assert_eq!(import(&[], &target), (vec![], vec![]));
}

#[gpui_kit::test]
fn settings_theme_switches_and_remembers(cx: &mut gpui_kit::TestAppContext) {
    use birchpad_commands::Invocation;
    use serde_json::json;

    let (workspace, cx) = crate::workspace::tests::open_workspace(cx);
    let run = |name: &str, cx: &mut gpui_kit::VisualTestContext| {
        let invocation = Invocation::with_args("settings.theme", json!({ "name": name }));
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        });
        cx.run_until_parked();
        result
    };
    run("dark", cx).unwrap();
    assert!(crate::theme::current().dark);
    assert_eq!(crate::theme::current().name, "Dark");
    cx.update(|_, cx| {
        assert_eq!(AppState::global(cx).state.theme.as_deref(), Some("Dark"));
        assert_eq!(chosen(cx), "Dark");
        assert!(gpui_kit::component::Theme::global(cx).mode.is_dark());
    });
    // An unknown theme changes nothing.
    assert!(run("No Such Theme", cx).is_err());
    assert!(run("", cx).is_err());
    assert_eq!(crate::theme::current().name, "Dark");
    run("Default", cx).unwrap();
    assert!(!crate::theme::current().dark);
    cx.update(|_, cx| assert!(!gpui_kit::component::Theme::global(cx).mode.is_dark()));
}
