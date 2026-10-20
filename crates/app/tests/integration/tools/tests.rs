use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, tab_names};
use anyhow::Result;
use birchpad::tools::*;
use birchpad::workspace::Workspace;
use birchpad_core::hash::HashKind;

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

fn type_and_select_all(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
    run(workspace, Invocation::new("edit.select-all"), cx).unwrap();
}

#[gpui_kit::test]
fn converts_the_selection_and_undoes_in_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_and_select_all(&workspace, "Hi мир", cx);
    for (to, expected) in [
        ("base64-encode", "SGkg0LzQuNGA"),
        ("base64-decode", "Hi мир"),
        ("url-encode", "Hi%20%D0%BC%D0%B8%D1%80"),
        ("url-decode", "Hi мир"),
        ("ascii-to-hex", "486920D0BCD0B8D180"),
        ("hex-to-ascii", "Hi мир"),
    ] {
        run(
            &workspace,
            Invocation::with_args("edit.convert", json!({ "to": to })),
            cx,
        )
        .unwrap();
        assert_eq!(active_text(&workspace, cx), expected, "{to}");
    }
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "486920D0BCD0B8D180");
}

#[gpui_kit::test]
fn refuses_what_does_not_convert(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_and_select_all(&workspace, "not hex", cx);
    assert!(
        run(
            &workspace,
            Invocation::with_args("edit.convert", json!({ "to": "hex-to-ascii" })),
            cx
        )
        .is_err()
    );
    assert_eq!(active_text(&workspace, cx), "not hex");
    assert!(
        run(
            &workspace,
            Invocation::with_args("edit.convert", json!({ "to": "rot13" })),
            cx
        )
        .is_err()
    );
    // Nothing selected: nothing to do.
    run(&workspace, Invocation::new("cursor.document-end"), cx).unwrap();
    run(
        &workspace,
        Invocation::with_args("edit.convert", json!({ "to": "base64-encode" })),
        cx,
    )
    .unwrap();
    assert_eq!(active_text(&workspace, cx), "not hex");
}

#[gpui_kit::test]
fn hashes_the_selection_into_the_clipboard(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_and_select_all(&workspace, "abc", cx);
    run(
        &workspace,
        Invocation::with_args("tools.hash-selection", json!({ "hash": "md5" })),
        cx,
    )
    .unwrap();
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
    run(&workspace, Invocation::new("cursor.document-end"), cx).unwrap();
    assert!(
        run(
            &workspace,
            Invocation::with_args("tools.hash-selection", json!({ "hash": "sha1" })),
            cx
        )
        .is_err()
    );
    assert!(
        run(
            &workspace,
            Invocation::with_args("tools.hash-selection", json!({ "hash": "crc" })),
            cx
        )
        .is_err()
    );
}

#[test]
fn hash_listings() {
    let files = vec![
        (std::path::PathBuf::from("a.txt"), Ok(b"abc".to_vec())),
        (
            std::path::PathBuf::from("gone.txt"),
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found",
            )),
        ),
    ];
    let listing = hash_listing(HashKind::Md5, &files);
    assert_eq!(
        listing,
        "900150983cd24fb0d6963f7d28e17f72  a.txt\n# gone.txt: not found\n"
    );
    assert_eq!(hash_listing(HashKind::Sha256, &[]), "");
}

#[gpui_kit::test]
fn text_opens_in_a_new_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_text_in_new_document("x  y\n", window, cx)
        })
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "x  y\n");
}

#[gpui_kit::test]
fn every_hash_of_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_and_select_all(&workspace, "abc", cx);
    for (name, length) in [("md5", 32), ("sha1", 40), ("sha256", 64), ("sha512", 128)] {
        run(
            &workspace,
            Invocation::with_args("tools.hash-selection", json!({ "hash": name })),
            cx,
        )
        .unwrap();
        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.map(|hash| hash.len()), Some(length), "{name}");
    }
}

#[gpui_kit::test]
fn hashes_of_chosen_files_open_in_a_new_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "abc").unwrap();
    let gone = dir.path().join("gone.txt");
    let tabs = tab_names(&workspace, cx).len();
    run(
        &workspace,
        Invocation::with_args("tools.hash-files", json!({ "hash": "sha256" })),
        cx,
    )
    .unwrap();
    let chosen = vec![file.clone(), gone.clone()];
    cx.simulate_path_prompt_response(move |_| Some(chosen));
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx).len(), tabs + 1);
    let listing = active_text(&workspace, cx);
    assert!(
        listing.starts_with(&format!(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  {}",
            file.display()
        )),
        "{listing}"
    );
    assert!(
        listing.contains(&format!("# {}: ", gone.display())),
        "{listing}"
    );

    // Cancelled: no new document.
    run(
        &workspace,
        Invocation::with_args("tools.hash-files", json!({ "hash": "md5" })),
        cx,
    )
    .unwrap();
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx).len(), tabs + 1);
}
