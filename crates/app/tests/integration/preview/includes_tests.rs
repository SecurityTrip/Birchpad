//! Tests of the includes the preview follows (ADR 0062): paths written with the preprocessor's
//! variables and functions worked out as PlantUML works them out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use birchpad::preview::{IncludeLink, MOST_INCLUDED_FILES, Preprocessor, walk_includes};
use gpui_kit::TestAppContext;

use crate::workspace::open_workspace;

/// A preprocessor that has read `text`'s lines, through a walk of its includes.
fn after(text: &str) -> Preprocessor {
    let mut preprocessor = Preprocessor::default();
    // `walk_includes` keeps its preprocessor; this reads the same lines with a fresh one.
    preprocessor.read(text, None);
    preprocessor
}

#[test]
fn variables_and_functions_give_their_values() {
    let preprocessor = after(
        "!$HOME ?= \"../..\"\n!$HOME ?= \"ignored\"\n!$LIB = $HOME + '/lib'\n\
         !global $N = 2\n\
         !function FromRoot($p)\n  !return $HOME + $p\n!endfunction\n\
         !function $twice($a, $b = \"x\") !return $a + $b + $a\n\
         !procedure $draw($a)\n  !$HOME = \"in a procedure\"\n!endprocedure\n",
    );
    let none = HashMap::new();
    let value = |expr: &str| preprocessor.evaluate(expr, &none, None);
    assert_eq!(
        value("$HOME").as_deref(),
        Some("../.."),
        "?= keeps the first"
    );
    assert_eq!(value("$LIB").as_deref(), Some("../../lib"));
    assert_eq!(value("$N").as_deref(), Some("2"));
    assert_eq!(
        value("FromRoot(\"/a.puml\")").as_deref(),
        Some("../../a.puml")
    );
    assert_eq!(value("$twice(\"a\")").as_deref(), Some("axa"), "a default");
    assert_eq!(value("$twice('a', 'b')").as_deref(), Some("aba"));
    assert_eq!(
        value("\"x + y\" + 'z'").as_deref(),
        Some("x + yz"),
        "a + in quotes"
    );
    // What is not known has no value.
    for unknown in [
        "$NOPE",
        "Nope(1)",
        "$HOME +",
        "%filename()",
        "$HOME $LIB",
        "bare",
    ] {
        assert_eq!(value(unknown), None, "{unknown}");
    }
}

#[test]
fn include_lines_get_their_variables_and_calls() {
    let preprocessor =
        after("!$HOME = \"../..\"\n!function FromRoot($p)\n!return $HOME + $p\n!endfunction\n");
    let folder = std::env::temp_dir();
    let substitute = |text: &str| preprocessor.substitute(text, Some(&folder));
    assert_eq!(
        substitute("$HOME/library/main.iuml").as_deref(),
        Some("../../library/main.iuml")
    );
    assert_eq!(
        substitute("FromRoot(/parts/billing/containers.puml)").as_deref(),
        Some("../../parts/billing/containers.puml"),
        "an argument as written"
    );
    assert_eq!(
        substitute("plain/file.puml").as_deref(),
        Some("plain/file.puml")
    );
    assert_eq!(
        substitute("Other(x).puml").as_deref(),
        Some("Other(x).puml"),
        "a call of no known function is text"
    );
    assert_eq!(
        substitute("%dirpath()/a.puml").map(PathBuf::from),
        Some(PathBuf::from(format!("{}/a.puml", folder.display())))
    );
    assert_eq!(substitute("$UNSET/a.puml"), None);
    assert_eq!(substitute("").as_deref(), Some(""));
}

#[test]
fn calls_that_never_end_have_no_value() {
    let preprocessor = after("!function $loop($a) !return $loop($a)\n");
    assert_eq!(
        preprocessor.evaluate("$loop(1)", &HashMap::new(), None),
        None
    );
}

/// The repository's layout: a document under `views`, a library under `repository`, both
/// reached through `$DIAGRAMS_ROOT` and a function of its own.
fn repository() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let views = root.join("views").join("context");
    let services = root.join("library").join("services");
    let billing = root.join("parts").join("billing");
    for folder in [&views, &services, &billing] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(
        root.join("library").join("main.iuml"),
        "!function FromRoot($p)\n!return $DIAGRAMS_ROOT + $p\n!endfunction\n\
         !include $DIAGRAMS_ROOT/library/services/all.iuml\n",
    )
    .unwrap();
    std::fs::write(services.join("all.iuml"), "class Service\n").unwrap();
    std::fs::write(billing.join("containers.puml"), "class Billing\n").unwrap();
    (dir, views)
}

