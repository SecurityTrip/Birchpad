use super::*;
use crate::settings::{UpdateChannel, UpdateMode};

fn table(toml: &str) -> Option<Table> {
    Some(toml::from_str(toml).unwrap())
}

#[test]
fn a_language_with_nothing_of_its_own_is_dropped() {
    let resolved = resolve(Sources {
        user: table(
            "[languages.python]\ntab-widt = -1\n[languages.rust]\ntab-width = 2\n\
                 [languages.go]\ninsert-spaces = false\n",
        ),
        ..Sources::default()
    });
    let ids: Vec<&String> = resolved.settings.languages.keys().collect();
    assert_eq!(ids, ["go", "rust"], "python has only an unknown key");
    // Written out and read again, the same.
    let written = Table::try_from(&resolved.settings).unwrap();
    let again = resolve(Sources {
        user: Some(written),
        ..Sources::default()
    });
    assert_eq!(again.settings, resolved.settings);
    assert!(resolve(Sources::default()).settings.languages.is_empty());
}

#[test]
fn later_layers_win_and_policy_locks() {
    let resolved = resolve(Sources {
        machine: table("updates.mode = 'notify'\neditor.tab-width = 2"),
        user: table("updates.mode = 'auto'\nupdates.channel = 'beta'\neditor.tab-width = 8"),
        policy: table("[updates]\nmode = 'off'"),
        diagnostics: Vec::new(),
    });

    assert_eq!(resolved.settings.updates.mode, UpdateMode::Off);
    assert_eq!(resolved.settings.updates.channel, UpdateChannel::Beta);
    assert_eq!(resolved.settings.editor.tab_width, 8);
    assert!(resolved.is_locked("updates.mode"));
    assert!(!resolved.is_locked("updates.channel"));
    assert!(resolved.diagnostics.is_empty());
    assert_eq!(
        resolved.policies(),
        [("updates.mode".to_owned(), "\"off\"".to_owned())]
    );
}

#[test]
fn layers_have_names_for_messages() {
    assert_eq!(Layer::Defaults.to_string(), "built-in defaults");
    assert_eq!(Layer::Machine.to_string(), "machine defaults");
    assert_eq!(Layer::User.to_string(), "user settings");
    assert_eq!(Layer::Policy.to_string(), "administrator policy");
}

#[test]
fn locked_keys_are_exactly_the_policy_keys_that_applied() {
    let resolved = resolve(Sources {
        policy: table("updates.mode = 'off'\neditor.tab-width = 8\neditor.word-wrap = 'x'"),
        ..Sources::default()
    });
    let mut locked: Vec<_> = resolved.locked_keys().collect();
    locked.sort_unstable();
    assert_eq!(locked, ["editor.tab-width", "updates.mode"]);
    assert!(
        !resolved.is_locked("editor.word-wrap"),
        "its value was invalid"
    );
    assert!(!resolved.is_locked("editor"), "only whole keys");
    // No policy at all: nothing is locked.
    assert_eq!(resolve(Sources::default()).locked_keys().count(), 0);
}

#[test]
fn set_path_replaces_a_value_in_the_way_with_a_table() {
    let mut table: Table = toml::from_str("editor = 5").unwrap();
    set_path(&mut table, "editor.tab-width", Value::Integer(2));
    assert_eq!(table, toml::from_str("editor.tab-width = 2").unwrap());
    // A key of one part, and one several tables deep that do not exist yet.
    set_path(&mut table, "top", Value::Boolean(true));
    set_path(&mut table, "a.b.c", Value::Integer(1));
    assert_eq!(
        table,
        toml::from_str("top = true\neditor.tab-width = 2\na.b.c = 1").unwrap()
    );
}

#[test]
fn invalid_key_is_skipped_others_apply() {
    let resolved = resolve(Sources {
        user: table("editor.tab-width = 'four'\neditor.word-wrap = true\nunknown.key = 1"),
        ..Sources::default()
    });

    assert_eq!(resolved.settings.editor.tab_width, 4);
    assert!(resolved.settings.editor.word_wrap);
    assert_eq!(resolved.diagnostics.len(), 1);
    assert_eq!(resolved.diagnostics[0].layer, Layer::User);
    assert_eq!(
        resolved.diagnostics[0].key.as_deref(),
        Some("editor.tab-width")
    );
}

#[test]
fn invalid_policy_value_does_not_lock() {
    let resolved = resolve(Sources {
        policy: table("updates.mode = 'sometimes'"),
        ..Sources::default()
    });
    assert_eq!(resolved.settings.updates.mode, UpdateMode::Auto);
    assert!(!resolved.is_locked("updates.mode"));
    assert_eq!(resolved.diagnostics.len(), 1);
}

#[test]
fn highlighting_settings_nest_two_levels() {
    let resolved = resolve(Sources {
        user: table(
            "highlighting.smart.enabled = false\nhighlighting.token-style.match-case = true",
        ),
        ..Sources::default()
    });
    let highlighting = &resolved.settings.highlighting;
    assert!(!highlighting.smart.enabled);
    assert!(highlighting.smart.whole_word, "defaults stay");
    assert!(highlighting.token_style.match_case);
    assert!(resolved.diagnostics.is_empty());
}

#[test]
fn session_settings() {
    let resolved = resolve(Sources {
        user: table("session.backup-interval-seconds = 30"),
        policy: table("session.backup-unsaved = false"),
        ..Sources::default()
    });
    let session = &resolved.settings.session;
    assert!(session.remember, "on by default, as in Notepad++");
    assert!(!session.backup_unsaved);
    assert_eq!(session.backup_interval_seconds, 30);
    assert!(resolved.is_locked("session.backup-unsaved"));
}

