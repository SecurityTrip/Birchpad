//! The application's commands: every handler registered in `birchpad_ui`'s command registry,
//! for the workspace or the editor, and the keymap that binds keys to them.

use anyhow::Result;
#[cfg(test)]
use birchpad_commands::{Invocation, Platform};
use birchpad_commands::{Keymap, Scope};
pub(crate) use birchpad_ui::{
    ActiveKeymap, CommandRegistry, CommandTarget, RunCommand, reload_keymap,
};
#[cfg(test)]
use birchpad_ui::{key_bindings, replace_command_bindings};
#[cfg(test)]
use gpui_kit::PlatformKeyboardMapper;
use gpui_kit::{App, Context, Window};
use serde::de::DeserializeOwned;

use crate::editor::EditorView;
use crate::workspace::Workspace;

impl CommandTarget for Workspace {
    const SCOPE: Scope = Scope::Workspace;
}

/// Registering the handlers of the workspace and of the editor.
pub(crate) trait Register {
    /// Registers a workspace command whose arguments decode to `A` (`()` for none).
    fn workspace<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut Workspace, A, &mut Window, &mut Context<Workspace>) -> Result<()> + 'static,
    );

    /// Registers an editor command whose arguments decode to `A` (`()` for none).
    fn editor<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    );
}

impl Register for CommandRegistry {
    fn workspace<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut Workspace, A, &mut Window, &mut Context<Workspace>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }

    fn editor<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }
}

/// A registry with every handler of the application.
pub(crate) fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::default();
    crate::workspace::register_commands(&mut registry);
    crate::editor::register_commands(&mut registry);
    registry
}

/// Installs the registry and the keymap (defaults plus the user's `keymap.toml`, if any).
pub(crate) fn init(user_keymap: Option<&str>, cx: &mut App) -> Keymap {
    cx.set_global(registry());
    crate::editor_hooks::install(cx);
    let keymap = birchpad_ui::bind_keymap(user_keymap, cx);
    birchpad_menu_bar::init(cx);
    keymap
}

/// Catalog commands that have neither a handler nor a "pending" mark.
#[cfg(test)]
fn unregistered(registry: &CommandRegistry) -> Vec<&'static str> {
    birchpad_commands::COMMANDS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !registry.contains(id) && !registry.is_pending(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use gpui_kit::{KeybindingKeystroke, Keystroke, TestAppContext};

    use super::*;
    use crate::workspace::tests::open_workspace;

    /// A layout where Shift+2 types `"`, as on a Russian keyboard.
    struct QuoteOnShift2;

    impl PlatformKeyboardMapper for QuoteOnShift2 {
        fn map_key_equivalent(&self, mut keystroke: Keystroke, _: bool) -> KeybindingKeystroke {
            if keystroke.modifiers.shift && keystroke.key == "2" {
                keystroke.modifiers.shift = false;
                keystroke.key = "\"".into();
            }
            KeybindingKeystroke::from_keystroke(keystroke)
        }

        fn get_key_equivalents(&self) -> Option<&rustc_hash::FxHashMap<char, char>> {
            None
        }
    }

    #[test]
    fn every_catalog_command_has_a_handler() {
        let registry = registry();
        assert_eq!(unregistered(&registry), Vec::<&str>::new());
    }

    #[test]
    fn default_keymap_converts_to_gpui_bindings() {
        for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
            let keymap = Keymap::with_defaults(platform);
            let mapper = gpui_kit::DummyKeyboardMapper;
            assert_eq!(
                key_bindings(&keymap, &mapper, false).len(),
                keymap.bindings().len()
            );
        }
        // On Linux, Alt+Shift+0 (Unfold All) and Ctrl+Alt+Shift+N (Clear Style) also get the
        // spelling X11 and Wayland report.
        let keymap = Keymap::with_defaults(Platform::Linux);
        let shifted = keymap
            .bindings()
            .iter()
            .filter(|binding| binding.shifted_symbol_keys().is_some())
            .count();
        assert!(shifted >= 15, "{shifted}");
        assert_eq!(
            key_bindings(&keymap, &gpui_kit::DummyKeyboardMapper, true).len(),
            keymap.bindings().len() + shifted
        );
    }

    #[gpui_kit::test]
    fn bindings_follow_the_keyboard_layout(cx: &mut TestAppContext) {
        let (_, cx) = open_workspace(cx);
        let clear_style_2 = RunCommand(Invocation::with_args(
            "mark.clear",
            serde_json::json!({ "style": 2 }),
        ));
        let keys = |cx: &mut gpui_kit::VisualTestContext| {
            cx.update(|_, cx| {
                let keymap = cx.key_bindings();
                let keymap = keymap.borrow();
                let ours = keymap
                    .bindings()
                    .filter(|binding| binding.action().as_any().is::<RunCommand>())
                    .count();
                let keys: Vec<String> = keymap
                    .bindings_for_action(&clear_style_2)
                    .map(|binding| binding.keystrokes()[0].key().to_owned())
                    .collect();
                (keymap.bindings().len(), ours, keys)
            })
        };
        // Linux also reports Shift+2 as the symbol it types on a US layout (ADR 0014).
        let linux = Platform::current() == Platform::Linux;
        let symbol: &[&str] = if linux { &["@"] } else { &[] };
        let (all, ours, before) = keys(cx);
        assert_eq!(before, [symbol, &["2"]].concat());

        cx.update(|_, cx| {
            let keymap = Keymap::with_defaults(Platform::current());
            replace_command_bindings(key_bindings(&keymap, &QuoteOnShift2, linux), cx);
        });
        let (all_after, ours_after, after) = keys(cx);
        assert_eq!(
            after,
            [symbol, &["\""]].concat(),
            "the key Shift+2 types on the new layout"
        );
        assert_eq!(
            (all_after, ours_after),
            (all, ours),
            "the other bindings are kept"
        );

        // The Linux bindings, checked wherever the tests run.
        let linux_keys: Vec<String> = key_bindings(
            &Keymap::with_defaults(Platform::Linux),
            &QuoteOnShift2,
            true,
        )
        .iter()
        .filter(|binding| binding.action().partial_eq(&clear_style_2))
        .map(|binding| binding.keystrokes()[0].key().to_owned())
        .collect();
        assert_eq!(linux_keys, ["@", "\""]);
    }
}