const DOCUMENT: &str = "@startuml\n!$DIAGRAMS_ROOT ?= \"../..\"\n\
                        !include $DIAGRAMS_ROOT/library/main.iuml\n\
                        !include FromRoot(/parts/billing/containers.puml)\n@enduml\n";

#[test]
fn includes_through_variables_and_functions_are_followed() {
    let (dir, views) = repository();
    let root = dir.path();
    let found = walk_includes(DOCUMENT, Some(&views), std::slice::from_ref(&views));
    assert_eq!(
        found.files,
        [
            root.join("library").join("main.iuml"),
            root.join("library").join("services").join("all.iuml"),
            root.join("parts").join("billing").join("containers.puml"),
        ]
    );
    // The function comes from the included file, read before the line that calls it.
    assert_eq!(
        found.links[2],
        IncludeLink {
            from: None,
            written: "FromRoot(/parts/billing/containers.puml)".into(),
            path: root.join("parts").join("billing").join("containers.puml"),
        }
    );
    assert_eq!(
        found.links[1].from,
        Some(root.join("library").join("main.iuml"))
    );
}

#[test]
fn includes_that_cannot_be_worked_out_are_left_out() {
    let (_dir, views) = repository();
    // The variable is set after the include, and the function is never defined.
    let text = "!include $LATER/library/main.iuml\n!$LATER = \"../..\"\n\
                !include Unknown(/x.puml)\n!include <C4/C4_Context>\n";
    assert!(walk_includes(text, Some(&views), &[]).files.is_empty());
    assert_eq!(walk_includes("", None, &[]), Default::default());
    // A quoted path with a part.
    std::fs::write(views.join("with space.iuml"), "").unwrap();
    assert_eq!(
        walk_includes(
            "!include \"with space.iuml\"\n!include with space.iuml!2",
            Some(&views),
            &[]
        )
        .files,
        [views.join("with space.iuml")]
    );
}

#[test]
fn walks_end_at_their_limit() {
    let dir = tempfile::tempdir().unwrap();
    let mut text = String::from("!$DIR = \".\"\n");
    for index in 0..=MOST_INCLUDED_FILES {
        std::fs::write(dir.path().join(format!("{index}.iuml")), "").unwrap();
        text.push_str(&format!("!include $DIR/{index}.iuml\n"));
    }
    let found = walk_includes(&text, Some(dir.path()), &[]);
    assert_eq!(found.files.len(), MOST_INCLUDED_FILES);
}

#[gpui_kit::test]
fn ctrl_click_on_an_include_through_a_function_opens_its_file(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let (dir, views) = repository();
    let document = views.join("context.puml");
    std::fs::write(&document, DOCUMENT).unwrap();
    let invocation = birchpad_commands::Invocation::with_args(
        "preview.open-include",
        serde_json::json!({
            "target": "FromRoot(/parts/billing/containers.puml)",
            "document": document,
        }),
    );
    let opened = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    opened.unwrap();
    let active = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).path().map(Path::to_path_buf)
    });
    assert_eq!(
        active,
        Some(
            dir.path()
                .join("parts")
                .join("billing")
                .join("containers.puml")
        )
    );
}

/// A function shaped as the one 7.4y is about: unquoted, with a `!local` variable from
/// `%get_variable_value` and its `!return`s in `!if`, `!elseif` and `!else`.
const FROM_ROOT: &str = "!unquoted function FromRoot($path)\n\
                         !local $branch = %get_variable_value(\"$BRANCH\")\n\
                         !if %strpos($branch, \"release\") >= 0\n\
                         !return $DIAGRAMS_ROOT + \"/released\" + $path\n\
                         !elseif $branch == \"\"\n\
                         !return $DIAGRAMS_ROOT + %string($path)\n\
                         !else\n\
                         !return $DIAGRAMS_ROOT + \"/\" + $branch + $path\n\
                         !endif\n\
                         !endfunction\n";

