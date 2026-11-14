use gpui_kit::TestAppContext;

use crate::workspace::open_workspace;
use birchpad::commands::*;
use birchpad_commands::Invocation;
use birchpad_commands::Keymap;
use birchpad_commands::Layer;
use birchpad_commands::Platform;
use birchpad_ui::key_bindings;
use birchpad_ui::replace_command_bindings;

#[test]
fn every_catalog_command_has_a_handler() {
    let registry = registry();
    assert_eq!(unregistered(&registry), Vec::<&str>::new());
}

#[gpui_kit::test]
fn default_keymap_converts_to_gpui_bindings(cx: &mut TestAppContext) {
    for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
        let keymap = Keymap::with_defaults(platform);
        assert_eq!(
            cx.update(|cx| key_bindings(&keymap, false, cx).len()),
            keymap.bindings().len()
        );
    }
    // On Linux, Alt+Shift+0 (Unfold All) and Ctrl+Alt+Shift+N (Clear Style) also get the
    // spelling X11 and Wayland report.
    let keymap = Keymap::with_defaults(Platform::Linux);
    let shifted = keymap
        .bindings()
        .iter()
        .filter(|binding| binding.shifted_symbol_keys().is_some())
        .count();
    assert!(shifted >= 15, "{shifted}");
    assert_eq!(
        cx.update(|cx| key_bindings(&keymap, true, cx).len()),
        keymap.bindings().len() + shifted
    );
}

#[gpui_kit::test]
fn an_empty_keymap_has_no_bindings(cx: &mut TestAppContext) {
    let keymap = Keymap::new(Platform::current());
    assert!(cx.update(|cx| key_bindings(&keymap, true, cx)).is_empty());
}

#[gpui_kit::test]
fn a_keymap_with_a_broken_binding_keeps_the_others(cx: &mut TestAppContext) {
    let mut keymap = Keymap::new(Platform::Windows);
    keymap.add_layer(
        Layer::User,
        "[[binding]]
keys = \"f7\"
command = \"file.open\"

         [[binding]]
keys = \"f8\"
command = \"file.save\"
context = \"Editor &&\"
",
    );
    let bindings = cx.update(|cx| key_bindings(&keymap, false, cx));
    let keys: Vec<String> = bindings
        .iter()
        .map(|binding| binding.keystrokes()[0].key().to_owned())
        .collect();
    assert_eq!(keys, ["f7"], "the binding with a bad context is left out");
}

#[gpui_kit::test]
fn command_bindings_are_replaced_and_the_others_kept(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let clear_style_2 = RunCommand(Invocation::with_args(
        "mark.clear",
        serde_json::json!({ "style": 2 }),
    ));
    let keys = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| {
            let keymap = cx.key_bindings();
            let keymap = keymap.borrow();
            let ours = keymap
                .bindings()
                .filter(|binding| binding.action().as_any().is::<RunCommand>())
                .count();
            let mut keys: Vec<String> = keymap
                .bindings_for_action(&clear_style_2)
                .map(|binding| binding.keystrokes()[0].key().to_owned())
                .collect();
            keys.sort();
            (keymap.bindings().len(), ours, keys)
        })
    };
    // Linux also reports Shift+2 as the symbol it types on a US layout (ADR 0014).
    let linux = Platform::current() == Platform::Linux;
    let symbol: &[&str] = if linux { &["@"] } else { &[] };
    let (all, ours, before) = keys(cx);
    assert_eq!(before, [&["2"], symbol].concat());

    cx.update(|_, cx| {
        let mut keymap = Keymap::with_defaults(Platform::current());
        keymap.add_layer(
            Layer::User,
            "[[binding]]
keys = \"f9\"
command = \"mark.clear\"
args = { style = 2 }
",
        );
        let bindings = key_bindings(&keymap, linux, cx);
        replace_command_bindings(bindings, cx);
    });
    let (all_after, ours_after, after) = keys(cx);
    assert_eq!(after, [&["2"], symbol, &["f9"]].concat(), "the new binding");
    assert_eq!(
        (all_after, ours_after),
        (all + 1, ours + 1),
        "the other bindings are kept"
    );

    // No command bindings at all: ours go, the component library's stay.
    cx.update(|_, cx| replace_command_bindings(Vec::new(), cx));
    let (all_none, ours_none, none) = keys(cx);
    assert_eq!((ours_none, none.len()), (0, 0));
    assert_eq!(all_none, all - ours);
}

#[gpui_kit::test]
fn linux_gets_the_shifted_symbol_too(cx: &mut TestAppContext) {
    let clear_style_2 = RunCommand(Invocation::with_args(
        "mark.clear",
        serde_json::json!({ "style": 2 }),
    ));
    // The Linux bindings, checked wherever the tests run.
    let linux_keys: Vec<String> = cx.update(|cx| {
        key_bindings(&Keymap::with_defaults(Platform::Linux), true, cx)
            .iter()
            .filter(|binding| binding.action().partial_eq(&clear_style_2))
            .map(|binding| binding.keystrokes()[0].key().to_owned())
            .collect()
    });
    assert_eq!(linux_keys, ["@", "2"]);
}
