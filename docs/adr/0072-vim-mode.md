# ADR 0072: Vim mode

- Status: accepted
- Date: 2026-11-27

## Decision

Preferences > Editing > Vim mode is off by default. Each editor view starts in Normal mode
when the preference is enabled. A focused editor handles modal keys before the ordinary
keymap. With the mode off, the keymap and text input work as before. Insert mode uses the
usual text input path, including input methods. An indicator in the editor names the mode.

Normal mode covers movements, entering Insert and Visual modes, changing, deleting and
yanking text, putting it back, and undo and redo. Visual mode selects whole graphemes,
including the one under the cursor. Modal changes use the editor's ordinary transactions
and read-only checks. Yank, delete and change share an unnamed register for the app session.
The first implementation covers the keys listed in the roadmap. Ex commands, named registers,
text objects, visual line and block modes, and Vim mappings are outside this implementation.

The behavior comes from the Vim user-facing documentation for [modes][modes], [motions and
operators][motions], and [changes][changes].

[modes]: https://vimhelp.org/intro.txt.html#vim-modes-intro
[motions]: https://vimhelp.org/motion.txt.html
[changes]: https://vimhelp.org/change.txt.html
