# ADR 0002: GPUI as the UI framework

- Status: accepted (after the Phase 0 spike)
- Date: 2026-10-01

## Context

The editor view needs proper text shaping (complex scripts, font fallback, emoji), IME support,
fast custom painting of only the visible lines, and enough widgets for the rest of the application
(menus, tabs, docks, trees, dialogs). Candidates were GPUI (Zed's framework), egui and
iced with cosmic-text.

## Decision

Use **GPUI through `gpui-kit`** (Longbridge), which bundles a pinned GPUI snapshot (`gpui-pre`),
`gpui-base` and `gpui-component` (dock, tabs, tree, menus, inputs, dialogs). Birchpad's editor view
is our own element on top of `birchpad-core`; we do not use gpui-component's code editor.

## Spike results (Windows 11, x64)

The spike (`crates/app`) renders the document with `uniform_list`, one custom element per visible
line, carets and selections as quads, and an `EntityInputHandler` for typing and IME.

- Builds cleanly on Windows with MSVC; no extra SDKs needed. Clean debug build of dependencies
  takes about 2 minutes.
- Cyrillic, CJK (via font fallback), accented Latin and color emoji render correctly in Consolas.
- Typing (including Cyrillic), Enter, selection with Shift+arrows across lines, undo/redo and
  paging work through `birchpad-core`.
- Release build, generated 1 000 000-line, 75 MB document: building the rope takes ~0.1 s, the
  window is up in ~1.4 s including generating the text, Ctrl+End to the last line responds in
  ~60 ms, typing at line 1 000 000 is immediate, working set ~150 MB.
- Debug build, 200 000 lines / 15 MB: opens in ~0.3 s, ~100 MB working set.

## Risks

- `gpui-pre` is a snapshot of Zed's GPUI published by a third party; Zed's own `gpui` crate on
  crates.io is not updated regularly. If `gpui-kit` stalls we can depend on Zed's repository by
  git revision; our code only uses GPUI through `gpui_kit::*`.
- API churn: GPUI is pre-1.0. Keep the editor view's GPUI surface small and isolated.
- Unverified yet: Linux (Wayland/X11) and macOS builds, Windows on ARM, real IME composition
  (Japanese/Chinese), screen readers.

## Fallback

If GPUI becomes untenable, the text model is UI-independent; iced with cosmic-text is the next
candidate for the editor view.
