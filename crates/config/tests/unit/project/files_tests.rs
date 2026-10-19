use super::*;

#[test]
fn every_file_once_in_order() {
    let file = |name: &str| ProjectItem::File(PathBuf::from(name));
    let workspace = ProjectWorkspace {
        projects: vec![
            ProjectFolder {
                name: "one".into(),
                items: vec![
                    file("/a"),
                    ProjectItem::Folder(ProjectFolder {
                        name: "inner".into(),
                        items: vec![file("/b"), file("/a")],
                    }),
                ],
            },
            ProjectFolder {
                name: "empty".into(),
                items: Vec::new(),
            },
            ProjectFolder {
                name: "two".into(),
                items: vec![file("/c")],
            },
        ],
    };
    assert_eq!(
        workspace.files(),
        [
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c")
        ]
    );
    assert!(ProjectWorkspace::default().files().is_empty());
}
