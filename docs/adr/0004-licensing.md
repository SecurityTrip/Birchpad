# ADR 0004: Licensing

- Status: accepted
- Date: 2026-10-01

## Decision

Birchpad is dual-licensed **MIT OR Apache-2.0**, the norm in the Rust ecosystem.

## Rationale

- Enterprises approve permissively licensed software easily, which matters for per-machine
  deployment.
- Plugin authors, including commercial ones, face no "derivative work" questions.
- Apache-2.0 adds an explicit patent grant.
- It is compatible with GPUI and gpui-component (Apache-2.0) and with most crates.

## Consequences

- Code from GPL projects (Notepad++, Zed's editor crates) and MPL projects (Helix) may be read
  for ideas but never copied.
- `cargo-deny` in CI rejects dependencies with incompatible licenses.
- "Notepad++" is not used in the product name or branding.
