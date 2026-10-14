//! The keymap in effect, bound to GPUI: the default keymap with the user's `keymap.toml` over
//! it, every binding dispatching [`RunCommand`].

use std::rc::Rc;

use birchpad_commands::{Keymap, Layer, Platform};
use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate, PlatformKeyboardMapper};

use crate::RunCommand;

/// Builds the keymap (the defaults plus the user's `keymap.toml`, if any) and binds its keys.
pub fn bind_keymap(user_keymap: Option<&str>, cx: &mut App) -> Keymap {
    let keymap = build_keymap(user_keymap);
    let mapper = cx.keyboard_mapper().clone();
    cx.bind_keys(key_bindings(&keymap, mapper.as_ref(), shifted_symbols()));
    cx.set_global(ActiveKeymap(keymap.clone()));
    // Shift with a digit types a different character on each layout (Shift+2 is `@` on a US
    // layout and `"` on a Russian one), so the bindings are rebuilt for the new layout.
    cx.on_keyboard_layout_change(rebind).detach();
    keymap
}

/// The keymap in effect: the defaults with the user's `keymap.toml` over them.
pub struct ActiveKeymap(pub Keymap);

impl gpui_kit::Global for ActiveKeymap {}

fn shifted_symbols() -> bool {
    Platform::current() == Platform::Linux
}

fn build_keymap(user_keymap: Option<&str>) -> Keymap {
    let mut keymap = Keymap::with_defaults(Platform::current());
    if let Some(source) = user_keymap {
        keymap.add_layer(Layer::User, source);
    }
    for diagnostic in keymap.diagnostics() {
        eprintln!("keymap: {diagnostic}");
    }
    keymap
}

/// Binds the keys of the active keymap again.
fn rebind(cx: &mut App) {
    let mapper = cx.keyboard_mapper().clone();
    let bindings = key_bindings(
        &cx.global::<ActiveKeymap>().0,
        mapper.as_ref(),
        shifted_symbols(),
    );
    replace_command_bindings(bindings, cx);
}

/// Takes the user's keymap anew, after the Shortcut Mapper changed it.
pub fn reload_keymap(user_keymap: Option<&str>, cx: &mut App) {
    cx.set_global(ActiveKeymap(build_keymap(user_keymap)));
    rebind(cx);
}

/// Replaces the bindings of [`RunCommand`] with `bindings`. Every other binding (text inputs
/// and dialogs of the component library) stays, in the same order.
pub fn replace_command_bindings(bindings: Vec<KeyBinding>, cx: &mut App) {
    let current: Vec<KeyBinding> = cx.key_bindings().borrow().bindings().cloned().collect();
    let mut bindings = Some(bindings);
    let mut all = Vec::with_capacity(current.len());
    for binding in current {
        if binding.action().as_any().is::<RunCommand>() {
            // Ours are bound together: the new ones go where the first one was.
            all.extend(bindings.take().into_iter().flatten());
        } else {
            all.push(binding);
        }
    }
    all.extend(bindings.into_iter().flatten());
    cx.clear_key_bindings();
    cx.bind_keys(all);
}

/// Turns the effective bindings of a keymap into GPUI key bindings.
///
/// Keys go through the platform's keyboard mapper: Windows reports Shift with a digit or
/// punctuation key as the character it types on the current layout (Alt+Shift+0 arrives as
/// `alt-)` on a US or Russian layout), so `alt-shift-0` in a keymap has to become that too.
/// Linux reports such keys the same way but GPUI has no mapper there: with `shifted_symbols`,
/// those bindings get a second binding spelled the way Linux reports them (on a US layout).
/// It comes first, so menus keep showing the binding as written.
pub fn key_bindings(
    keymap: &Keymap,
    mapper: &dyn PlatformKeyboardMapper,
    shifted_symbols: bool,
) -> Vec<KeyBinding> {
    keymap
        .bindings()
        .iter()
        .flat_map(|binding| {
            let shifted = shifted_symbols
                .then(|| binding.shifted_symbol_keys())
                .flatten();
            shifted
                .into_iter()
                .chain([binding.keys_string()])
                .map(move |keys| (binding, keys))
        })
        .filter_map(|(binding, keys)| {
            let context = match binding
                .context
                .as_deref()
                .map(KeyBindingContextPredicate::parse)
            {
                None => None,
                Some(Ok(predicate)) => Some(Rc::new(predicate)),
                Some(Err(error)) => {
                    eprintln!("keymap: bad context {:?}: {error}", binding.context);
                    return None;
                }
            };
            KeyBinding::load(
                &keys,
                Box::new(RunCommand(binding.invocation.clone())),
                context,
                false,
                None,
                mapper,
            )
            .inspect_err(|error| eprintln!("keymap: {keys}: {error}"))
            .ok()
        })
        .collect()
}
