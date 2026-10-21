use birchpad_plugin::wasm::*;

fn command(id: &str, title: &str) -> PluginCommand {
    PluginCommand {
        id: id.into(),
        title: title.into(),
        context_menu: false,
    }
}

#[test]
fn checks_the_commands_a_plugin_adds() {
    let checked = check_commands(vec![command("a", " A "), command("b-2", "B")]).unwrap();
    assert_eq!(checked[0].title, "A");
    // Whether it goes into the context menu is kept.
    let in_menu = PluginCommand {
        context_menu: true,
        ..command("menu", "In the menu")
    };
    assert!(check_commands(vec![in_menu]).unwrap()[0].context_menu);
    assert!(check_commands(Vec::new()).unwrap().is_empty());
    for commands in [
        vec![command("A", "x")],
        vec![command("", "x")],
        vec![command("a", "x"), command("a", "y")],
        vec![command("a", " ")],
        vec![command(
            "a",
            "two
lines",
        )],
    ] {
        assert!(check_commands(commands).is_err());
    }
    let most: Vec<_> = (0..MAX_COMMANDS)
        .map(|i| command(&format!("c{i}"), "x"))
        .collect();
    assert!(check_commands(most.clone()).is_ok());
    let too_many = [most, vec![command("one-more", "x")]].concat();
    assert!(check_commands(too_many).is_err());
}
