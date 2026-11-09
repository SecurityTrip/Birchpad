use birchpad_lsp::PositionEncoding;
use birchpad_lsp::capabilities::*;
use birchpad_lsp::lsp_types::ServerCapabilities;
use serde_json::json;

fn abilities(capabilities: serde_json::Value) -> Abilities {
    let capabilities: ServerCapabilities = serde_json::from_value(capabilities).unwrap();
    Abilities::of(&capabilities)
}

#[test]
fn a_server_that_says_nothing_can_do_nothing() {
    let none = abilities(json!({}));
    assert_eq!(none, Abilities::default());
    assert_eq!(none.encoding, PositionEncoding::Utf16);
    // Each provider turned off with false.
    let off = abilities(json!({
        "textDocumentSync": 0,
        "hoverProvider": false,
        "definitionProvider": false,
        "referencesProvider": false,
        "renameProvider": false,
        "documentFormattingProvider": false,
    }));
    assert_eq!(off, Abilities::default());
}

#[test]
fn synchronization_and_saving_options() {
    let full = abilities(
        json!({ "textDocumentSync": { "openClose": true, "change": 2,
        "save": { "includeText": true } } }),
    );
    assert!(full.sync);
    assert_eq!(full.save, Some(true));
    let save_only = abilities(json!({ "textDocumentSync": { "save": true } }));
    assert!(!save_only.sync, "neither opened nor changed");
    assert_eq!(save_only.save, Some(false));
    let changes = abilities(json!({ "textDocumentSync": { "change": 1, "save": false } }));
    assert!(changes.sync);
    assert_eq!(changes.save, None);
    let none = abilities(json!({ "textDocumentSync": { "change": 0, "openClose": false } }));
    assert!(!none.sync);
    let save_options = abilities(json!({ "textDocumentSync": { "openClose": true, "save": {} } }));
    assert_eq!(save_options.save, Some(false));
}

#[test]
fn providers_with_options_count() {
    let all = abilities(json!({
        "positionEncoding": "utf-16",
        "completionProvider": {},
        "hoverProvider": { "workDoneProgress": false },
        "definitionProvider": true,
        "referencesProvider": {},
        "renameProvider": true,
        "documentFormattingProvider": {},
    }));
    assert!(all.completion && all.hover && all.definition);
    assert!(all.references && all.rename && all.formatting);
    assert!(all.trigger_characters.is_empty());
    assert_eq!(all.encoding, PositionEncoding::Utf16);
}

#[test]
fn the_initialize_parameters_name_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let params = initialize_params(dir.path(), None);
    assert_eq!(params["processId"], serde_json::Value::Null);
    let name = dir.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(params["workspaceFolders"][0]["name"], name);
    assert_eq!(params["rootPath"], dir.path().to_str().unwrap());
    assert_eq!(params["capabilities"]["workspace"]["applyEdit"], false);
    // A relative root has no URI and no folders.
    let params = initialize_params(std::path::Path::new("relative"), Some(1));
    assert_eq!(params["rootUri"], serde_json::Value::Null);
    assert_eq!(params["workspaceFolders"], serde_json::Value::Null);
}
