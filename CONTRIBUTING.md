# Contributing to Birchpad

Thanks for your interest in Birchpad!

## Before you start

- For anything bigger than a small fix, open an issue or a discussion first so we can agree on
  the approach.
- Architecture decisions are recorded in [`docs/adr`](docs/adr). If your change contradicts one,
  propose a new ADR instead of silently diverging.

## Development

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

CI runs the same commands with warnings treated as errors, on Windows (x64 and ARM64), Linux and
macOS, plus [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for licenses and advisories.

### Ground rules for the code

- `birchpad-core` must never depend on UI crates. Everything in it is testable without a window.
- Positions in the text model are **byte offsets** into the rope. Convert to chars, UTF-16 or
  visual columns only at the edges (rendering, LSP, status bar).
- Test fixtures under `tests/fixtures` are committed byte-for-byte (see `.gitattributes`);
  never let an editor "fix" their encoding or line endings.
- No `unsafe` without a comment explaining why it is sound, and a reviewer who agrees.

### Tests

Birchpad has no testers: its tests are its quality assurance. For every function or feature a
change touches, the tests cover:

- **the positive scenario**: it does what it should with ordinary input;
- **the negative scenario**: invalid input, a missing or wrong argument, a refused action (a
  read-only document, a file that cannot be read or written), an error path. It fails cleanly,
  without a panic and without changing what it must not;
- **boundary values**: empty text, the first and the last line or character, the end of the
  text without a final line break, zero, one and the maximum, each limit and one past it, CRLF,
  wide and combining characters, very long lines.

A bug fix comes with a test that fails without the fix. `cargo llvm-cov --workspace` shows the
code no test runs yet.

Everything that reads input from outside (files and their encodings, settings, the state file,
sessions and Notepad++'s `session.xml`, `.workspace` files, the keymap, the command line and the
message that hands it to a running instance, the update manifest) is fuzzed. The checks live in
`crates/fuzz`: each takes any bytes and checks more than "no panic" (what reads cleanly writes
back the same). `cargo test -p birchpad-fuzz` runs them over the seeds in `fuzz/seeds`, every
cut of each seed and a few hundred mutants of it (`BIRCHPAD_FUZZ_MUTANTS=20000` for more); CI
also runs them under libFuzzer for a minute each. To fuzz longer, with a nightly toolchain and
`cargo install cargo-fuzz`:

```bash
cargo +nightly fuzz run session fuzz/corpus/session fuzz/seeds/session
```

An input that crashes a check becomes a seed once the bug is fixed; seeds named `reject-*` must
be refused, the others accepted.

End-to-end tests start the built application in a real window, as a portable copy in a folder
of its own, and drive it with a script of keystrokes, typed text and commands (the `e2e`
feature, `crates/app/src/e2e.rs`); the test then checks the application's reports and the files
on disk, and starts it again to see the session come back. Windows briefly appear while they
run; on Linux, run them under `xvfb-run`:

```bash
cargo test -p birchpad --features e2e --test e2e
```

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`, `ci:`, `chore:`). The changelog is generated from them.
- Keep pull requests focused; one logical change per PR.

## Licensing and third-party code

Birchpad is dual-licensed under MIT OR Apache-2.0. By contributing you agree that your
contribution is licensed the same way.

Do **not** copy code from GPL-licensed projects (including Notepad++ and Zed's editor crates) or
from MPL-licensed projects (such as Helix). Reading them for ideas is fine; copying is not.
Compatibility with Notepad++ *file formats* (themes, user-defined languages, API files) is welcome.
