//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "bidi/properties.rs"]
mod bidi_properties;

#[path = "bidi/tests.rs"]
mod bidi;

#[path = "block/tests.rs"]
mod block;

#[path = "cells/tests.rs"]
mod cells;

#[path = "display_map/tests.rs"]
mod display_map;

#[path = "folding/tests.rs"]
mod folding;

#[path = "guides/tests.rs"]
mod guides;

#[path = "wrap/tests.rs"]
mod wrap;
