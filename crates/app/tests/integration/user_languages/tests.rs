use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::user_languages::*;
use birchpad::workspace::Workspace;
use birchpad_syntax::UdlKind;
use birchpad_syntax::UdlStyle;

const MINI: &str = r#"<NotepadPlus><UserLang name="Mini" ext="mini"><KeywordLists><Keywords name="Keywords1">if</Keywords><Keywords name="Comments">00#</Keywords></KeywordLists><Styles><WordsStyle name="KEYWORDS1" fgColor="FF0000" fontStyle="1"/><WordsStyle name="LINE COMMENTS" fgColor="00FF00"/></Styles></UserLang></NotepadPlus>"#;

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("userDefineLangs")).unwrap();
    std::fs::write(dir.path().join("userDefineLangs").join("mini.xml"), MINI).unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        init(cx);
    });
    (workspace, cx, dir)
}

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> anyhow::Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

#[test]
fn folders_and_imports() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.xml"), MINI).unwrap();
    std::fs::write(dir.path().join("a.xml"), MINI.replace("Mini", "Alpha")).unwrap();
    std::fs::write(dir.path().join("dup.xml"), MINI).unwrap();
    std::fs::write(dir.path().join("bad.xml"), "<x").unwrap();
    std::fs::write(dir.path().join("note.txt"), MINI).unwrap();
    let names: Vec<_> = read_folder(dir.path())
        .iter()
        .map(|language| language.name.clone())
        .collect();
    assert_eq!(names, ["Alpha", "Mini"]);
    assert!(read_folder(&dir.path().join("missing")).is_empty());
    let target = dir.path().join("settings").join("userDefineLangs");
    let (count, errors) = import(
        &[
            dir.path().join("b.xml"),
            dir.path().join("bad.xml"),
            dir.path().join("gone.xml"),
        ],
        &target,
    );
    assert_eq!((count, errors.len()), (1, 2));
    assert!(target.join("b.xml").is_file());
    assert!(!target.join("bad.xml").exists());
    assert_eq!(import(&[], &target), (0, vec![]));
}

#[test]
fn styles_become_text_styles() {
    let style = text_style(UdlStyle {
        color: Some(0x123456),
        background: None,
        bold: true,
        italic: false,
        underline: true,
    });
    assert_eq!(style.color, Some(0x123456ff));
    assert_eq!(style.background, None);
    assert!(style.bold && style.underline && !style.italic);
}

#[test]
fn the_theme_colors_what_the_language_leaves_unset() {
    let language = birchpad_syntax::parse_udl(
        r#"<NotepadPlus><UserLang name="T"><Styles>
            <WordsStyle name="COMMENTS" fgColor="00FF00" colorStyle="1" />
            <WordsStyle name="KEYWORDS1" bgColor="FFFF00" colorStyle="2" fontStyle="1" />
        </Styles></UserLang></NotepadPlus>"#,
    )
    .unwrap()
    .remove(0);
    let theme = |name: &str| {
        birchpad::editor::theme::syntax_style(None, birchpad_syntax::resolve(name).unwrap())
    };
    // Its own color stays.
    let comment = style_of(&language, UdlKind::Comment).unwrap();
    assert_eq!(comment.color, Some(0x00ff00ff));
    // A style without a color: the theme's color, its own background and boldness.
    let keyword = style_of(&language, UdlKind::Keyword(0)).unwrap();
    assert_eq!(
        keyword.color,
        theme("keyword").and_then(|style| style.color)
    );
    assert_eq!(keyword.background, Some(0xffff00ff));
    assert!(keyword.bold);
    // No style at all: the theme's.
    assert_eq!(style_of(&language, UdlKind::Number), theme("number"));
    assert_eq!(style_of(&language, UdlKind::Delimiter(7)), theme("string"));
    assert_eq!(theme_highlight(UdlKind::LineComment), "comment");
    assert_eq!(theme_highlight(UdlKind::FolderCode2), "keyword");
}

#[gpui_kit::test]
fn chosen_detected_and_remembered(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    cx.update(|_, cx| {
        assert_eq!(names(cx), ["Mini"]);
        assert!(for_file("x.MINI", cx).is_some());
        assert!(for_file("x.txt", cx).is_none());
        assert!(find("Nope", cx).is_none());
    });
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "if x # note" })),
        cx,
    )
    .unwrap();
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "udl:Mini" })),
        cx,
    )
    .unwrap();
    let buffer = workspace.read_with(cx, |workspace, cx| {
        workspace.active_view(cx).unwrap().read(cx).buffer.clone()
    });
    let pieces = buffer.read_with(cx, |buffer, _| buffer.udl_pieces(100));
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0].0, 0..2);
    let status = workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| {
            birchpad::status_bar::StatusInfo::of(view, cx).language
        })
    });
    assert_eq!(status.as_deref(), Some("User defined language file - Mini"));
    // An unknown one is refused; a built-in language replaces it.
    assert!(
        run(
            &workspace,
            Invocation::with_args("language.set", json!({ "language": "udl:Nope" })),
            cx
        )
        .is_err()
    );
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    assert!(buffer.read_with(cx, |buffer, _| buffer.user_language().is_none()));
    assert!(buffer.read_with(cx, |buffer, _| buffer.udl_pieces(100).is_empty()));
    // A file with its extension gets it.
    let path = dir.path().join("prog.mini");
    std::fs::write(&path, "if y").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    let opened = workspace.read_with(cx, |workspace, cx| {
        workspace.active_view(cx).unwrap().read(cx).buffer.clone()
    });
    assert_eq!(
        opened.read_with(cx, |buffer, _| buffer
            .user_language()
            .map(|found| found.name.clone())),
        Some("Mini".to_owned())
    );
}

#[gpui_kit::test]
fn the_folder_and_the_collection_open(cx: &mut TestAppContext) {
    use birchpad::file_commands::{SHELL, Shell};
    let (workspace, cx, dir) = setup(cx);
    let taken = || SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut()));
    taken();
    std::fs::remove_dir_all(dir.path().join("userDefineLangs")).unwrap();
    run(&workspace, Invocation::new("language.open-udl-folder"), cx).unwrap();
    assert_eq!(taken(), [Shell::Open(dir.path().join("userDefineLangs"))]);
    assert!(
        dir.path().join("userDefineLangs").is_dir(),
        "made when missing"
    );
    run(&workspace, Invocation::new("language.udl-collection"), cx).unwrap();
    assert_eq!(taken(), [Shell::Browse(COLLECTION_URL.to_owned())]);

    // Without a settings folder there is no folder to open.
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = None);
    assert!(run(&workspace, Invocation::new("language.open-udl-folder"), cx).is_err());
    assert!(taken().is_empty());
}
