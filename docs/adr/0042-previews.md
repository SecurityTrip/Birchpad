# ADR 0042: Previews beside the text

- Status: accepted
- Date: 2026-10-19

## Context

Markdown is written to be read rendered, and checking the result means leaving the editor.
Notepad++ has no preview of its own (plugins add one in a docked panel). IntelliJ IDEA shows a
Markdown file as its text and the rendered page side by side, with three buttons at the top
right of the editor: Editor Only, Editor and Preview, Preview Only (issue #41). Its PlantUML
plugin shows `.puml` diagrams the same way (issue #42).

## Decision

- **Beside the text, not in a panel.** The preview belongs to the document, as in IDEA: the
  pane draws a Markdown document's view and its preview side by side with a splitter, or one
  of them alone. A side panel would show only the active document and compete with Function
  List and Document Map for the right dock.
- **Three modes.** The buttons at the top right of the document and View > Preview (Editor
  Only `view.preview-editor`, Editor and Preview `view.preview-split`, Preview Only
  `view.preview-only`) switch the active document's mode; the menu checks the one shown. Each
  view of a document has its own mode; the one chosen last is kept in `state.toml`
  (`preview`) and applies to documents shown later. Editor and Preview is the default, as in
  IDEA. On a document without a preview the commands say so.
- **Which documents.** A document has a preview by its language: Markdown, whether found from
  the extension or chosen in the Language menu. PlantUML has no language in Birchpad yet, so a
  PlantUML document is known by its extension: `.puml`, `.plantuml`, `.pu`, `.iuml`, `.wsd`. A
  document that stops being either shows its text alone again.
- **Rendering.** GPUI Kit's `TextView` renders the Markdown (CommonMark with GitHub's tables,
  task lists and strikethrough), parsing it in the background. The preview follows every edit,
  undo included, and is drawn in the theme's text and background colors.
- **Links and images.** A link with a scheme (`https:`, `mailto:`) opens in the system's
  browser; a path, relative to the document's folder or a `file:` URL, opens the file in
  Birchpad; `#anchors` do nothing yet. Images are read from files next to the document and
  from `data:` URLs. Images on the web are not fetched: Birchpad connects to the network only
  for updates (`PRIVACY.md`).

- **PlantUML draws its own diagrams.** PlantUML is a Java program under the GPL; Birchpad
  does not bundle it but runs the one installed: `preview.plantuml` in the settings names the
  `plantuml` command (the default, found on `PATH`) or a `plantuml.jar`, run with `java -jar`.
  It reads the text on its standard input and writes each diagram as SVG
  (`-tsvg -pipe -pipedelimitor`), which the preview shows one under the other. It runs in the
  background once the text has rested for 400 ms, since each run starts Java; a new run
  replaces one under way, and the diagrams drawn last stay until the new ones arrive. A
  diagram with mistakes comes back as PlantUML's picture of them. When PlantUML cannot be
  started, the preview says how to install or set it. Rendering on a PlantUML server was left
  out: it would send the text over the network.

## Consequences

- The preview does not scroll with the text yet, and anchors inside the document are not
  followed.
- PlantUML diagrams inside Markdown code blocks are not drawn yet.
- A diagram that includes files from the web (`!includeurl`) makes PlantUML fetch them; that
  is PlantUML's doing, on the user's request.
- Other kinds of preview fit the same frame: a language or extensions, a renderer, the same
  modes.
