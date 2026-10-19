use super::*;
use crate::Layer;
use serde_json::json;

const P: Platform = Platform::Windows;

fn effective(text: &str) -> Keymap {
    let mut keymap = Keymap::with_defaults(P);
    keymap.add_layer(Layer::User, text);
    assert!(
        keymap.diagnostics().is_empty(),
        "{:?}\n{text}",
        keymap.diagnostics()
    );
    keymap
}

fn keys_of(keymap: &Keymap, invocation: &Invocation) -> Vec<String> {
    keymap
        .bindings_for(invocation)
        .map(crate::Binding::keys_string)
        .collect()
}

#[test]
fn binds_keys_keeping_the_file() {
    let text = "# my keys\n[[binding]]\nkeys = \"ctrl-alt-x\"\ncommand = \"file.new\" # mine\n";
    let save = Invocation::new("file.save");
    let edited = bind(text, P, "ctrl-shift-k", None, &save).unwrap();
    assert!(edited.starts_with(text), "{edited}");
    let keymap = effective(&edited);
    assert!(keys_of(&keymap, &save).contains(&"ctrl-shift-k".to_owned()));
    // Binding the same keys again replaces the file's binding rather than adding one.
    let edited = bind(
        &edited,
        P,
        "ctrl-shift-k",
        None,
        &Invocation::new("file.close"),
    )
    .unwrap();
    assert_eq!(edited.matches("ctrl-shift-k").count(), 1, "{edited}");
    let keymap = effective(&edited);
    assert!(!keys_of(&keymap, &save).contains(&"ctrl-shift-k".to_owned()));
    // A context, and arguments.
    let case = Invocation::with_args("edit.convert-case", json!({ "to": "upper" }));
    let edited = bind("", P, "ctrl-alt-u", Some("Editor"), &case).unwrap();
    let keymap = effective(&edited);
    let binding = keymap
        .bindings_for(&case)
        .find(|b| b.layer == Layer::User)
        .unwrap();
    assert_eq!(binding.context.as_deref(), Some("Editor"));
}

#[test]
fn unbinds_file_and_default_keys() {
    let new = Invocation::new("file.new");
    let edited = unbind("", P, "ctrl-n", None).unwrap();
    assert!(edited.contains("unbind = true"), "{edited}");
    assert!(keys_of(&effective(&edited), &new).is_empty());
    // Keys only the file binds are simply removed from it.
    let bound = bind("", P, "ctrl-alt-q", None, &new).unwrap();
    let edited = unbind(&bound, P, "ctrl-alt-q", None).unwrap();
    assert_eq!(edited, "");
    // Keys nothing binds change nothing.
    assert_eq!(unbind("", P, "ctrl-alt-shift-f12", None).unwrap(), "");
}

#[test]
fn reset_gives_the_default_keys_back() {
    let new = Invocation::new("file.new");
    let text = unbind("", P, "ctrl-n", None).unwrap();
    let text = bind(&text, P, "ctrl-alt-q", None, &new).unwrap();
    let text = bind(&text, P, "ctrl-alt-w", None, &Invocation::new("file.close")).unwrap();
    let edited = reset(&text, P, &new).unwrap();
    let keymap = effective(&edited);
    assert_eq!(
        keys_of(&keymap, &new),
        keys_of(&Keymap::with_defaults(P), &new)
    );
    // Other commands keep their keys.
    assert!(keys_of(&keymap, &Invocation::new("file.close")).contains(&"ctrl-alt-w".to_owned()));
    // Arguments tell invocations apart.
    let upper = Invocation::with_args("edit.convert-case", json!({ "to": "upper" }));
    let lower = Invocation::with_args("edit.convert-case", json!({ "to": "lower" }));
    let text = bind("", P, "ctrl-alt-1", None, &upper).unwrap();
    let text = bind(&text, P, "ctrl-alt-2", None, &lower).unwrap();
    let edited = reset(&text, P, &upper).unwrap();
    assert!(
        !edited.contains("ctrl-alt-1") && edited.contains("ctrl-alt-2"),
        "{edited}"
    );
    // Nothing to reset.
    assert_eq!(reset("", P, &new).unwrap(), "");
}

#[test]
fn other_platforms_are_left_alone() {
    let text =
        "[[binding]]\nkeys = \"ctrl-alt-x\"\ncommand = \"file.new\"\nplatforms = [\"macos\"]\n";
    assert_eq!(unbind(text, P, "ctrl-alt-x", None).unwrap(), text);
    assert_eq!(reset(text, P, &Invocation::new("file.new")).unwrap(), text);
    let edited = bind(text, P, "ctrl-alt-x", None, &Invocation::new("file.save")).unwrap();
    assert!(edited.starts_with(text), "{edited}");
}

#[test]
fn refuses_what_it_cannot_write() {
    let new = Invocation::new("file.new");
    for keys in ["", "ctrl-", "ctrl-nosuchkey-x", "hyper-a"] {
        assert!(bind("", P, keys, None, &new).is_err(), "{keys:?}");
        assert!(unbind("", P, keys, None).is_err(), "{keys:?}");
    }
    assert!(bind("", P, "ctrl-alt-x", None, &Invocation::new("no.such")).is_err());
    for text in ["[[binding]", "binding = 3", "binding = [1]"] {
        assert!(bind(text, P, "ctrl-alt-x", None, &new).is_err(), "{text}");
        assert!(unbind(text, P, "ctrl-alt-x", None).is_err(), "{text}");
        assert!(reset(text, P, &new).is_err(), "{text}");
    }
}

#[test]
fn a_binding_for_one_platform() {
    let save = Invocation::new("file.save");
    let text = bind_on("", Platform::Windows, "ctrl-alt-q", None, &save, true).unwrap();
    assert!(text.contains("platforms = [\"windows\"]"), "{text}");
    let mut windows = Keymap::with_defaults(Platform::Windows);
    windows.add_layer(Layer::User, &text);
    let binding = windows
        .bindings_for(&save)
        .find(|binding| binding.keys_string() == "ctrl-alt-q")
        .expect("bound on Windows");
    assert_eq!(
        binding.platforms.as_deref(),
        Some(&["windows".to_owned()][..])
    );
    let mut linux = Keymap::with_defaults(Platform::Linux);
    linux.add_layer(Layer::User, &text);
    assert!(
        linux
            .bindings_for(&save)
            .all(|binding| binding.keys_string() != "ctrl-alt-q")
    );
    // Bound everywhere: no platforms.
    let text = bind_on("", Platform::Linux, "ctrl-k ctrl-s", None, &save, false).unwrap();
    assert!(!text.contains("platforms"), "{text}");
}
