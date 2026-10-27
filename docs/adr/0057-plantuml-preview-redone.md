# ADR 0057: The PlantUML preview redone

- Status: accepted
- Date: 2026-10-28
- Changes [ADR 0042](0042-previews.md) and [ADR 0045](0045-plantuml-installed-on-request.md):
  how a PlantUML document's preview runs PlantUML and shows what it drew.

## Context

ADR 0042 shows a PlantUML document's diagrams beside its text, and ADR 0045 installs the
PlantUML that draws them. Using it, people asked for more (issues #50 to #53):

- A diagram that includes another file by a relative path (`!include common.puml`) fails:
  PlantUML reads the text from its standard input and looks for the file in its own working
  folder, Birchpad's, not the document's.
- A drawing is shown, and that is all: it cannot be saved as a picture or copied.
- A large diagram is shrunk to the width of the preview, and cannot be enlarged.
- The preview's background is the theme's, so on a dark theme a diagram drawn for white paper
  (dark lines, no background of its own) is hard to read.

Two things of the old preview were also wanting: each change of the text started PlantUML again
without ending the run it replaced, so fast typing left Java processes drawing text already
gone; and a diagram with a mistake was only PlantUML's picture of it, with nothing to say on
which line of the document it is.

## Decision

- **PlantUML runs in the document's folder.** The process starts in the folder of the document
  (Birchpad's own for an untitled one), so that `!include`, `!includesub`, `!import` and images
  by relative path are found as they are next to the file. A `preview.plantuml` that is a
  relative path (`tools/plantuml.jar`) is taken from Birchpad's working folder before that, as
  it was.
- **A run the text moved past is stopped.** The text rests 400 ms before PlantUML draws it, as
  before; when a newer drawing starts, the process of the one it replaces is ended, and a run
  not started yet never starts. The diagrams drawn last stay until the new ones arrive.
- **Mistakes are listed by their line.** When PlantUML ends with an error, its error output
  (`ERROR`, the line in the diagram, the message, for each diagram with a mistake) is read: each
  mistake is placed on the document's line by the diagram whose picture shows its message and
  the line where that diagram starts, and shown above the diagrams as "Line 9: Syntax Error?",
  a link that puts the caret on that line. Error output of another form is shown as it is; what
  a run that succeeded writes there (Java's warnings) is not a mistake.
- **Zoom and scroll.** The preview's toolbar has Zoom Out, the zoom (a click sets 100%), Zoom In
  and Fit; Ctrl with the mouse wheel zooms too, and View > Preview has the same commands. Fit,
  the default, shrinks a diagram wider than the preview to its width, as before; the zoom goes
  from 25% to 400% by steps, and from Fit starts at 100%. A zoomed diagram keeps its size and
  the preview scrolls both ways. Above 100% the SVG is drawn again at the larger size, so that
  it stays sharp, up to a limit of pixels past which it is enlarged as a picture.
- **Background.** Diagrams are drawn on white by default, as PlantUML draws them for paper;
  `preview.plantuml-background` (Preferences > Preview) can give them the theme's background
  instead. The toolbar's button switches between the two and saves the choice in the settings,
  so that later diagrams have it too. A diagram's own background (`skinparam backgroundColor`)
  is drawn by PlantUML over it either way.
- **Export.** The context menu of a diagram (and View > Preview, for the first one) has Copy
  Diagram (a PNG on the clipboard), Export Diagram as PNG... and Export Diagram as SVG...
  The SVG is PlantUML's own; the PNG is drawn from it by Birchpad (resvg, which GPUI already
  uses to draw SVG), with the system's fonts, at `preview.plantuml-png-scale` times its size
  (2 by default, 1 to 8), on the background the preview gives it: white, or transparent with
  the theme's background. Nothing goes over the network and PlantUML is not run again.

## Consequences

- Diagrams split over several files work in the preview as they do with PlantUML on the
  command line.
- Typing in a large diagram starts at most one PlantUML at a time.
- The lines of mistakes rely on the form of PlantUML's error output and on its pictures of
  mistakes repeating the message; when either changes, the mistakes are still shown, without
  their lines.
- PNG export draws text with the system's fonts, as the preview does; a font PlantUML asked
  for that the system lacks is replaced, so the PNG can differ a little from a PNG PlantUML
  would make.
- The zoom is the preview's, not remembered between documents or sessions.
