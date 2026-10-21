# ADR 0044: Print preview and native printing

- Status: accepted
- Date: 2026-10-20
- Changes [ADR 0034](0034-printing.md): the browser prints only where the platform has no
  printing Birchpad can use, and the pages are Birchpad's own everywhere.

## Context

ADR 0034 printed through the browser: Birchpad wrote the document as one HTML page and the
browser laid it out on paper, with the browser's own headers, margins and page breaks. Notepad++
prints through Windows with its Preferences > Print: line numbers, colors (WYSIWYG, Invert,
Black on white, No background colour), margins, and a header and a footer of three parts each,
made of variables (`$(FULL_CURRENT_PATH)`, `$(FILE_NAME)`, `$(CURRENT_DIRECTORY)`,
`$(CURRENT_PRINTING_PAGE)`, `$(SHORT_DATE)`, `$(LONG_DATE)`, `$(TIME)`); it prints the
selection when there is one, and File > Print Now prints on the default printer without a
dialog. GPUI has no printing on any platform.

## Decision

- **Birchpad lays out the pages.** `birchpad_tools::print` puts the document, or its selection,
  on pages of paper as Preferences > Print sets them up (`[print]` in `settings.toml`: paper,
  orientation, margins in millimeters, the font's size in points, line numbers, colors, the
  header's and footer's parts). The text is monospaced on a grid: a cell is 0.6 of the font's
  size wide, a row 1.2 of it high, so that a page holds as many cells and rows on every printer
  and in the preview. Long lines wrap onto the next rows, East Asian wide characters take two
  cells, tabs go to the next stop; a header or a footer with text takes its row and an empty one.
- **File > Print... is a print preview** of these pages, one at a time, with Page Setup...
  (Preferences > Print, over the preview, which follows what changes) and Print....
- **Windows prints natively**: Print... in the preview opens Windows' print dialog (the
  printer, the pages, the copies), and File > Print Now prints on the default printer without
  it. The pages are drawn with GDI at the same places on the paper, in Consolas, on a thread of
  their own so that the window does not wait.
- **macOS and Linux print through the browser**, as before but with Birchpad's pages: a page
  of the paper's size for each page (CSS `@page`), opened in the browser, whose print dialog
  prints them. Native printing there needs what Birchpad does not have: on macOS,
  `NSPrintOperation` prints an `NSView` that draws the pages, and GPUI's views are not
  `NSView`s that print; on Linux the print dialog is GTK's, a toolkit Birchpad does not use.
  Print Now cannot skip the browser's dialog there.
- **Export to HTML** keeps the one page of ADR 0034, which is for reading, not for paper.

## Consequences

- The preview, the printer and the browser show the same pages, page numbers included.
- A font whose glyphs are narrower than 0.6 of its size leaves the right margin a little wider
  than set; one that has no glyph for a character prints the system's substitute.
- Printing on macOS and Linux still needs a browser.
