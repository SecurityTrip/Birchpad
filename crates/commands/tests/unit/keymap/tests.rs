use super::*;

const PLATFORMS: [Platform; 3] = [Platform::Windows, Platform::Linux, Platform::MacOs];

fn keys(source: &str, platform: Platform) -> Vec<Keystroke> {
    Keystroke::parse_sequence(source, platform).unwrap()
}

fn command_for<'a>(keymap: &'a Keymap, source: &str, context_path: &[&str]) -> Option<&'a str> {
    keymap
        .resolve(&keys(source, keymap.platform()), context_path)
        .map(|binding| binding.invocation.command.as_str())
}

#[test]
fn default_keymap_is_clean_on_every_platform() {
    for platform in PLATFORMS {
        let keymap = Keymap::with_defaults(platform);
        assert!(
            keymap.diagnostics().is_empty(),
            "{platform:?}: {:#?}",
            keymap.diagnostics()
        );
        assert!(keymap.prefix_conflicts().is_empty());
        assert!(keymap.bindings().len() > 60);
    }
}

#[test]
fn default_bindings_follow_notepad_plus_plus() {
    let windows = Keymap::with_defaults(Platform::Windows);
    let editor = ["Workspace", "Editor"];
    assert_eq!(command_for(&windows, "ctrl-s", &editor), Some("file.save"));
    assert_eq!(
        command_for(&windows, "ctrl-alt-s", &editor),
        Some("file.save-as")
    );
    assert_eq!(
        command_for(&windows, "ctrl-left", &editor),
        Some("cursor.word-left")
    );
    assert_eq!(
        command_for(&windows, "ctrl-tab", &editor),
        Some("view.switch-document")
    );
    assert_eq!(
        command_for(&windows, "ctrl-pagedown", &editor),
        Some("view.next-tab")
    );
    assert_eq!(
        command_for(&windows, "ctrl-shift-pagedown", &editor),
        Some("view.move-tab-forward")
    );
    // Editing keys only apply inside an editor.
    assert_eq!(command_for(&windows, "ctrl-z", &["Workspace"]), None);

    let mac = Keymap::with_defaults(Platform::MacOs);
    assert_eq!(command_for(&mac, "cmd-s", &editor), Some("file.save"));
    assert_eq!(command_for(&mac, "cmd-left", &editor), Some("cursor.home"));
    assert_eq!(
        command_for(&mac, "alt-backspace", &editor),
        Some("edit.delete-word-left")
    );
    assert_eq!(
        command_for(&windows, "alt-backspace", &editor),
        Some("edit.undo")
    );
}

#[test]
fn every_default_binding_targets_a_known_command() {
    for platform in PLATFORMS {
        for binding in Keymap::with_defaults(platform).bindings() {
            assert!(catalog::find(&binding.invocation.command).is_some());
        }
    }
}

#[test]
fn user_layer_overrides_and_unbinds() {
    let mut keymap = Keymap::with_defaults(Platform::Linux);
    keymap.add_layer(
        Layer::User,
        r#"
                [[binding]]
                keys = "ctrl-s"
                command = "file.save-all"

                [[binding]]
                keys = "ctrl-w"
                unbind = true

                [[binding]]
                keys = "ctrl-shift-l"
                command = "edit.convert-eol"
                args = { eol = "lf" }
                context = "Editor"
            "#,
    );
    assert!(
        keymap.diagnostics().is_empty(),
        "{:?}",
        keymap.diagnostics()
    );
    let path = ["Workspace", "Editor"];
    assert_eq!(command_for(&keymap, "ctrl-s", &path), Some("file.save-all"));
    assert_eq!(command_for(&keymap, "ctrl-w", &path), None);
    let eol = keymap
        .resolve(&keys("ctrl-shift-l", Platform::Linux), &path)
        .unwrap();
    assert_eq!(eol.invocation.args, serde_json::json!({ "eol": "lf" }));
    assert_eq!(eol.layer, Layer::User);

    let save_all = Invocation::new("file.save-all");
    let shortcuts: Vec<String> = keymap
        .bindings_for(&save_all)
        .map(Binding::keys_string)
        .collect();
    assert_eq!(shortcuts, ["ctrl-shift-s", "ctrl-s"]);
}

