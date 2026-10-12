//! Birchpad's plugins and scripts, without a window.
//!
//! - [`wasm`]: plugins as WebAssembly components of the `birchpad:plugin` world
//!   (`wit/birchpad.wit`), run in Wasmtime with no access to anything but the document
//!   (ADR 0036);
//! - [`script`]: user scripts in Rhai, with the same API (ADR 0037);
//! - [`manager`] and [`manifest`]: the plugins folder, `plugin.toml`, installing and removing,
//!   and the administrator's allowlist (ADR 0038);
//! - [`session`]: what one call sees and does, shared by both.

pub mod manager;
pub mod manifest;
pub mod script;
pub mod session;
pub mod wasm;

pub use manifest::{API_VERSION, EventKind, Manifest};
pub use session::{DocumentState, Effect, Outcome, Session};
pub use wasm::{Plugin, PluginCommand, Runtime};
