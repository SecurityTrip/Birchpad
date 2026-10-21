//! Birchpad's plugins and scripts, without a window.
//!
//! - [`wasm`]: plugins as WebAssembly components of the `birchpad:plugin` world
//!   (`wit/birchpad.wit`), run in Wasmtime with no access to anything but the document and
//!   what the user allowed (ADR 0036, ADR 0046);
//! - [`script`]: user scripts in Rhai, with the same API (ADR 0037);
//! - [`manager`] and [`manifest`]: the plugins folder, `plugin.toml`, installing and removing,
//!   the plugins' settings and permissions, and the administrator's allowlist (ADR 0038);
//! - [`session`]: what one call sees and does, shared by both.

pub mod manager;
pub mod manifest;
pub mod script;
pub mod session;
pub mod wasm;

pub use manifest::{API_VERSION, EventKind, Manifest, Permission, SettingKind, SettingSpec};
pub use session::{Control, DocumentState, Effect, Grants, Outcome, Session};
pub use wasm::{Plugin, PluginCommand, Runtime, TaskRunner};