#[test]
fn languages_new_documents_and_files() {
    use crate::settings::{BackupOnSave, DefaultDirectory};
    let resolved = resolve(Sources {
        user: table(
            "languages.python.tab-width = 4\nlanguages.makefile.insert-spaces = false\n\
                 new-document.encoding = 'utf-8-bom'\nfiles.backup-on-save = 'verbose'\n\
                 files.default-directory = 'remember'\nlanguages.rust.tab-width = 'x'",
        ),
        ..Sources::default()
    });
    let settings = &resolved.settings;
    assert_eq!(settings.languages["python"].tab_width, Some(4));
    assert_eq!(settings.languages["makefile"].insert_spaces, Some(false));
    assert_eq!(settings.new_document.encoding, "utf-8-bom");
    assert_eq!(settings.new_document.line_ending, "system", "default");
    assert_eq!(settings.files.backup_on_save, BackupOnSave::Verbose);
    assert_eq!(settings.files.default_directory, DefaultDirectory::Remember);
    assert!(!settings.session.remember_inaccessible);
    assert!(!settings.tabs.exit_on_last_close);
    assert_eq!(resolved.diagnostics.len(), 1, "{:?}", resolved.diagnostics);
    assert!(crate::check_setting("languages.go.tab-width", &toml::Value::Integer(8)).is_ok());
    assert!(crate::check_setting("languages.go.width", &toml::Value::Integer(8)).is_err());
    assert!(crate::check_setting("languages..tab-width", &toml::Value::Integer(8)).is_err());
}

#[test]
fn tab_settings() {
    let defaults = resolve(Sources::default()).settings.tabs;
    assert!(defaults.close_button && defaults.peeker, "as in Notepad++");
    assert!(
        defaults.switcher && defaults.switcher_mru,
        "as in Notepad++"
    );
    assert!(!defaults.vertical && !defaults.multi_line && !defaults.reduce);
    assert!(!defaults.lock && !defaults.double_click_close);

    let resolved = resolve(Sources {
        user: table(
            "tabs.vertical = true\ntabs.double-click-close = true\ntabs.lock = 'yes'\n\
                 tabs.switcher-mru = false",
        ),
        policy: table("tabs.close-button = false"),
        ..Sources::default()
    });
    let tabs = &resolved.settings.tabs;
    assert!(tabs.vertical && tabs.double_click_close && !tabs.switcher_mru);
    assert!(!tabs.lock, "a value of the wrong type keeps the default");
    assert!(!tabs.close_button);
    assert!(resolved.is_locked("tabs.close-button"));
    assert_eq!(resolved.diagnostics.len(), 1);
    assert_eq!(resolved.diagnostics[0].key.as_deref(), Some("tabs.lock"));
}

#[test]
fn toolbar_settings() {
    use crate::settings::{DEFAULT_TOOLBAR, ToolbarSize};
    let default = resolve(Sources::default()).settings.toolbar;
    assert!(default.show);
    assert_eq!(default.size, ToolbarSize::Small);
    assert_eq!(default.buttons, DEFAULT_TOOLBAR);
    let resolved = resolve(Sources {
        user: table(
            "toolbar.size = 'large'
toolbar.buttons = ['file.new', '|', 'no.such']
                 toolbar.show = 1",
        ),
        ..Sources::default()
    });
    let toolbar = &resolved.settings.toolbar;
    assert_eq!(toolbar.size, ToolbarSize::Large);
    assert_eq!(toolbar.buttons, ["file.new", "|", "no.such"]);
    assert!(toolbar.show, "a value of the wrong type keeps the default");
    assert_eq!(resolved.diagnostics.len(), 1);
    // No buttons at all is a toolbar without buttons.
    let empty = resolve(Sources {
        user: table("toolbar.buttons = []"),
        ..Sources::default()
    });
    assert!(empty.settings.toolbar.buttons.is_empty());
}

#[test]
fn instance_settings() {
    use crate::settings::InstanceMode;
    let instances = |toml: &str| {
        resolve(Sources {
            user: table(toml),
            ..Sources::default()
        })
    };
    let default = instances("").settings.instances;
    assert_eq!(default.mode, InstanceMode::Single);
    assert!(!default.new_instance(false) && !default.new_instance(true));
    let session = instances("instances.mode = 'session'").settings.instances;
    assert!(!session.new_instance(false) && session.new_instance(true));
    let multi = instances("instances.mode = 'multi'").settings.instances;
    assert!(multi.new_instance(false) && multi.new_instance(true));
    let wrong = instances("instances.mode = 'sometimes'");
    assert_eq!(wrong.settings.instances.mode, InstanceMode::Single);
    assert_eq!(wrong.diagnostics.len(), 1);
}

#[test]
fn edge_and_current_line_settings() {
    use crate::settings::{CurrentLine, Edge};
    let resolved = resolve(Sources {
        user: table(
            "editor.edge = 'line'\neditor.edge-columns = [80, 120]\n\
                 editor.current-line = 'frame'\neditor.edge-column = 72",
        ),
        ..Sources::default()
    });
    let editor = &resolved.settings.editor;
    assert_eq!(editor.edge, Edge::Line);
    assert_eq!(editor.edge_columns, [80, 120]);
    assert_eq!(editor.current_line, CurrentLine::Frame);
    assert!(editor.indent_guides, "on by default, as in Notepad++");
    assert!(!editor.show_whitespace && !editor.show_eol && !editor.wrap_symbol);
    // The edge-column of earlier versions is ignored like any unknown key.
    assert!(resolved.diagnostics.is_empty());
}
