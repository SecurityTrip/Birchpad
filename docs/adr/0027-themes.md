# ADR 0027: Themes

- Status: accepted
- Date: 2026-10-10

## Context

Until Phase 4, every color was a constant: the editor's in `crates/app/src/editor/theme.rs`,
following Notepad++'s default style, and those of the tabs, panels and bars spread over the
application. Phase 4 brings themes, the Style Configurator and importing Notepad++ themes.

A Notepad++ theme is an XML file (`stylers.xml`, or one of its `themes` folder) with global
styles ("Default Style", "Caret colour", "Mark Style 1", ...) and styles for each of Scintilla's
lexers, by lexer and style number. Birchpad highlights with tree-sitter: its styles are named
after highlight captures (`keyword`, `function.method`), the same for every language. Notepad++
themes color only the text area; the rest of its window follows Windows, or its own dark mode.

## Decision

- **A crate of its own, `birchpad-theme`,** without UI: the theme model, its file format, the
  built-in themes and the Notepad++ import, tested and fuzzed like other input from outside.
- **What a theme holds:** editor colors (Notepad++'s global styles: text, background, selection,
  caret, current line, margins, symbols, edge, braces, highlights and the five token styles),
  colors of the rest of the window (`ui`: panels, bars, borders, lists, accents), styles by
  highlight name, and styles of one language that differ from those. A style is a color, a
  background, bold, italic and underline. Highlight names fall back to their prefix
  (`function.method` to `function`); at each step a language's own style comes before the
  theme's, so that a language's `string` does not hide the theme's `string.escape`.
- **Theme files are TOML** and set any subset of the colors; the rest come from the built-in
  theme of the same brightness, so a few lines make a theme. A file is dark when it says so
  (`dark = true`) or its editor background is dark. A style a file sets replaces the base's more
  specific ones (`string` replaces `string.escape`), so that what the file sets is not hidden by
  styles it does not know about. `function.method = "#..."` without quotes, which TOML reads as a
  table, still styles `function.method`. Keys of a newer version are ignored.
- **Two built-in themes,** Default (Notepad++'s default colors, as Birchpad had them) and Dark,
  written as complete theme files that document the format.
- **Notepad++ themes are read as they are.** Global styles become editor colors; translucent
  highlights keep Birchpad's opacity. Each lexer style is matched to a highlight name by its
  name, which Scintilla's lexers build from the same words (a style whose name says COMMENT is a
  comment), with a table for the exceptions (YAML's IDENTIFIER is a key, Python's DEF NAME a
  function). The style most lexers give a highlight becomes the theme's, and a lexer that differs
  gets styles of its own for its language. `javascript.js` is JavaScript, and Notepad++'s search
  results lexer is skipped. Fonts are not taken yet.
- **The window follows the theme:** a dark theme switches gpui-component's controls (buttons,
  inputs, dialogs) to their dark mode, and Birchpad's own panels and bars take the theme's `ui`
  colors, which Notepad++ themes do not set and the built-in theme of their brightness provides.
- **The Style Configurator** edits a copy of a theme and shows each change at once. Saving
  writes the whole theme as `<name>.toml` in the `themes` folder: it comes before a Notepad++
  file of the same name and replaces a built-in theme of that name, so the built-in files stay
  untouched. Highlighting styles are set for all languages or one; a style with nothing set is
  removed, so the name takes its prefix's style, or that of all languages, again.

## Consequences

- The match from Scintilla's style names is a guess where lexers name things their own way; a
  style without a match is dropped, and the highlight keeps the base theme's color.
- Tree-sitter grammars find more than Scintilla's lexers (properties, parameters), which
  Notepad++ themes cannot color; the base theme does.
- A theme file is resolved when it is read: later changes to the built-in themes reach theme
  files that do not set those colors, as intended.
