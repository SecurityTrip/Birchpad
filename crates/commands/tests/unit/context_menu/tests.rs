use serde_json::json;

use super::*;

fn command(node: &Node) -> Option<(&str, &serde_json::Value)> {
    match node {
        Node::Command { invocation, .. } => Some((&invocation.command, &invocation.args)),
        _ => None,
    }
}

#[test]
fn the_default_menu() {
    let nodes = parse(DEFAULT).unwrap();
    assert_eq!(command(&nodes[0]).unwrap().0, "edit.cut");
    let folders: Vec<(&str, usize)> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::Folder { name, items } => Some((name.as_str(), items.len())),
            _ => None,
        })
        .collect();
    assert_eq!(
        folders,
        [
            ("Style All Occurrences of Token", 5),
            ("Convert Case to", 2)
        ]
    );
    assert_eq!(
        command(nodes.last().unwrap()).unwrap().0,
        "edit.block-comment"
    );
    // Every command of it exists.
    fn check(nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Command { invocation, .. } => {
                    assert!(crate::find(&invocation.command).is_some())
                }
                Node::Folder { items, .. } => check(items),
                Node::Separator => {}
            }
        }
    }
    check(&nodes);
}

#[test]
fn items_labels_arguments_and_folders() {
    let nodes = parse(
        r#"
            [[item]]
            separator = true
            [[item]]
            command = "edit.copy"
            label = "Copy it"
            [[item]]
            command = "no.such"
            [[item]]
            separator = true
            [[item]]
            separator = true
            [[item]]
            command = "mark.style-all"
            args = { style = 2 }
            folder = "Styles"
            [[item]]
            command = "nothing.here"
            folder = "Empty"
            [[item]]
            command = "edit.paste"
            [[item]]
            command = "edit.cut"
            folder = "Styles"
            [[item]]
            [[item]]
            separator = true
            "#,
    )
    .unwrap();
    assert_eq!(nodes.len(), 5, "{nodes:#?}");
    assert_eq!(
        nodes[0],
        Node::Command {
            invocation: Invocation::new("edit.copy"),
            label: Some("Copy it".into())
        }
    );
    assert_eq!(nodes[1], Node::Separator);
    let Node::Folder { name, items } = &nodes[2] else {
        panic!("{:?}", nodes[2]);
    };
    assert_eq!(name, "Styles");
    assert_eq!(
        command(&items[0]).unwrap(),
        ("mark.style-all", &json!({ "style": 2 }))
    );
    assert_eq!(command(&nodes[3]).unwrap().0, "edit.paste");
    // The same folder after other items is another submenu.
    assert!(matches!(&nodes[4], Node::Folder { name, .. } if name == "Styles"));

    assert!(parse("").unwrap().is_empty());
}

#[test]
fn broken_files_are_refused() {
    for text in [
        "[[item]\ncommand = 1",
        "[[item]]\ncommand = 1",
        "[[item]]\ncomand = \"edit.copy\"",
        "item = 3",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

#[test]
fn folders_of_one_name_meet_when_an_empty_one_goes() {
    let nodes = parse(
        r#"
            [[item]]
            command = "edit.copy"
            folder = "A"
            [[item]]
            separator = true
            folder = "B"
            [[item]]
            command = "edit.paste"
            folder = "A"
            "#,
    )
    .unwrap();
    assert_eq!(nodes.len(), 1, "{nodes:#?}");
    assert!(matches!(&nodes[0], Node::Folder { items, .. } if items.len() == 2));
    assert_eq!(parse(&write(&nodes)).unwrap(), nodes);
}

#[test]
fn written_menus_read_back_the_same() {
    let default = parse(DEFAULT).unwrap();
    assert_eq!(parse(&write(&default)).unwrap(), default);
    let nodes = vec![
        Node::Folder {
            name: "Tools".into(),
            items: vec![
                Node::Command {
                    invocation: Invocation::with_args("mark.style-all", json!({ "style": 3 })),
                    label: Some("Third".into()),
                },
                Node::Separator,
                Node::Command {
                    invocation: Invocation::new("edit.copy"),
                    label: None,
                },
            ],
        },
        Node::Separator,
        Node::Command {
            invocation: Invocation::new("edit.paste"),
            label: None,
        },
    ];
    assert_eq!(parse(&write(&nodes)).unwrap(), nodes);
    assert_eq!(
        write(&[]),
        "item = []
"
    );
    assert!(parse(&write(&[])).unwrap().is_empty());
}
