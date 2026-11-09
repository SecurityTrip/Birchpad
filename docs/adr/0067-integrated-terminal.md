# ADR 0067: Integrated terminal

- Status: accepted
- Date: 2026-11-09
- Builds on [ADR 0058](0058-parts-of-the-window-in-crates-of-their-own.md) (the search
  results panel under the documents, the place this panel shares) and
  [ADR 0041](0041-crate-layout.md).

## Context

Roadmap 7.7 asks for a terminal in a panel: the user's shell beside the documents, as other
editors have it. A terminal is two hard parts. The pseudo-terminal: a ConPTY on Windows, a pty
pair, a session and a controlling terminal on Linux and macOS, each with its own system calls
and `unsafe`. The emulator: VT100 to xterm escape sequences, a grid with scrollback, wide
characters, the alternate screen of full-screen programs, modes that change what keys send.
Writing either is months of work and a lasting source of bugs.

`alacritty_terminal` (Apache-2.0) is both, without a window: Alacritty's grid, parser (`vte`)
and pty on the three platforms, published as a library and used by other editors. Most of what
it needs GPUI already builds (`parking_lot`, `polling`, `regex-automata`, `unicode-width`,
`windows-sys`, `rustix`, `libc`); it adds `vte`, `rustix-openpty`, `signal-hook`, `miow` and
`piper`, all under MIT or Apache-2.0. This is the last-resort dependency CONTRIBUTING asks to
justify: there is no smaller way to a terminal that works.

## Decision

### A crate without a window

- `birchpad-terminal` holds the emulator and the session: an `Emulator` that takes bytes and
  gives a `Screen` (rows of cells with their characters and colors resolved to RGB through a
  `Palette`, the cursor, the title, the modes), and a `Session` that runs a shell on a pty, feeds
  its output to the emulator on Alacritty's thread, writes keys and pastes to it, resizes it and
  says when it exits. Keys become bytes in the crate (`keys::encode`), as xterm sends them,
  application cursor mode included; tests drive the emulator with bytes and the session with a
  real shell.
- Alacritty's own types stay inside the crate; the application sees Birchpad's.

### The panel

- View > Terminal (``Ctrl+` ``) shows and hides a panel under the documents, as the search results
  are shown (beside them when both are shown, the terminal under). One session per window,
  started the first time the panel opens, in the active document's folder (or Folder as
  Workspace's, or the home folder), with the shell of `terminal.shell`: empty for the system's
  (`%COMSPEC%` on Windows, `$SHELL` elsewhere, `/bin/sh` without it), or a command line split as
  `lsp.servers` are.
- The panel draws the screen in the editor's font with the theme's colors: the 16 colors of
  ANSI from a light or a dark palette by the theme's brightness, the 240 others as xterm has them,
  RGB as sent. The cursor is a block while the panel has the focus, an outline otherwise.
- Typed text and keys go to the shell; Shift+Insert and ``Ctrl+Shift+V`` paste the clipboard, in
  bracketed paste when the program asked for it; the wheel scrolls back through
  `terminal.scrollback` lines (10 000), and typing scrolls back to the bottom.
- When the shell exits the panel says so, and Enter starts a new one. Hiding the panel keeps the
  session; closing the window ends it, as does quitting Birchpad.

### Left for later

- Several terminals in tabs, selecting text with the mouse and copying it, links, and the
  terminal's own settings for fonts and colors.

## Consequences

- A terminal that runs what the user's shell runs, full-screen programs too, on the three
  platforms, with a dependency that is maintained for that purpose.
- The application keeps only drawing and input; what a terminal does is tested without a window.
