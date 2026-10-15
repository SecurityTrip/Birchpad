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
cargo nextest run --workspace
```

[`cargo-nextest`](https://nexte.st) (`cargo install cargo-nextest --locked`) runs each test in a
process of its own, as many at once as there are cores; `cargo test --workspace` runs the same
tests more slowly, one test binary after another. The crates have no doctests, which nextest
would not run; examples are tests.
Its settings are in `.config/nextest.toml`. Debug builds optimize the dependencies
(`[profile.dev.package."*"]` in `Cargo.toml`): the window's tests spend most of their time in
GPUI, and the test window turns motion off (`set_reduce_motion`), so no test waits for a dialog
to slide in.

CI runs the same commands with warnings treated as errors, in four workflows that start together
on every push to `main` and every pull request, each job of them side by side:

- **Linux** (`.github/workflows/linux.yml`): Clippy, the tests and the end-to-end tests on
  Ubuntu, and the tests in Debian trixie and bookworm containers, against Debian's own libraries;
- **Windows** (`windows.yml`): Clippy and the tests on x64 and ARM64, the end-to-end tests on x64;
- **macOS** (`macos.yml`): Clippy, the tests and the end-to-end tests;
- **Checks** (`checks.yml`): formatting,
  [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) for licenses and advisories,
  coverage and fuzzing.

The jobs' names (`test (ubuntu-latest)`, `e2e (windows-latest)`, ...) are the checks the `main`
branch's ruleset requires: a job renamed without the ruleset leaves a pull request waiting for a
check that never comes.

Clippy runs in a job of its own beside the tests, and CI builds keep only line numbers as debug
information (`CARGO_PROFILE_DEV_DEBUG=line-tables-only`), so test binaries link and cache faster.

### Ground rules for the code

- `birchpad-core` must never depend on UI crates. Everything in it is testable without a window.
- Logic that needs no window goes to the crate of its kind (`core`, `io`, `config`, `tools`,
  ...), not to the application. What parts of the window share (the command registry, the
  application's settings, dialog buttons, messages to the user) is in `birchpad-ui`; a command
  handler is registered for the view that runs it (`CommandTarget`), not in the registry. The
  editor (`birchpad-editor`) asks the application for anything else through `EditorHooks`; see
  ADR 0041 for the layers.
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
code no test runs yet; cover that first. CI fails when a crate's share of lines run falls under
its floor in `.github/coverage-floors.toml`, and floors only rise: raise one in the change whose
tests lift the crate well over it.

Everything that reads input from outside (files and their encodings, settings, the state file,
sessions and Notepad++'s `session.xml`, `.workspace` files, the keymap, the text's context menu
(`context-menu.toml`), the command line and the message that hands it to a running instance, the
update manifest, plugins' `plugin.toml`, and the JSON, XML and hex dumps the built-in tools
read) is fuzzed. The checks live in `crates/fuzz`: each takes any bytes and checks more than "no
panic" (what reads cleanly writes back the same). `cargo test -p birchpad-fuzz` runs them over
the seeds in `fuzz/seeds`, every cut of each seed and a few hundred mutants of it
(`BIRCHPAD_FUZZ_MUTANTS=20000` for more); CI also runs them under libFuzzer for a minute each.
To fuzz longer, with a nightly toolchain and `cargo install cargo-fuzz`:

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

What no automated test sees (input methods, screen readers, displays, installers and updates)
is checked by hand before each release: [docs/release-checklist.md](docs/release-checklist.md).

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`, `ci:`, `chore:`). The changelog is generated from them.
- Keep pull requests focused; one logical change per PR.

## Licensing and third-party code

Birchpad is dual-licensed under MIT OR Apache-2.0. By contributing you agree that your
contribution is licensed the same way.

Do **not** copy code from GPL-licensed projects (including Notepad++ and Zed's editor crates) or
from MPL-licensed projects (such as Helix).
Compatibility with Notepad++ *file formats* (themes, user-defined languages, API files) is welcome.

### Clean room

Birchpad reimplements what Notepad++ does without its code. So that nothing of it slips in,
even rewritten:

- **Do not read the source** of Notepad++, Scintilla, Lexilla or of a Notepad++ plugin
  (ComparePlus, JSON Viewer, XML Tools, Hex Editor, DSpellCheck...) while working on Birchpad,
  nor of another GPL or MPL editor for the feature at hand. Someone who has studied a part of it
  leaves that part to someone else.
- **Work from the outside:** the user manual (npp-user-manual.org), the program's menus, dialogs
  and behavior as a user sees them, and the files it reads and writes. Describe what to build in
  your own words first (the roadmap, an ADR), then write the code from that description.
- **File formats are fine to follow:** element and attribute names (`tabColourId`,
  `WidgetStyle`), keys and values of `session.xml`, `stylers.xml`, `langs.xml`, UDL and API
  files, `nativeLang` files and the command line's options, so that Birchpad reads what users
  already have.
- **Data is not ours to bundle:** Notepad++'s themes, API files, UDL collection, translations
  and keyword lists are covered by its license. Birchpad reads them from the user's own files
  and ships only data it wrote itself or that has a license compatible with MIT and Apache-2.0
  (noted in `deny.toml` or next to the file).
- **Names of menu items and settings** may match Notepad++'s, so that its users find their way;
  longer texts (descriptions, help, messages) are written anew. Icons and images are drawn for
  Birchpad or come under a compatible license, with that license kept.
- **Code and data from elsewhere** under a permissive license keeps its license and copyright
  notice, and the change says where it comes from. Code written with an AI assistant is held to
  the same rules: ask for it from the description, never by pointing at the GPL source.

A pull request that touches a Notepad++ feature says what it was built from (manual, behavior,
file samples).
