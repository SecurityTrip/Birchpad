//! Running plugins: WebAssembly components of the `plugin` world in `wit/birchpad.wit`, in
//! Wasmtime (ADR 0036).
//!
//! A plugin sees nothing but the host interface: no files, no network, no clock. Each call
//! runs on a budget of fuel (about one unit an instruction) and in at most
//! [`MEMORY_LIMIT`] bytes of memory, so a plugin that loops or grows without end stops with an
//! error instead of hanging Birchpad. A plugin that traps is instantiated afresh on its next
//! call.

use anyhow::{Context as _, Result, anyhow};
use birchpad_i18n::t;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

use crate::manifest::{EventKind, is_valid_id};
use crate::session::{Outcome, Session};

mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "plugin",
    });
}

use bindings::Plugin as Bindings;
use bindings::birchpad::plugin::host;
use bindings::birchpad::plugin::types;

/// The fuel one call may burn: a few seconds of work at most.
pub const DEFAULT_FUEL: u64 = 5_000_000_000;
/// The most memory a plugin may have.
pub const MEMORY_LIMIT: usize = 256 << 20;
/// The most commands a plugin may add.
pub const MAX_COMMANDS: usize = 200;

/// A command of a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
}

/// What a plugin's store holds: the session of the call under way.
struct State {
    session: Session,
    limits: StoreLimits,
}

/// The engine every plugin runs in.
#[derive(Clone)]
pub struct Runtime {
    engine: Engine,
    fuel: u64,
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Self::with_fuel(DEFAULT_FUEL)
    }

    /// A runtime whose calls may burn at most `fuel`.
    pub fn with_fuel(fuel: u64) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.wasm_component_model(true);
        #[cfg(all(windows, target_arch = "aarch64"))]
        config
            .target("pulley64")
            .map_err(|error| anyhow!("{error}"))?;
        let engine = Engine::new(&config).map_err(|error| anyhow!("{error}"))?;
        Ok(Self { engine, fuel })
    }

    /// Compiles a component. This takes a while: do it off the main thread.
    pub fn compile(&self, bytes: &[u8]) -> Result<Component> {
        Component::new(&self.engine, bytes)
            .map_err(|error| anyhow!("{error}"))
            .with_context(|| t!("plugin.not-a-component"))
    }

    /// Instantiates a compiled component and asks it for its commands.
    pub fn load(&self, component: Component) -> Result<Plugin> {
        let mut plugin = Plugin {
            runtime: self.clone(),
            component,
            live: None,
            commands: Vec::new(),
        };
        let (commands, _) = plugin.call(Session::empty(), |bindings, store| {
            bindings.call_commands(store)
        })?;
        plugin.commands = check_commands(commands)?;
        Ok(plugin)
    }
}

fn check_commands(commands: Vec<types::Command>) -> Result<Vec<PluginCommand>> {
    if commands.len() > MAX_COMMANDS {
        anyhow::bail!(
            "{}",
            t!(
                "plugin.too-many-commands",
                count = commands.len(),
                limit = MAX_COMMANDS
            )
        );
    }
    let mut checked: Vec<PluginCommand> = Vec::with_capacity(commands.len());
    for command in commands {
        if !is_valid_id(&command.id) {
            anyhow::bail!(
                "{}",
                t!("plugin.not-a-command-id", id = format!("{:?}", command.id))
            );
        }
        if checked.iter().any(|known| known.id == command.id) {
            anyhow::bail!("{}", t!("plugin.command-twice", id = command.id));
        }
        let title = command.title.trim();
        if title.is_empty() || title.contains(char::is_control) {
            anyhow::bail!("{}", t!("plugin.title-of-one-line", id = command.id));
        }
        checked.push(PluginCommand {
            id: command.id,
            title: title.to_owned(),
        });
    }
    Ok(checked)
}

/// A loaded plugin.
pub struct Plugin {
    runtime: Runtime,
    component: Component,
    /// The instance, until a call traps.
    live: Option<(Store<State>, Bindings)>,
    commands: Vec<PluginCommand>,
}

impl Plugin {
    /// The commands the plugin added.
    pub fn commands(&self) -> &[PluginCommand] {
        &self.commands
    }

