# ADR 0059: PlantUML's layouts, and Graphviz

- Status: accepted, changed by [ADR 0063](0063-plantuml-epl-build-with-elk.md)
- Date: 2026-11-04
- Changes [ADR 0057](0057-plantuml-preview-redone.md): which layout PlantUML lays diagrams out
  with, and how a crash of it is shown. Keeps [ADR 0045](0045-plantuml-installed-on-request.md)
  as it is: Birchpad installs PlantUML on request, not Graphviz.

## Context

PlantUML lays out most diagrams other than sequence diagrams (class, component, C4 context and
container diagrams) with a layout engine: Graphviz's `dot`, a program of its own, or smetana,
a port of a part of Graphviz to Java that comes in `plantuml.jar`. The PlantUML Birchpad
installs (ADR 0045) runs with smetana, so that it needs nothing else; a PlantUML of the user's
own ran with whatever PlantUML chose.

On a real architecture repository, smetana crashed on the two largest diagrams (about 3,400
lines once included): `NullPointerException` in `SmetanaEdge`. The preview listed Java's stack
trace as the mistake, dozens of lines that say nothing to a reader. The same diagrams are drawn
on Confluence and in IDEs, which use Graphviz.

Graphviz could be installed by Birchpad the way PlantUML is, as a component of the update
manifest. Against it:

- Graphviz is a native program with libraries of its own (cairo, expat, its plugins), built
  per platform; on Linux and macOS the builds are the systems' packages, which a portable
  archive cannot replace without carrying their libraries along. PlantUML's package is one jar
  and a Java runtime; Graphviz's would be a build for each platform kept by the project.
- Its license (EPL 1.0 for the releases Birchpad could ship) asks for its source to be offered
  with the binaries, a duty the PlantUML package does not add.
- Those who meet smetana's limits are those whose diagrams are large and shared, drawn on
  Confluence or in an IDE: they mostly have Graphviz installed already.

## Decision

- **A layout setting.** `preview.plantuml-layout` (Preferences > Preview): `automatic` (the
  default), `smetana` or `graphviz`. Automatic lays out with Graphviz when its `dot` is found
  and with smetana otherwise; it applies to every PlantUML the preview runs, the installed one
  and the user's own.
- **Where `dot` is found.** The `GRAPHVIZ_DOT` variable when it names a file, else `dot` on
  `PATH`, else, on Windows, Graphviz's own installation folder (`Program Files\Graphviz\bin`),
  since its installer does not put it on `PATH` by default. The path found is given to PlantUML
  as `GRAPHVIZ_DOT`; smetana is asked for with `-Playout=smetana`. With `graphviz` set and no
  `dot` found, PlantUML looks for it itself and says so in its picture when it fails.
- **A crash is one line.** When PlantUML ends with a Java exception rather than its own
  `ERROR` lines, the preview shows one line: PlantUML could not draw the diagram, with the
  layout it used and the exception's first line; the stack trace is folded under it, shown by
  a click, for a bug report.
- **Graphviz is not installed by Birchpad.** PlantUML stays the only component it installs on
  request; the crash line says that Graphviz lays out what smetana cannot. This is looked at
  again if smetana fails on diagrams most people draw.

## Consequences

- With Graphviz installed, diagrams in the preview are laid out as on Confluence and in IDEs,
  without a setting to change.
- A user's own PlantUML without Graphviz used to fail on diagrams that need a layout; with the
  automatic layout it draws them with smetana.
- Diagrams laid out by smetana can look different from Graphviz's, and the layout used depends
  on what is installed: the same document can look different on two machines. `smetana` or
  `graphviz` in the settings fixes it.
- Telling a crash from PlantUML's mistakes relies on Java's form of a stack trace (`at` lines);
  when it is not there, the error output is shown as it is, as before.
