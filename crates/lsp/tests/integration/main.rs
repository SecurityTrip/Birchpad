//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "capabilities_tests.rs"]
mod capabilities;

#[path = "client_tests.rs"]
mod client;

#[path = "command_line_tests.rs"]
mod command_line;

#[path = "convert_tests.rs"]
mod convert;

#[path = "framing_tests.rs"]
mod framing;

#[path = "languages_tests.rs"]
mod languages;

#[path = "message_tests.rs"]
mod message;

#[path = "position_tests.rs"]
mod position;

#[path = "uri_tests.rs"]
mod uri;
