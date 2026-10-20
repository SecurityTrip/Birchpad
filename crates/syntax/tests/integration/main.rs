//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "brackets/tests.rs"]
mod brackets;

#[path = "folds/tests.rs"]
mod folds;

#[path = "highlight/tests.rs"]
mod highlight;

#[path = "language/tests.rs"]
mod language;

#[path = "outline/tests.rs"]
mod outline;

#[path = "syntax/reparse_properties.rs"]
mod syntax_reparse_properties;

#[path = "syntax/tests.rs"]
mod syntax;

#[path = "udl/definition/tests.rs"]
mod udl_definition;

#[path = "udl/tests.rs"]
mod udl;
