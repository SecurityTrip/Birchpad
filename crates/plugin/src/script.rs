//! User scripts in Rhai (ADR 0037): `.rhai` files in the `scripts` folder, run from the
//! Plugins menu over the active document.
//!
//! A script sees the same API as a plugin, as functions:
//!
//! | Function | Does |
//! |---|---|
//! | `text()`, `set_text(s)` | the whole text |
//! | `length()` | its length in bytes |
//! | `text_range(start, end)`, `replace(start, end, s)` | a part of it, by byte offsets |
//! | `selected_text()`, `insert(s)` | the primary selection; typing over every selection |
//! | `selections()`, `set_selection(anchor, head)` | selections as `#{anchor, head}` maps |
//! | `path()`, `language()` | the document's file (or `()`) and language |
//! | `run_command(id)`, `run_command(id, #{...})` | a command, once the script ends |
//! | `message(s)`, `print(s)` | a message to the user |
//! | `show_panel(title, [lines])` | the scripts' panel |
//!
//! Scripts cannot touch files or the network, and stop after [`MAX_OPERATIONS`] operations.

use birchpad_i18n::t;
use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, Position};

use crate::session::{Outcome, Session};

/// The most operations a script may run, so that one that loops ends.
pub const MAX_OPERATIONS: u64 = 50_000_000;

type Shared = Rc<RefCell<Session>>;

fn refused(message: String) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(message.into(), Position::NONE))
}

fn offset(value: i64) -> Result<usize, Box<EvalAltResult>> {
    usize::try_from(value).map_err(|_| refused(format!("{value} is not a position")))
}

fn engine(session: &Shared) -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(64)
        .set_max_expr_depths(128, 64)
        .set_max_string_size(256 << 20)
        .set_max_array_size(1 << 20)
        .set_max_map_size(1 << 16);

    let s = session.clone();
    engine.on_print(move |text| s.borrow_mut().message(text));
    let s = session.clone();
    engine.on_debug(move |text, _, _| s.borrow_mut().message(text));

    let s = session.clone();
    engine.register_fn("text", move || s.borrow().text());
    let s = session.clone();
    engine.register_fn("length", move || s.borrow().len() as i64);
    let s = session.clone();
    engine.register_fn("set_text", move |text: &str| {
        s.borrow_mut().set_text(text).map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("text_range", move |start: i64, end: i64| {
        s.borrow()
            .text_range(offset(start)?, offset(end)?)
            .map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("replace", move |start: i64, end: i64, text: &str| {
        s.borrow_mut()
            .replace(offset(start)?, offset(end)?, text)
            .map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("selected_text", move || s.borrow().selected_text());
    let s = session.clone();
    engine.register_fn("insert", move |text: &str| {
        s.borrow_mut().insert(text).map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("selections", move || -> Array {
        s.borrow()
            .selections()
            .into_iter()
            .map(|(anchor, head)| {
                let mut map = Map::new();
                map.insert("anchor".into(), Dynamic::from(anchor as i64));
                map.insert("head".into(), Dynamic::from(head as i64));
                Dynamic::from_map(map)
            })
            .collect()
    });
    let s = session.clone();
    engine.register_fn("set_selection", move |anchor: i64, head: i64| {
        s.borrow_mut()
            .set_selections(&[(offset(anchor)?, offset(head)?)])
            .map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("path", move || -> Dynamic {
        s.borrow().path().map_or(Dynamic::UNIT, Dynamic::from)
    });
    let s = session.clone();
    engine.register_fn("language", move || s.borrow().language());
    let s = session.clone();
    engine.register_fn("run_command", move |id: &str| {
        s.borrow_mut()
            .run_command(id, serde_json::Value::Null)
            .map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("run_command", move |id: &str, args: Map| {
        let args = serde_json::to_value(Dynamic::from_map(args))
            .map_err(|error| refused(format!("the arguments of {id}: {error}")))?;
        s.borrow_mut().run_command(id, args).map_err(refused)
    });
    let s = session.clone();
    engine.register_fn("message", move |text: &str| s.borrow_mut().message(text));
    let s = session.clone();
    engine.register_fn("show_panel", move |title: &str, lines: Array| {
        let lines = lines.into_iter().map(|line| line.to_string()).collect();
        s.borrow_mut().show_panel(title, lines);
    });
    engine
}

/// Runs the script `source` (named `name` in messages) over `session`.
pub fn run(name: &str, source: &str, session: Session) -> Result<Outcome> {
    run_limited(name, source, session, MAX_OPERATIONS)
}

/// [`run`], stopping the script after `operations` (at most [`MAX_OPERATIONS`]): for fuzzing,
/// where a script that loops must end in a fraction of the time.
pub fn run_limited(name: &str, source: &str, session: Session, operations: u64) -> Result<Outcome> {
    let shared: Shared = Rc::new(RefCell::new(session));
    let mut engine = engine(&shared);
    engine.set_max_operations(operations.min(MAX_OPERATIONS));
    let result = engine.run(source);
    drop(engine);
    let session = Rc::try_unwrap(shared)
        .map_err(|_| anyhow!("{}", t!("plugin.script-running", name = name)))?
        .into_inner();
    match result {
        Ok(()) => Ok(session.finish()),
        Err(error) => {
            let error = match *error {
                EvalAltResult::ErrorTooManyOperations(_) => {
                    t!("plugin.script-too-long")
                }
                error => error.to_string(),
            };
            Err(anyhow!(
                "{}",
                t!("plugin.script-failed", name = name, reason = error)
            ))
        }
    }
}

/// Checks that `source` is a script, without running it.
pub fn check(source: &str) -> Result<()> {
    Engine::new()
        .compile(source)
        .map(|_| ())
        .map_err(|error| anyhow!("{error}"))
}

#[cfg(test)]
#[path = "../tests/unit/script/tests.rs"]
mod tests;
