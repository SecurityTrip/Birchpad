use birchpad_commands::Placeholder;

use birchpad::menus::*;
use birchpad_commands::Invocation;
use birchpad_commands::Menu;
use birchpad_commands::MenuItem;

#[test]
fn labels_of_every_kind_of_item() {
    birchpad::i18n::set_language("en");
    let labelled = MenuItem::Command {
        invocation: Invocation::new("file.new"),
        label: Some("Fresh".into()),
    };
    assert_eq!(
        menu_label(&labelled),
        "Fresh",
        "its own label, untranslated"
    );
    assert_eq!(menu_label(&MenuItem::command("file.new")), "New");
    let submenu = MenuItem::Submenu(Menu::new("Recent Files", Vec::new()));
    assert_eq!(menu_label(&submenu), "Recent Files");
    assert_eq!(menu_label(&MenuItem::Separator), "");
    assert_eq!(
        menu_label(&MenuItem::Placeholder(Placeholder::RecentFiles)),
        ""
    );
}

/// The macOS layout, on every platform: About, Check for Updates and Quit move to an
/// application menu, and no menu starts or ends with a separator.
#[test]
fn the_macos_application_menu() {
    let menus = mac_layout(birchpad_commands::main_menu());
    let app = &menus[0];
    assert_eq!(app.title, "Birchpad");
    let commands = |menu: &Menu| -> Vec<String> {
        menu.items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Command { invocation, .. } => Some(invocation.command.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        commands(app),
        ["help.about", "help.check-updates", "file.exit"]
    );
    for menu in &menus[1..] {
        for moved in ["help.about", "help.check-updates", "file.exit"] {
            assert!(
                !commands(menu).iter().any(|id| id == moved),
                "{}: {moved}",
                menu.title
            );
        }
        assert!(
            !matches!(menu.items.first(), Some(MenuItem::Separator)),
            "{}",
            menu.title
        );
        assert!(
            !matches!(menu.items.last(), Some(MenuItem::Separator)),
            "{}",
            menu.title
        );
    }
    // A menu left with separators only goes; an empty model gets the application menu.
    let only_app = Menu::new(
        "Help",
        vec![
            MenuItem::Separator,
            MenuItem::command("help.about"),
            MenuItem::Separator,
        ],
    );
    let menus = mac_layout(vec![only_app]);
    assert_eq!(menus.len(), 1);
    assert_eq!(mac_layout(Vec::new()).len(), 1);
}
