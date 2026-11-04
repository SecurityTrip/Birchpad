# ADR 0060: PlantUML kept running

- Status: accepted
- Date: 2026-11-04
- Changes [ADR 0057](0057-plantuml-preview-redone.md): how the preview runs PlantUML for each
  drawing.

## Context

ADR 0057 starts PlantUML for each drawing: the text rests 400 ms, Java starts, PlantUML reads
the text from its input, draws it and ends. Starting Java takes about half a second before
PlantUML reads a line, and its classes are loaded and compiled anew each time; on the diagrams
of an architecture repository, with their includes, a drawing took 1.5 to 3.5 s, most of it
Java starting and warming up.

PlantUML's `-pipe` reads diagrams from its input as they come, and writes each picture, with
the delimiter `-pipedelimitor` names after it, as soon as the diagram's `@end` line is read. A
process that is not ended can draw one text after another.

## Decision

- **One process per way of running.** For a drawing run with Java (the PlantUML Birchpad
  installs, or a `plantuml.jar`), the preview gives the text to a PlantUML kept running for the
  same program, arguments, folder and variables (the include path, Graphviz's `dot`), started
  if there is none. Up to four are kept, for documents in that many folders; the one used
  longest ago is ended past that. A program of another kind (a `plantuml` script, anything the
  setting names) is started for each drawing as before: it may not read its input as it comes.
- **The end of a drawing.** After the text, the preview writes a diagram that fails on purpose,
  `!assert 1 == 2 : "birchpad-drawing-N"`, with a number of its own. The pictures before the
  one that shows that message are the drawing's; PlantUML's error output up to that message
  (less its own `ERROR` and line) is the drawing's, and is read for mistakes as before.
  Without an exit status, a drawing has mistakes when that output has PlantUML's `ERROR` lines
  or a Java stack trace; anything else there (Java's warnings when it starts) is not a mistake.
- **Only complete texts.** A text whose diagram is still open (`@startuml` with no `@enduml`
  yet, as while it is typed) would take in the end of the drawing; it is drawn by a PlantUML
  started for it.
- **Stopping.** A drawing the text moved past ends the kept process, as a run is ended now; the
  next drawing starts another. A process that ends or does not start is not kept, and the
  drawing is made by a PlantUML started for it, which says what went wrong.

## Consequences

- Measured on the same two diagrams (a sequence and a class diagram, smetana, the PlantUML
  Birchpad installs, on Windows): started for each drawing, 0.5 to 0.7 s a drawing; kept
  running, 0.01 to 0.03 s once warm, and the first drawing of a process as long as before.
  The ignored test `a_kept_plantuml_draws_faster` measures it again with that PlantUML (1.38 s
  against 0.05 s for the two).
  The preview keeps it, as the roadmap asked, because it is faster.
- Up to four Java processes stay alive while Birchpad runs, each holding the memory of a
  PlantUML; they end with Birchpad, whose end closes their input.
- A diagram whose text PlantUML reads wrongly in a stream (an `@end` line inside a note, say)
  can leave a kept process out of step; the drawing then fails or is stopped, and the process
  is ended with it.
