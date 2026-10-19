use super::*;

fn base() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\work")
    } else {
        PathBuf::from("/work")
    }
}

fn file(path: &str) -> ProjectItem {
    ProjectItem::File(base().join(path))
}

#[test]
fn reads_notepad_plus_plus_workspaces() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <Project name="Editor">
        <Folder name="Sources">
            <File name="src\main.rs" />
            <Folder name="Empty" />
        </Folder>
        <File name="README.md" />
        <File name="" />
        <Unknown name="x" />
    </Project>
    <Project name="Second &amp; last" />
</NotepadPlus>"#;
    let workspace = ProjectWorkspace::parse(xml, &base()).unwrap();
    let src = if cfg!(windows) {
        r"src\main.rs"
    } else {
        "src/main.rs"
    };
    assert_eq!(
        workspace,
        ProjectWorkspace {
            projects: vec![
                ProjectFolder {
                    name: "Editor".into(),
                    items: vec![
                        ProjectItem::Folder(ProjectFolder {
                            name: "Sources".into(),
                            items: vec![
                                file(src),
                                ProjectItem::Folder(ProjectFolder {
                                    name: "Empty".into(),
                                    items: vec![],
                                }),
                            ],
                        }),
                        file("README.md"),
                    ],
                },
                ProjectFolder {
                    name: "Second & last".into(),
                    items: vec![],
                },
            ],
        },
        "empty file names and unknown tags are left out"
    );
}

#[test]
fn paths_resolve_with_either_separator_and_dot_dots() {
    let resolved = |name: &str| resolve(name, &base());
    assert_eq!(resolved("a/b.txt"), base().join("a").join("b.txt"));
    assert_eq!(resolved(r"a\b.txt"), base().join("a").join("b.txt"));
    assert_eq!(resolved(r"a\..\.\c.txt"), base().join("c.txt"));
    let outside = resolved(r"..\other\d.txt");
    assert_eq!(
        outside,
        base().parent().unwrap().join("other").join("d.txt")
    );
    let absolute = if cfg!(windows) { r"D:\x.txt" } else { "/x.txt" };
    assert_eq!(resolved(absolute), PathBuf::from(absolute));
}

#[test]
fn writes_paths_inside_relative_and_round_trips() {
    let elsewhere = if cfg!(windows) {
        r"D:\other\x.txt"
    } else {
        "/other/x.txt"
    };
    let workspace = ProjectWorkspace {
        projects: vec![ProjectFolder {
            name: "<Tricky> \"name\" & 'quotes'\n".into(),
            items: vec![
                file("a/b.txt"),
                ProjectItem::File(PathBuf::from(elsewhere)),
                ProjectItem::Folder(ProjectFolder {
                    name: "Group".into(),
                    items: vec![file("c.txt")],
                }),
            ],
        }],
    };
    let xml = workspace.to_xml(&base());
    assert!(xml.contains(r#"<File name="a\b.txt" />"#), "{xml}");
    assert!(
        xml.contains(&format!("<File name=\"{elsewhere}\" />")),
        "{xml}"
    );
    assert!(xml.contains("&lt;Tricky&gt; &quot;name&quot; &amp; &apos;quotes&apos;&#10;"));
    assert_eq!(ProjectWorkspace::parse(&xml, &base()).unwrap(), workspace);
    assert_eq!(
        ProjectWorkspace::default().to_xml(&base()),
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n</NotepadPlus>\n"
    );
}

#[test]
fn a_file_named_as_the_workspace_folder_round_trips() {
    // `.` names the workspace's own folder; written relative it would be an empty name,
    // which reads as no file.
    let workspace = ProjectWorkspace::parse(
        r#"<NotepadPlus><Project name="P"><File name="." /></Project></NotepadPlus>"#,
        &base(),
    )
    .unwrap();
    assert_eq!(workspace.projects[0].items, vec![ProjectItem::File(base())]);
    let xml = workspace.to_xml(&base());
    assert_eq!(
        ProjectWorkspace::parse(&xml, &base()).unwrap(),
        workspace,
        "{xml}"
    );
}

#[test]
fn files_that_are_no_workspace_are_refused() {
    for (xml, reason) in [
        ("", "no XML"),
        ("<Session />", "another root"),
        ("<NotepadPlus><Project>", "unclosed"),
    ] {
        assert!(ProjectWorkspace::parse(xml, &base()).is_err(), "{reason}");
    }
    assert_eq!(
        ProjectWorkspace::parse("<NotepadPlus/>", &base()).unwrap(),
        ProjectWorkspace::default()
    );
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        ProjectWorkspace::load(&dir.path().join("missing.workspace")),
        Err(ProjectError::Read { .. })
    ));
    let broken = dir.path().join("broken.workspace");
    fs::write(&broken, "<Session/>").unwrap();
    let error = ProjectWorkspace::load(&broken).unwrap_err();
    assert!(
        error.to_string().contains("is not a Notepad++ workspace"),
        "{error}"
    );
    // Saving into a folder that does not exist fails without creating it.
    let nowhere = dir.path().join("no").join("x.workspace");
    assert!(matches!(
        ProjectWorkspace::default().save(&nowhere),
        Err(ProjectError::Write { .. })
    ));
}

#[test]
fn saved_workspaces_load_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.workspace");
    let workspace = ProjectWorkspace {
        projects: vec![ProjectFolder {
            name: "P".into(),
            items: vec![ProjectItem::File(dir.path().join("sub").join("f.txt"))],
        }],
    };
    workspace.save(&path).unwrap();
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains(r#"name="sub\f.txt""#)
    );
    assert_eq!(ProjectWorkspace::load(&path).unwrap(), workspace);
}
