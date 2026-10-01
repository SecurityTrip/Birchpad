# ADR 0001: Our own editing engine in Rust

- Status: accepted
- Date: 2026-10-01

## Context

Notepad++ is a Win32 shell around Scintilla (editing) and Lexilla (lexers). Two ways to build a
Notepad++-class editor in Rust:

- **A.** Win32 shell in Rust, Scintilla and Lexilla through FFI. Fastest path to parity and
  identical behavior, but Windows-only, the core stays in C++, and new features are bounded by
  what Scintilla can do.
- **B.** Our own text model and editor view in Rust on a GPU-rendered UI framework. Much more
  work up front, but cross-platform and without a ceiling for later features (LSP, inline
  widgets, minimap, collaboration).

## Decision

Option B. Birchpad targets Windows, Linux and macOS from the start and is meant to grow beyond
Notepad++.

## Consequences

- The editor view is the largest single piece of work; Scintilla's documentation serves as the
  behavioral specification for Notepad++ parity (column mode, multi-editing, markers, ...).
- Existing Notepad++ DLL plugins cannot be supported. File formats (themes, UDL, API files,
  shortcuts) can be, and should be imported.
- The text model lives in `birchpad-core` with no UI dependencies, so it can be tested in
  isolation and reused by other front ends.
