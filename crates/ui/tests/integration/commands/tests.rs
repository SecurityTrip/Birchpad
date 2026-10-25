use gpui_kit::{Render, TestAppContext};
use serde::Deserialize;

use birchpad_commands::Invocation;
use birchpad_commands::Scope;
use birchpad_ui::*;
use gpui_kit::Context;
use gpui_kit::Window;

/// A view of the workspace's scope that notes what it ran.
#[derive(Default)]
struct Workspace(Vec<String>);

impl CommandTarget for Workspace {
    const SCOPE: Scope = Scope::Workspace;
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
    }
}

/// A view of the editor's scope.
#[derive(Default)]
struct Editor(String);

impl CommandTarget for Editor {
    const SCOPE: Scope = Scope::Editor;
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
    }
}

#[derive(Deserialize)]
struct TextArgs {
    text: String,
}

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::default();
    registry.register("file.new", |this: &mut Workspace, (), _, _| {
        this.0.push("new".to_owned());
        Ok(())
    });
    registry.register(
        "edit.insert-text",
        |this: &mut Editor, args: TextArgs, _, _| {
            this.0.push_str(&args.text);
            Ok(())
        },
    );
    registry
}

#[gpui_kit::test]
fn each_target_gets_its_own_handlers(cx: &mut TestAppContext) {
    let registry = registry();
    assert!(registry.contains("file.new") && registry.contains("edit.insert-text"));
    assert!(!registry.contains("file.save"), "not registered");
    assert!(registry.handler::<Workspace>("edit.insert-text").is_none());
    assert!(registry.handler::<Editor>("file.new").is_none());
    assert!(registry.handler::<Editor>("no.such.command").is_none());

    let window = cx.add_window(|_, _| Workspace::default());
    let run = registry.handler::<Workspace>("file.new").unwrap();
    window
        .update(cx, |this, window, cx| {
            run(this, &Invocation::new("file.new"), window, cx)
        })
        .unwrap()
        .unwrap();
    let ran = window.update(cx, |this, _, _| this.0.clone()).unwrap();
    assert_eq!(ran, ["new"]);

    let window = cx.add_window(|_, _| Editor::default());
    let run = registry.handler::<Editor>("edit.insert-text").unwrap();
    let typed = Invocation::with_args("edit.insert-text", serde_json::json!({ "text": "hi" }));
    let wrong = Invocation::with_args("edit.insert-text", serde_json::json!({ "text": 5 }));
    window
        .update(cx, |this, window, cx| {
            run(this, &typed, window, cx).unwrap();
            // Arguments that do not decode are an error, and nothing runs.
            assert!(run(this, &wrong, window, cx).is_err());
            assert!(run(this, &Invocation::new("edit.insert-text"), window, cx).is_err());
            assert_eq!(this.0, "hi");
        })
        .unwrap();
}

#[test]
#[should_panic(expected = "registered in the wrong scope")]
fn a_handler_is_registered_in_its_commands_scope() {
    CommandRegistry::default().register("edit.undo", |_: &mut Workspace, (), _, _| Ok(()));
}

#[test]
#[should_panic(expected = "is not in the catalog")]
fn only_catalog_commands_have_handlers() {
    CommandRegistry::default().register("no.such.command", |_: &mut Workspace, (), _, _| Ok(()));
}

#[derive(Deserialize)]
struct OptionalArgs {
    #[serde(default)]
    _count: usize,
}

#[test]
fn commands_whose_arguments_all_have_defaults_run_bare() {
    let mut registry = registry();
    registry.register("edit.redo", |_: &mut Editor, _: OptionalArgs, _, _| Ok(()));
    assert!(registry.runs_bare("file.new"));
    assert!(registry.runs_bare("edit.redo"));
    // Its text is required.
    assert!(!registry.runs_bare("edit.insert-text"));
    // Not registered.
    assert!(!registry.runs_bare("file.save"));
    assert!(!registry.runs_bare("no.such.command"));
}

#[test]
#[should_panic(expected = "registered twice")]
fn a_command_has_one_handler() {
    let mut registry = registry();
    registry.register("file.new", |_: &mut Workspace, (), _, _| Ok(()));
}

#[test]
#[should_panic(expected = "is implemented")]
fn a_command_with_a_handler_is_not_pending() {
    registry().pending("file.new");
}

#[test]
#[should_panic(expected = "is not registered")]
fn only_registered_commands_are_turned_off() {
    registry().enabled_when("file.save", |_| false);
}

#[gpui_kit::test]
fn pending_and_turned_off_commands_cannot_run(cx: &mut TestAppContext) {
    let mut registry = registry();
    registry.pending("file.save");
    assert!(registry.is_pending("file.save") && !registry.is_pending("file.new"));
    registry.enabled_when("edit.insert-text", |_| false);
    cx.update(|cx| {
        assert!(registry.is_enabled("file.new", cx));
        assert!(!registry.is_enabled("file.save", cx), "pending");
        assert!(!registry.is_enabled("edit.insert-text", cx), "turned off");
        assert!(
            registry.is_enabled("no.such.command", cx),
            "nothing says otherwise"
        );
    });
}
