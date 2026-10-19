use super::*;

#[test]
fn has_notepad_plus_plus_top_level_menus() {
    let titles: Vec<String> = main_menu().into_iter().map(|menu| menu.title).collect();
    assert_eq!(
        titles,
        [
            "File", "Edit", "Search", "View", "Encoding", "Language", "Settings", "Tools", "Macro",
            "Run", "Plugins", "Window", "Help"
        ]
    );
}

#[test]
fn labels_come_from_the_item_or_the_catalog() {
    let titled = MenuItem::Command {
        invocation: Invocation::new("file.new"),
        label: None,
    };
    assert_eq!(titled.label(), Some("New"));
    let labelled = MenuItem::Command {
        invocation: Invocation::new("file.new"),
        label: Some("Another".into()),
    };
    assert_eq!(labelled.label(), Some("Another"));
    // A command the catalog does not know has no label of its own; a separator none.
    let unknown = MenuItem::Command {
        invocation: Invocation::new("no.such"),
        label: None,
    };
    assert_eq!(unknown.label(), None);
    assert_eq!(MenuItem::Separator.label(), None);
}

#[test]
fn every_menu_command_exists_and_has_a_label() {
    for menu in main_menu() {
        for invocation in menu.invocations() {
            assert!(
                catalog::find(&invocation.command).is_some(),
                "unknown command {} in menu {}",
                invocation.command,
                menu.title
            );
        }
        fn check(menu: &Menu) {
            for item in &menu.items {
                match item {
                    MenuItem::Command { .. } => assert!(item.label().is_some()),
                    MenuItem::Submenu(sub) => check(sub),
                    _ => {}
                }
            }
        }
        check(&menu);
    }
}