    fn instantiate(&self) -> Result<(Store<State>, Bindings)> {
        let mut linker = Linker::<State>::new(&self.runtime.engine);
        host::add_to_linker::<State, HasSelf<State>>(&mut linker, |state| state)
            .map_err(|error| anyhow!("{error}"))?;
        let state = State {
            session: Session::empty(),
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT)
                .instances(16)
                .tables(16)
                .memories(4)
                .build(),
        };
        let mut store = Store::new(&self.runtime.engine, state);
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.runtime.fuel)
            .map_err(|error| anyhow!("{error}"))?;
        let bindings = Bindings::instantiate(&mut store, &self.component, &linker)
            .map_err(|error| anyhow!("{error:#}"))
            .with_context(|| t!("plugin.does-not-start"))?;
        Ok((store, bindings))
    }

    /// Calls into the plugin with `session`, and gives back what the call returned with what
    /// it did.
    fn call<T>(
        &mut self,
        session: Session,
        f: impl FnOnce(&Bindings, &mut Store<State>) -> wasmtime::Result<T>,
    ) -> Result<(T, Outcome)> {
        let (mut store, bindings) = match self.live.take() {
            Some(live) => live,
            None => self.instantiate()?,
        };
        store.data_mut().session = session;
        store
            .set_fuel(self.runtime.fuel)
            .map_err(|error| anyhow!("{error}"))?;
        let result = f(&bindings, &mut store);
        let session = std::mem::take(&mut store.data_mut().session);
        match result {
            Ok(value) => {
                self.live = Some((store, bindings));
                Ok((value, session.finish()))
            }
            // The instance may be left half way through something: start afresh next time.
            Err(error) => Err(describe_trap(&error)),
        }
    }

    /// Runs the plugin's command `id`.
    pub fn run(&mut self, id: &str, session: Session) -> Result<Outcome> {
        if !self.commands.iter().any(|command| command.id == id) {
            anyhow::bail!("{}", t!("plugin.no-plugin-command", id = id));
        }
        let (result, outcome) =
            self.call(session, |bindings, store| bindings.call_run(store, id))?;
        result.map_err(|message| anyhow!(message))?;
        Ok(outcome)
    }

    /// Tells the plugin about an event.
    pub fn event(&mut self, event: EventKind, session: Session) -> Result<Outcome> {
        let event = match event {
            EventKind::Opened => types::Event::Opened,
            EventKind::Saved => types::Event::Saved,
            EventKind::Closed => types::Event::Closed,
            EventKind::Activated => types::Event::Activated,
        };
        let ((), outcome) = self.call(session, |bindings, store| {
            bindings.call_on_event(store, event)
        })?;
        Ok(outcome)
    }

    /// Tells the plugin that line `line` of its panel was clicked.
    pub fn panel_click(&mut self, line: u32, session: Session) -> Result<Outcome> {
        let ((), outcome) = self.call(session, |bindings, store| {
            bindings.call_on_panel_click(store, line)
        })?;
        Ok(outcome)
    }
}

fn describe_trap(error: &wasmtime::Error) -> anyhow::Error {
    if let Some(trap) = error.downcast_ref::<wasmtime::Trap>() {
        return match trap {
            wasmtime::Trap::OutOfFuel => anyhow!("{}", t!("plugin.too-long")),
            trap => anyhow!("{}", t!("plugin.failed", reason = trap)),
        };
    }
    anyhow!("{}", t!("plugin.failed", reason = format!("{error:#}")))
}

fn offset(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

impl types::Host for State {}

impl host::Host for State {
    fn text(&mut self) -> String {
        self.session.text()
    }

    fn length(&mut self) -> u64 {
        self.session.len() as u64
    }

    fn text_range(&mut self, start: u64, end: u64) -> Result<String, String> {
        self.session.text_range(offset(start), offset(end))
    }

    fn replace(&mut self, start: u64, end: u64, text: String) -> Result<(), String> {
        self.session.replace(offset(start), offset(end), &text)
    }

    fn insert(&mut self, text: String) {
        let _ = self.session.insert(&text);
    }

    fn selections(&mut self) -> Vec<types::Selection> {
        self.session
            .selections()
            .into_iter()
            .map(|(anchor, head)| types::Selection {
                anchor: anchor as u64,
                head: head as u64,
            })
            .collect()
    }

    fn set_selections(&mut self, selections: Vec<types::Selection>) -> Result<(), String> {
        let ranges: Vec<(usize, usize)> = selections
            .into_iter()
            .map(|range| (offset(range.anchor), offset(range.head)))
            .collect();
        self.session.set_selections(&ranges)
    }

    fn path(&mut self) -> Option<String> {
        self.session.path()
    }

    fn language(&mut self) -> String {
        self.session.language()
    }

    fn run_command(&mut self, id: String, arguments: String) -> Result<(), String> {
        self.session.run_command_json(&id, &arguments)
    }

    fn message(&mut self, text: String) {
        self.session.message(&text);
    }

    fn show_panel(&mut self, title: String, lines: Vec<String>) {
        self.session.show_panel(&title, lines);
    }
}

#[cfg(test)]
#[path = "../tests/unit/wasm/tests.rs"]
mod tests;
