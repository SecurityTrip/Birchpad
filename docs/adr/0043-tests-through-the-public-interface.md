# ADR 0043: Tests through the public interface

- Status: accepted
- Date: 2026-10-20

## Context

Roadmap 6.35 moved every test out of `src` into its crate's `tests` folder, but as unit tests:
`src` included them with `#[path]` and `#[cfg(test)]`, so they still reached the crates'
private items, and the application, a program without a library, could be tested no other
way. A test that reads private fields breaks when the code is reorganized without changing what
it does, and says nothing about what the crate offers its users. Unit tests also build a copy of
each crate of their own: the workspace's tests built every crate twice.

## Decision

- **One test binary per crate.** The tests that were unit tests are integration tests, in
  `tests/integration`: a `main.rs` that names one module per part of `src`
  (`tests/integration/pane/tests.rs` for `src/pane.rs`), built as one binary, so that a crate
  with a window links its tests once. Other integration tests (properties, timings, the
  end-to-end tests) stay files of their own directly in `tests`. Nothing in `src` is a test or
  includes one.
- **The application is a library and a program.** `birchpad` (`crates/app`) is a library,
  `src/lib.rs`, whose `run` starts Birchpad, and a program, `src/main.rs`, that calls it. The
  release tool is split in the same way. Their modules are public, so that the tests drive the
  window's parts as the application does.
- **What the tests use is public.** A crate's tests go through what it exports. Where a test
  checked a private helper, it goes through the public function that uses it (a Notepad++
  color through a theme read from XML, a code page through `decode` and `encode`), or the
  helper becomes public when it is a piece others can use on its own (`read_file_within`,
  `lua_pattern_to_regex`, `MenuBar::state`, `xdg_mime_default_args`). Platform calls that a
  test must not make on the real system take their place as a parameter: the registry key of
  the Explorer's verb or of the file associations (`add_at`, `register_at`), the code of
  long lines in `DisplayMap` (`set_long_lines`) instead of constants that tests changed.
- **`test-support` instead of `cfg(test)`.** What only tests need (reading a dialog's state,
  fakes of the system's clipboard, shell and trash) is behind a `test-support` feature, which
  each crate's tests turn on through a dev-dependency on the crate itself. Release builds never
  have it; the end-to-end tests run the program built with it, as they ran it with the
  editor's and `birchpad-ui`'s before.

## Consequences

- The window's crates (`birchpad`, `birchpad-editor`) export much: their views, fields and
  dialogs are public, as ADR 0041 already said of the editor. Code outside the workspace does
  not depend on them, so this costs no compatibility.
- A test may no longer reach a private item: a new test either uses what the crate offers, or
  the change makes public what the test needs, saying why.
- Tests that switch the interface's language take turns in `birchpad-i18n`, where the language
  is the process's unless the `per-thread` feature is on.
