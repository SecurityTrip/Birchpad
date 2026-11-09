//! Birchpad's language server client without a window (ADR 0065): the base protocol and
//! JSON-RPC ([`framing`], [`message`]), a [`Client`] that talks to a server process on threads of
//! its own, what Birchpad offers and reads of a server ([`capabilities`]), byte offsets and the
//! protocol's positions ([`position`]), files as URIs ([`uri`]), the servers' command lines
//! ([`command_line`]), language ids and root folders ([`languages`]), and the servers' answers
//! as plain values ([`convert`]).
//!
//! The application decides when to ask; nothing here waits on the window.

#[macro_use]
extern crate birchpad_i18n;

pub mod capabilities;
pub mod client;
pub mod command_line;
pub mod convert;
#[cfg(feature = "test-support")]
pub mod fake;
pub mod framing;
pub mod languages;
pub mod message;
pub mod position;
pub mod uri;

pub use capabilities::Abilities;
pub use client::{Client, RequestError, ServerEvent, Transport};
pub use command_line::CommandLine;
pub use lsp_types;
pub use position::PositionEncoding;
