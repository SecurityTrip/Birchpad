# ADR 0063: PlantUML's EPL build, with ELK

- Status: accepted
- Date: 2026-11-04
- Changes [ADR 0045](0045-plantuml-installed-on-request.md) (which build of PlantUML Birchpad
  installs) and [ADR 0059](0059-plantuml-layouts-and-graphviz.md) (what lays a diagram out
  when smetana cannot).

## Context

ADR 0059 lays diagrams out with Graphviz when it is installed and with smetana, PlantUML's own
layout, otherwise. On the largest diagrams of an architecture repository smetana crashed. It is
reproduced on made-up C4 container diagrams: two `System_Boundary` with `Container_Boundary` in
them and 43 relations of every kind across them (105 lines) are enough for smetana's routing of
a spline to fail ("triangulation failed" in `routesplines`, its port of Graphviz 2.38), the edge
to be lost, and a `NullPointerException` in `SmetanaEdge`. `skinparam linetype polyline` or
`ortho` does not help. Without Graphviz there was no way to see those diagrams at all.

PlantUML has a third layout, ELK (Eclipse Layout Kernel). Since PlantUML 1.2024.6 it is in
`plantuml.jar`, but not in the MIT build Birchpad installs (ADR 0045 chose it so that nothing
under the GPL is shipped): ELK is under the Eclipse Public License 2.0. PlantUML's EPL build
(`plantuml-epl` on Maven Central) has it, with EMF and Guava, but lacks Xtext's library
(`org.eclipse.xtext.xbase.lib`), which ELK's layered algorithm calls; with that library on the
class path, ELK lays out every one of those diagrams, smetana's crashes too, in about the time
smetana takes.

The EPL is no GPL: a weak copyleft, by file. Shipping PlantUML's EPL build unchanged, beside
Birchpad and run as a program of its own, asks for its notices (in the jar) and for where its
source is (Maven Central's sources jar) to be given; it asks nothing of Birchpad's own code.
ADR 0059's reasons not to install Graphviz still hold (native builds for each platform); its
point about the EPL 1.0 of Graphviz was a small one, and does not stand against a Java library.

## Decision

- **The EPL build.** The PlantUML package Birchpad installs (`.github/workflows/plantuml.yml`)
  is PlantUML's EPL build, with `lib/org.eclipse.xtext.xbase.lib.jar` from Maven Central, both
  checked against their SHA-256, and the Java runtime made for both; its `README.txt` gives
  their licenses and where their sources are. It is revision 3 of the package,
  `plantuml-1.2026.9-3` (the build of revision 2 stopped before it published anything). The
  package's workflow checks that it draws with smetana and with ELK. Its runtime's modules are
  named (those of revision 1's runtime, with `jdk.unsupported`): `jdeps` cannot list them for
  the EPL build, whose Guava names a module that is not there.
- **Running it.** A package with a `lib` folder runs PlantUML with the class path of
  `plantuml.jar` and `lib/*` and its main class, `net.sourceforge.plantuml.Run`; one without
  (revision 1) with `-jar`, as before.
- **ELK where smetana crashes.** With the automatic layout, a drawing smetana crashes on is
  drawn again with ELK; when ELK draws it, the preview shows that drawing and says, above the
  diagrams, that smetana could not lay it out and ELK did. When ELK is not there or crashes
  too, the crash of smetana is shown as before (ADR 0059). `preview.plantuml-layout` gets
  `elk`, to lay out with ELK always.
- **An earlier revision keeps working.** Birchpad uses the PlantUML of an earlier version or
  revision still installed until the new one is; under a crash of smetana, the preview then
  offers to install PlantUML again, with ELK.

## Consequences

- Diagrams smetana cannot lay out are drawn without Graphviz, by the PlantUML Birchpad
  installs; Graphviz, when found, still comes first.
- The PlantUML Birchpad installs is under the EPL 2.0 rather than MIT, and its package is about
  10 MB larger (ELK, EMF and Guava: 59 to 65 MB for each platform, from 48 to 54). The rule of
  ADR 0045, nothing under the GPL shipped, holds.
- ELK lays diagrams out otherwise than smetana and Graphviz: a diagram drawn with it after a
  crash looks different from the same diagram drawn on Confluence.
- A crash costs a second run of PlantUML, with ELK, before the preview shows anything.
