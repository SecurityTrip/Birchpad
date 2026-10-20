use birchpad_config::*;
use toml::Table;
use windows_registry::CURRENT_USER;

#[test]
fn reads_typed_values() {
    let path = format!(r"Software\Birchpad-tests\policy-{}", std::process::id());
    let key = CURRENT_USER.create(&path).unwrap();
    key.set_string("UpdateMode", "off").unwrap();
    key.set_u32("CrashReports", 0).unwrap();
    key.set_multi_string("AllowedPlugins", &["compare", "json-tools"])
        .unwrap();
    key.set_string("BackupUnsaved", "not a dword").unwrap();

    drop(key);
    let (table, diagnostics) = user_registry_policies(&path);
    CURRENT_USER.remove_tree(&path).unwrap();
    let (none, problems) = user_registry_policies(&path);
    assert!(
        none.is_empty() && problems.is_empty(),
        "a key that does not exist sets nothing"
    );

    let expected: Table = toml::from_str(
        "updates.mode = 'off'\n\
                 diagnostics.crash-reports = false\n\
                 plugins.allowed = ['compare', 'json-tools']",
    )
    .unwrap();
    assert_eq!(table, expected);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].key.as_deref(),
        Some("session.backup-unsaved")
    );
}
