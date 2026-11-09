# ADR 0066: Git through its command line

- Status: accepted
- Date: 2026-11-09
- Builds on [ADR 0047](0047-compare-as-compareplus.md) (Compare, which already compares a
  document with Git's HEAD) and [ADR 0041](0041-crate-layout.md).

## Context

Roadmap 7.6 asks for the changes of a file against its last commit in the margin, the blame
of a line, and the difference of a file from its last commit. Plugins > Compare already does the
last one: Compare to Git HEAD opens HEAD's text read-only beside the document and marks what
differs, as `git show` gives it. The other two need HEAD's text of every open file and a
line's last commit.

A Git library would be the larger part of Birchpad: libgit2 (`git2`) is C code built with every
release on three platforms, `gix` hundreds of thousands of lines and dozens of crates. Every
user of Git has its command line, which reads their configuration (`core.autocrlf`, filters,
`safe.directory`, credentials) as their other tools do.

## Decision

### The `git` program, asked

- Birchpad runs `git` (`git.program`, `git` on the `PATH` by default) for what it needs and
  reads its output; nothing is bundled. `git.enabled = false` turns all of it off.
- The crate `birchpad-git`, without a window, runs it (without a console window on Windows)
  and reads what it says: a file's text in HEAD (`git show HEAD:./<name>` in the file's folder),
  a line's blame (`git blame --porcelain -L <n>,<n> --contents - -- <name>`, the document's text
  as it is now on standard input, so that unsaved and uncommitted lines blame as such), and the
  changes of the lines between two texts. Blame's output is fuzzed (`git_blame`).
- A file outside a repository, untracked, or in a repository without commits has no HEAD text:
  no changes are shown for it, and nothing is said. Git that cannot run is said once.

### Changes in the margin

- A margin of its own, right of the change history's, shows each line's change against HEAD:
  a bar for added lines (`git-added`), for modified lines (`git-modified`), and a mark where
  lines were deleted (`git-deleted`), three new theme colors. Preferences > Margins >
  Git changes (`editor.git-changes`, on) shows or hides it.
- HEAD's text is read in the background when a document with a path opens, after it is saved
  or read again, and when the window comes back to the front (a commit may have been made
  meanwhile). The changes are computed from it and the document's text with the line
  comparison the change history uses (`diff_lines`), again after each edit, at most every
  300 ms. Large files (Preferences > Performance) get none.
- Tools > Git > Next Change and Previous Change go to the first line of the next and the previous
  block of changed lines, around the ends.

### Blame and the difference

- Tools > Git > Blame Line shows the caret's line's commit (its short id), author, date and
  summary in the call tip; a line not committed says so.
- Tools > Git > Compare with Last Commit is Compare to Git HEAD, through `git.program`.

## Consequences

- Git's own behavior holds: its configuration, its filters, its idea of HEAD.
- Each check runs a process; HEAD's text is read once per document and event, not per edit.
- Staged changes, other revisions than HEAD, and history views are left for later.