#[test]
fn deeper_context_wins() {
    let mut keymap = Keymap::new(Platform::Windows);
    keymap.add_layer(
        Layer::Default,
        r#"
                [[binding]]
                keys = "tab"
                command = "view.next-tab"

                [[binding]]
                keys = "tab"
                command = "edit.tab"
                context = "Editor"
            "#,
    );
    assert_eq!(
        command_for(&keymap, "tab", &["Workspace", "Editor"]),
        Some("edit.tab")
    );
    assert_eq!(
        command_for(&keymap, "tab", &["Workspace"]),
        Some("view.next-tab")
    );
}

#[test]
fn reports_problems_and_keeps_going() {
    let mut keymap = Keymap::new(Platform::Windows);
    keymap.add_layer(
        Layer::User,
        r#"
                [[binding]]
                keys = "ctrl-q"
                command = "no.such-command"

                [[binding]]
                keys = "hyper-q"
                command = "file.new"

                [[binding]]
                keys = "ctrl-e"

                [[binding]]
                keys = "ctrl-e"
                command = "file.new"
                platforms = ["amiga"]

                [[binding]]
                keys = "ctrl-k"
                command = "file.new"

                [[binding]]
                keys = "ctrl-k"
                command = "file.open"

                [[binding]]
                keys = "ctrl-k ctrl-c"
                command = "file.close"
            "#,
    );
    let diagnostics = keymap.diagnostics();
    assert_eq!(diagnostics.len(), 5, "{diagnostics:#?}");
    assert!(matches!(
        &diagnostics[4],
        KeymapDiagnostic::Duplicate { by, .. } if by.command == "file.open"
    ));
    assert_eq!(
        command_for(&keymap, "ctrl-k", &[]),
        Some("file.open"),
        "the later duplicate wins"
    );
    assert_eq!(keymap.prefix_conflicts().len(), 1);

    let mut broken = Keymap::new(Platform::Windows);
    broken.add_layer(Layer::User, "[[binding]\nkeys = ");
    assert!(matches!(
        broken.diagnostics(),
        [KeymapDiagnostic::Syntax { .. }]
    ));
}

#[test]
fn problems_read_as_sentences() {
    assert_eq!(Layer::Default.to_string(), "default keymap");
    assert_eq!(Layer::User.to_string(), "user keymap");
    let syntax = KeymapDiagnostic::Syntax {
        layer: Layer::User,
        message: "expected `]`".into(),
    };
    assert_eq!(syntax.to_string(), "user keymap: expected `]`");
    let invalid = KeymapDiagnostic::Invalid {
        layer: Layer::Default,
        keys: "hyper-q".into(),
        message: "unknown modifier or key `hyper`".into(),
    };
    assert_eq!(
        invalid.to_string(),
        "default keymap: `hyper-q`: unknown modifier or key `hyper`"
    );
    let duplicate = |context: Option<&str>| KeymapDiagnostic::Duplicate {
        layer: Layer::User,
        keys: "ctrl-k".into(),
        context: context.map(str::to_owned),
        replaced: Invocation::new("file.new"),
        by: Invocation::new("file.open"),
    };
    assert_eq!(
        duplicate(None).to_string(),
        "user keymap: `ctrl-k` is bound twice; `file.open` replaces `file.new`"
    );
    assert_eq!(
        duplicate(Some("Editor")).to_string(),
        "user keymap: `ctrl-k` in Editor is bound twice; `file.open` replaces `file.new`"
    );
}

#[test]
fn an_empty_user_keymap_changes_nothing() {
    let defaults = Keymap::with_defaults(Platform::Windows);
    let mut keymap = Keymap::with_defaults(Platform::Windows);
    keymap.add_layer(Layer::User, "");
    assert!(keymap.diagnostics().is_empty());
    assert_eq!(keymap.bindings().len(), defaults.bindings().len());
}

#[test]
fn platform_specific_bindings_are_filtered() {
    let linux = Keymap::with_defaults(Platform::Linux);
    let mac = Keymap::with_defaults(Platform::MacOs);
    assert!(command_for(&linux, "cmd-q", &[]).is_none());
    assert_eq!(command_for(&mac, "cmd-q", &[]), Some("file.exit"));
}
