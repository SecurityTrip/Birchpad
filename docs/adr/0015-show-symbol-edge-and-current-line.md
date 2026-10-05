# ADR 0015: Show Symbol, the vertical edge and the current line

- Status: accepted
- Date: 2026-10-05

## Context

Notepad++ can draw what the text does not show. View > Show Symbol covers spaces and tabs,
line endings, indentation guides and the wrap symbol. Preferences add the vertical edge and
the current line indicator. Scintilla draws all of these, and Notepad++ users expect them to
look and behave the same. ADR 0008 gave the editor a cell grid and per-frame layout of the
visible rows only; these aids are more things drawn over those rows.

## Decision

- **Settings and switches.** New `editor.*` settings, with Notepad++'s defaults:

  | Key | Default | Meaning |
  |---|---|---|
  | `show-whitespace` | `false` | a dot for each space, an arrow across each tab |
  | `show-eol` | `false` | a box labeled `CR`, `LF` or `CRLF` after each line |
  | `indent-guides` | `true` | dotted guides in the indentation |
  | `wrap-symbol` | `false` | a symbol at the end of each wrapped row |
  | `current-line` | `"background"` | `"off"`, `"background"` or `"frame"` |
  | `current-line-frame-width` | `1` | frame width in pixels, 1 to 6 |
  | `edge` | `"off"` | `"off"`, `"line"` or `"background"` |
  | `edge-columns` | `[80]` | a line at each in line mode; the first one in background mode |

  `edge-columns` replaces Phase 2's `edge-column`, which is now ignored like any unknown key.
  The first four are also View > Show Symbol switches: Show Space and Tab, Show End of Line,
  Show All Characters, Show Indent Guide and Show Wrap Symbol. As with Word Wrap, the
  switches are saved in `state.toml` and override the setting. `settings.toml` changes only
  when the user edits it. Show All Characters is checked when both spaces and line endings
  are shown. It turns both on, or both off if both are on. The edge and the current line are
  Preferences items in Notepad++, so they wait for the Preferences dialog (Phase 4).
- **Drawing.** Only the visible rows are measured, as with the rest of the layout (ADR 0008):
  - Spaces and tabs are found in the shaped part of each row (`DisplayText::chars`, one pass)
    and placed by the shaped glyph positions. Everything is drawn with quads, crisp at any
    scale and without paths: spaces as square dots, tabs as Scintilla's long arrow.
  - Line ending boxes sit after the end of the text, with labels shaped at 70% of the font
    size (`LineEnding::at`).
  - Indentation guides follow Scintilla's "look both" mode, which Notepad++ uses
    (`birchpad_view::indent_guides`):
    - One guide every tab width columns inside a line's leading whitespace, none at column 0.
    - A blank line takes the deeper indentation of the nearest lines with text within 20
      lines above and below. A line above that starts a fold counts one level deeper.
    - Guides in adjacent rows join into runs, drawn dotted every other pixel.
  - The edge in line mode is a one-pixel line at each column across the text area. In
    background mode, the text at or past the first column of its line gets the edge color,
    including on wrapped rows.
  - The current line is the line of the primary caret, across all its rows and the full
    width of the text area. It is drawn as a background under the decorations and the
    selection, or as a frame.
  - The wrap symbol (↵) sits near the right border, as in Scintilla's default. Showing it
    takes one column from the wrap width.
- **Colors** follow Notepad++'s default style (stylers.xml): orange whitespace symbols
  (`FFB56A`), grey guides (`C0C0C0`), the edge `80FFFF` and the current line `E8E8FF`.
  Line ending boxes are light grey with dark text. Phase 4 themes replace the constants.
- **Split Lines** (Ctrl+I) follows Notepad++: it breaks at the first edge column while the
  edge is shown, else at the width of the view.

## Consequences

- With every aid on, a debug build on a 14 MB, 200,000-line file spends about 1 ms more per
  frame on layout (median 2.8 ms against 1.8 ms).
- The drawn aids are not text, so copying never includes them, and the caret and the mouse
  ignore them.
- Notepad++ 8.6's Show Non-Printing Characters and Show Control Characters & Unicode EOL are
  not covered. Control characters already always show as pictures (ADR 0008). Unicode line
  separators are not line breaks in Birchpad.
- Highlighting the indentation guide of the matching brace, which Notepad++ does, is left
  for later.