#[test]
fn functions_run_their_locals_and_branches() {
    let call = |branch: Option<&str>| {
        let mut text = String::from("!$DIAGRAMS_ROOT = \"../..\"\n");
        if let Some(branch) = branch {
            text.push_str(&format!("!$BRANCH = \"{branch}\"\n"));
        }
        text.push_str(FROM_ROOT);
        after(&text).substitute("FromRoot(/parts/billing/containers.puml)", None)
    };
    assert_eq!(
        call(None).as_deref(),
        Some("../../parts/billing/containers.puml"),
        "no branch: the !elseif"
    );
    assert_eq!(
        call(Some("release-2")).as_deref(),
        Some("../../released/parts/billing/containers.puml"),
        "the !if"
    );
    assert_eq!(
        call(Some("main")).as_deref(),
        Some("../../main/parts/billing/containers.puml"),
        "the !else"
    );
}

#[test]
fn nested_branches_and_branches_not_taken() {
    let preprocessor = after(
        "!function $pick($a, $b)\n\
         !if $a == 1\n  !if $b == 1\n    !return \"both\"\n  !else\n    !return \"a\"\n  !endif\n\
         !elseif $b == 1\n  !return \"b\"\n!endif\n!return \"none\"\n!endfunction\n\
         !function $never()\n!if 0\n!return \"x\"\n!endif\n!endfunction\n\
         !function $unknown()\n!if $NOPE == 1\n!return \"x\"\n!endif\n!return \"y\"\n!endfunction\n",
    );
    let value = |expr: &str| preprocessor.evaluate(expr, &HashMap::new(), None);
    assert_eq!(value("$pick(1, 1)").as_deref(), Some("both"));
    assert_eq!(value("$pick(1, 0)").as_deref(), Some("a"));
    assert_eq!(value("$pick(0, 1)").as_deref(), Some("b"));
    assert_eq!(
        value("$pick(0, 0)").as_deref(),
        Some("none"),
        "after the !endif"
    );
    assert_eq!(value("$never()"), None, "no !return reached");
    assert_eq!(
        value("$unknown()"),
        None,
        "a condition that cannot be worked out"
    );
}

#[test]
fn built_in_functions_and_operators() {
    let preprocessor = after("!$S = \"a/b/c\"\n!$N = 3\n");
    let value = |expr: &str| preprocessor.evaluate(expr, &HashMap::new(), None);
    for (expr, expected) in [
        ("%strpos($S, \"/\")", "1"),
        ("%strpos($S, \"x\")", "-1"),
        ("%strpos(\"ж/\", \"/\")", "1"),
        ("%string($N)", "3"),
        ("%strlen($S)", "5"),
        ("%substr($S, 2)", "b/c"),
        ("%substr($S, 2, 1)", "b"),
        ("%get_variable_value(\"$S\")", "a/b/c"),
        ("%get_variable_value(\"$NONE\")", ""),
        ("%variable_exists(\"$N\")", "1"),
        ("%variable_exists(\"$NONE\")", "0"),
        ("%intval(\"12\")", "12"),
        ("%upper(\"ab\")", "AB"),
        ("%lower(\"AB\")", "ab"),
        ("%not(0)", "1"),
        ("%true() && %false()", "0"),
        ("%true() || %false()", "1"),
        ("!0", "1"),
        ("$N + 2", "5"),
        ("$N - 5", "-2"),
        ("$N + \"x\"", "3x"),
        ("($N + 1) * 2", "<none>"),
        ("$N >= 3 && $N < 4", "1"),
        ("10 > 9", "1"),
        ("\"10\" == \"10\"", "1"),
        ("\"b\" > \"a\"", "1"),
        ("1 != 1", "0"),
    ] {
        let expected = (expected != "<none>").then(|| expected.to_owned());
        assert_eq!(value(expr), expected, "{expr}");
    }
    // What cannot be worked out has no value.
    for unknown in [
        "%substr($S, -1)",
        "\"x\" - 1",
        "%intval(\"x\")",
        "(1",
        "\"open",
    ] {
        assert_eq!(value(unknown), None, "{unknown}");
    }
}

#[test]
fn includes_through_such_a_function_are_followed() {
    let (dir, views) = repository();
    let root = dir.path();
    std::fs::write(root.join("fullpath.iuml"), FROM_ROOT).unwrap();
    let text = "!$DIAGRAMS_ROOT ?= \"../..\"\n!include $DIAGRAMS_ROOT/fullpath.iuml\n\
                !include FromRoot(/parts/billing/containers.puml)\n";
    let found = walk_includes(text, Some(&views), std::slice::from_ref(&views));
    assert_eq!(
        found.files,
        [
            root.join("fullpath.iuml"),
            root.join("parts").join("billing").join("containers.puml"),
        ]
    );
}
