//! The settings of Preferences > Language Servers (ADR 0065).

use birchpad_config::*;
use toml::Table;

fn table(toml: &str) -> Option<Table> {
    Some(toml::from_str(toml).unwrap())
}

#[test]
fn servers_are_on_and_none_is_configured_by_default() {
    let settings = Settings::default();
    assert!(settings.lsp.enabled);
    assert!(settings.lsp.servers.is_empty());
    assert_eq!(settings.lsp.server("rust"), None);
    assert_eq!(LspSettings::default(), settings.lsp);
}

#[test]
fn a_language_has_the_server_configured_for_it() {
    let settings: Settings = toml::from_str(
        "[lsp.servers]\nrust = \"rust-analyzer\"\npython = '  pylsp -v  '\nc = ''\ncpp = '   '\n",
    )
    .unwrap();
    assert_eq!(settings.lsp.server("rust"), Some("rust-analyzer"));
    assert_eq!(settings.lsp.server("python"), Some("pylsp -v"), "trimmed");
    // Empty or blank: no server.
    assert_eq!(settings.lsp.server("c"), None);
    assert_eq!(settings.lsp.server("cpp"), None);
    assert_eq!(settings.lsp.server("go"), None);
    assert_eq!(settings.lsp.server(""), None);
    // Turned off, none.
    let mut off = settings.lsp.clone();
    off.enabled = false;
    assert_eq!(off.server("rust"), None);
}

#[test]
fn a_server_of_any_language_can_be_set() {
    check_setting(
        "lsp.servers.rust",
        &toml::Value::String("rust-analyzer".into()),
    )
    .unwrap();
    check_setting("lsp.servers.udl-name", &toml::Value::String("x".into())).unwrap();
    check_setting("lsp.enabled", &toml::Value::Boolean(false)).unwrap();
    let edited = edit_settings(
        "",
        "lsp.servers.go",
        Some(&toml::Value::String("gopls".into())),
    )
    .unwrap();
    let settings: Settings = toml::from_str(&edited).unwrap();
    assert_eq!(settings.lsp.server("go"), Some("gopls"));
    // Removed again.
    let removed = edit_settings(&edited, "lsp.servers.go", None).unwrap();
    let settings: Settings = toml::from_str(&removed).unwrap();
    assert_eq!(settings.lsp.server("go"), None);
}

#[test]
fn wrong_values_and_keys_are_refused() {
    for (key, value) in [
        ("lsp.servers.rust", toml::Value::Integer(1)),
        ("lsp.servers.rust", toml::Value::Boolean(true)),
        ("lsp.enabled", toml::Value::String("yes".into())),
    ] {
        assert!(check_setting(key, &value).is_err(), "{key} = {value}");
    }
    // No language id.
    assert!(check_setting("lsp.servers.", &toml::Value::String("x".into())).is_err());
    assert!(check_setting("lsp.servers", &toml::Value::String("x".into())).is_err());
    assert!(check_setting("lsp.server.rust", &toml::Value::String("x".into())).is_err());
    // A wrong value in a file is left out, and the rest kept.
    let resolved = resolve(Sources {
        user: table("lsp.servers.rust = 1\nlsp.servers.go = 'gopls'"),
        ..Sources::default()
    });
    assert_eq!(resolved.settings.lsp.server("rust"), None);
    assert_eq!(resolved.settings.lsp.server("go"), Some("gopls"));
    assert_eq!(resolved.diagnostics.len(), 1);
}

#[test]
fn the_machine_layer_may_configure_servers_for_everyone() {
    let resolved = resolve(Sources {
        machine: table("lsp.servers.rust = 'rust-analyzer'"),
        user: table("lsp.servers.python = 'pylsp'"),
        ..Sources::default()
    });
    assert_eq!(resolved.settings.lsp.server("rust"), Some("rust-analyzer"));
    assert_eq!(resolved.settings.lsp.server("python"), Some("pylsp"));
    // The user's own command line wins over the machine's.
    let resolved = resolve(Sources {
        machine: table("lsp.servers.rust = 'rust-analyzer'"),
        user: table("lsp.servers.rust = 'ra-multiplex'"),
        ..Sources::default()
    });
    assert_eq!(resolved.settings.lsp.server("rust"), Some("ra-multiplex"));
}
