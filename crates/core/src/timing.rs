//! The timing checks' limits (`cargo test --release -- --ignored`, the nightly performance job):
//! each check prints what it measured and fails when it took longer than its limit, the time it
//! may take in a release build on CI's Linux runners. The limits are generous, a few times what a
//! run takes, so that only a real slowdown fails.

use std::time::Duration;

/// Prints how long `what` took and fails if that is more than `limit`.
#[track_caller]
pub fn within(what: &str, took: Duration, limit: Duration) {
    eprintln!("{what}: {took:?} (limit {limit:?})");
    assert!(
        took <= limit,
        "{what} took {took:?}, more than its limit of {limit:?}"
    );
}

#[cfg(test)]
#[path = "../tests/unit/timing/tests.rs"]
mod tests;
