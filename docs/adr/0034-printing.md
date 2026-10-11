# ADR 0034: Printing

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++ prints through Windows' printing, with its colors and line numbers. GPUI has no
printing API on any platform, and Birchpad runs on Windows, macOS and Linux.

## Decision

- **Printing goes through the browser:** File > Print... (Ctrl+P) writes the document as an
  HTML page to a temporary file and opens it in the default browser, which shows its print
  dialog at once. The browser's dialog chooses the printer, the pages, the paper, margins and
  headers, on every platform, and can print to PDF.
- **The page** has the document's text in the theme's colors, with its syntax highlighting (the
  same styles as on screen, the inner highlight winning where they nest) or its user defined
  language's colors, and line numbers that are not copied with the text. Long lines wrap.
- **File > Export to HTML...** saves the same page, without the print call, where the user
  chooses.

## Consequences

- Printing needs a browser; a system without one cannot print from Birchpad.
- The page shows the whole document; printing a selection is done with the browser's page
  range.
