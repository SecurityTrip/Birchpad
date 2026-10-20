use birchpad_config::PluginInstall;
use birchpad_config::PluginSettings;
use birchpad_plugin::manager::*;
use std::path::Path;
use std::path::PathBuf;

fn manifest(id: &str, name: &str) -> String {
    format!("id = \"{id}\"\nname = \"{name}\"\nversion = \"1.0\"\napi = \"0.1\"\n")
}

fn make_plugin(dir: &Path, id: &str, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join(MANIFEST), manifest(id, name)).unwrap();
    std::fs::write(dir.join("plugin.wasm"), b"\0asm").unwrap();
    std::fs::write(dir.join("assets/readme.txt"), b"hi").unwrap();
    dir.join(MANIFEST)
}

fn allow() -> PluginSettings {
    PluginSettings::default()
}

#[test]
fn discovers_plugins_by_name_and_reports_broken_ones() {
    let temp = tempfile::tempdir().unwrap();
    let plugins = temp.path();
    make_plugin(&plugins.join("zeta"), "zeta", "alpha tools");
    make_plugin(&plugins.join("beta"), "beta", "Beta");
    make_plugin(&plugins.join("wrong-folder"), "other", "Other");
    std::fs::create_dir_all(plugins.join("empty")).unwrap();
    let no_entry = plugins.join("no-entry");
    make_plugin(&no_entry, "no-entry", "No entry");
    std::fs::remove_file(no_entry.join("plugin.wasm")).unwrap();
    std::fs::write(plugins.join("disabled.toml"), "").unwrap();

    let found = discover(plugins);
    let names: Vec<&str> = found
        .plugins
        .iter()
        .map(|p| p.manifest.name.as_str())
        .collect();
    assert_eq!(names, ["alpha tools", "Beta"]);
    assert_eq!(found.problems.len(), 3, "{:?}", found.problems);
    assert!(found.problems.iter().any(|p| p.contains("named after it")));
    assert!(discover(&plugins.join("missing")).plugins.is_empty());
}

#[test]
fn installs_updates_and_removes() {
    let temp = tempfile::tempdir().unwrap();
    let source = make_plugin(&temp.path().join("download"), "hello", "Hello");
    let plugins = temp.path().join("plugins");
    let installed = install(&source, &plugins, &allow()).unwrap();
    assert_eq!(installed.dir, plugins.join("hello"));
    assert!(plugins.join("hello/assets/readme.txt").is_file());
    // Again: an update replaces it, leaving no staging folder behind.
    std::fs::write(source.parent().unwrap().join("plugin.wasm"), b"\0asm v2").unwrap();
    install(&source, &plugins, &allow()).unwrap();
    assert_eq!(
        std::fs::read(plugins.join("hello/plugin.wasm")).unwrap(),
        b"\0asm v2"
    );
    assert_eq!(discover(&plugins).plugins.len(), 1);
    assert_eq!(std::fs::read_dir(&plugins).unwrap().count(), 1);
    // Installing what is installed from its own folder.
    assert!(install(&plugins.join("hello").join(MANIFEST), &plugins, &allow()).is_err());

    uninstall(&plugins, "hello").unwrap();
    assert!(discover(&plugins).plugins.is_empty());
    assert!(uninstall(&plugins, "hello").is_err());
    assert!(uninstall(&plugins, "../download").is_err());
    assert!(source.is_file());
}

#[test]
fn refuses_what_is_not_a_plugin() {
    let temp = tempfile::tempdir().unwrap();
    let plugins = temp.path().join("plugins");
    let broken = temp.path().join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join(MANIFEST), "id = 1").unwrap();
    assert!(install(&broken.join(MANIFEST), &plugins, &allow()).is_err());
    assert!(install(&temp.path().join("none/plugin.toml"), &plugins, &allow()).is_err());
    assert!(install(Path::new("plugin.toml"), &plugins, &allow()).is_err());
    assert!(!plugins.exists());
}

#[test]
fn follows_the_administrators_policy() {
    let temp = tempfile::tempdir().unwrap();
    let source = make_plugin(&temp.path().join("download"), "hello", "Hello");
    let plugins = temp.path().join("plugins");
    let deny = PluginSettings {
        install: PluginInstall::Deny,
        allowed: vec!["hello".into()],
        ..PluginSettings::default()
    };
    assert!(
        install(&source, &plugins, &deny)
            .unwrap_err()
            .to_string()
            .contains("administrator")
    );
    let listed = PluginSettings {
        install: PluginInstall::Allowlist,
        allowed: vec!["other".into()],
        ..PluginSettings::default()
    };
    assert!(permitted("hello", &listed).is_err());
    assert!(install(&source, &plugins, &listed).is_err());
    assert!(!plugins.join("hello").exists());
    let listed = PluginSettings {
        allowed: vec!["other".into(), "hello".into()],
        ..listed
    };
    assert!(permitted("hello", &listed).is_ok());
    install(&source, &plugins, &listed).unwrap();
    assert!(permitted("anything", &allow()).is_ok());
}

#[test]
fn refuses_a_plugin_too_large() {
    let temp = tempfile::tempdir().unwrap();
    let source = make_plugin(&temp.path().join("big"), "big", "Big");
    let file = std::fs::File::create(source.parent().unwrap().join("data.bin")).unwrap();
    file.set_len(MAX_PLUGIN_BYTES + 1).unwrap();
    let error = install(&source, &temp.path().join("plugins"), &allow()).unwrap_err();
    assert!(error.to_string().contains("more than"), "{error}");
}

#[test]
fn turns_plugins_on_and_off() {
    let temp = tempfile::tempdir().unwrap();
    let plugins = temp.path().join("plugins");
    assert!(read_disabled(&plugins).is_empty());
    set_enabled(&plugins, "b", false).unwrap();
    set_enabled(&plugins, "a", false).unwrap();
    set_enabled(&plugins, "a", false).unwrap();
    assert_eq!(read_disabled(&plugins), ["a", "b"]);
    set_enabled(&plugins, "b", true).unwrap();
    set_enabled(&plugins, "c", true).unwrap();
    assert_eq!(read_disabled(&plugins), ["a"]);
    std::fs::write(plugins.join(DISABLED), "disabled = 5").unwrap();
    assert!(read_disabled(&plugins).is_empty());
}
