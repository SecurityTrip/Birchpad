# ADR 0045: PlantUML installed on request

- Status: accepted, changed by [ADR 0057](0057-plantuml-preview-redone.md)
- Date: 2026-10-20
- Changes [ADR 0042](0042-previews.md): PlantUML previews no longer need PlantUML installed
  apart from Birchpad.

## Context

ADR 0042 draws PlantUML diagrams with the PlantUML installed on the computer: the `plantuml`
command, or a `plantuml.jar` run with Java. Few users have either, so for most a PlantUML
document's preview said how to install them and showed nothing. Drawing on a PlantUML server
works without installing anything, but sends the text of the diagram over the network, which
Birchpad does not do (`PRIVACY.md`). A port of an editor's PlantUML extension (VS Code's) would
not help: it runs the same Java program or calls the same servers, and Birchpad's plugins can do
neither.

## Decision

- **Birchpad installs PlantUML when asked.** When the preview finds no PlantUML, it offers
  Install PlantUML (and View > Preview has it). One click downloads, for this platform, a ZIP of
  PlantUML and the Java that runs it, checks it, unpacks it into Birchpad's data folder
  (`components/plantuml-<version>`), and every PlantUML preview draws with it from then on.
- **What is installed.** PlantUML's MIT build (`plantuml-mit` from Maven Central, checked
  against its SHA-256), so that nothing under the GPL is shipped; and a Java runtime made for it
  with `jlink` from Eclipse Temurin (GPL 2 with the Classpath Exception, a separate program),
  with only the modules PlantUML needs. About 50 MB per platform, for Windows x64 and ARM64,
  Linux x64, and macOS on Apple Silicon and Intel. Diagrams use PlantUML's own layout
  (`-Playout=smetana`), so that Graphviz is not needed either.
- **Built by Birchpad's workflow, pinned in Birchpad.** `.github/workflows/plantuml.yml` builds
  the packages when a tag `plantuml-<PlantUML's version>-<revision>` is pushed, checks that each
  draws, attests their provenance, and publishes them as that tag's pre-release. Birchpad holds
  each package's URL, size and SHA-256 (`birchpad_update::component::PLANTUML`); a package that
  differs is refused and deleted. A new PlantUML is a new tag and new pins in a release of
  Birchpad, which then installs it in place of the older one.
- **The network only on request.** The download is the user's click, through the transport of
  updates (`updates.proxy` too). With `network.online-features = false` the preview says that
  downloads are off and installs nothing.
- **The user's PlantUML first.** A `preview.plantuml` other than `plantuml` (a command or a
  `.jar` of the user's) is used as before; the installed one runs only for the default.

## Consequences

- PlantUML previews work after one click, on every platform Birchpad ships for, without Java,
  PlantUML or Graphviz on the computer.
- Birchpad's repository publishes releases that are not Birchpad's: `plantuml-*` pre-releases,
  which the update manifest's workflow leaves out.
- Diagrams that need Graphviz's own layout look a little different with Smetana; a user who
  wants Graphviz sets `preview.plantuml` to a PlantUML of their own.
- The MIT build lacks the few diagram kinds that only the GPL build has (ditaa).
