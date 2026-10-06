# Release checklist

The tests run in CI cover what a program can check (CONTRIBUTING.md, Tests). This list is what
they cannot: real input methods, screen readers, displays, installers and updates on real
systems. Before publishing a draft release (ADR 0009), a maintainer goes through it with the
draft's own artifacts, not a local build, and notes in the release what was not checked or
did not work.

Tick each item on at least one machine per platform: Windows 11 x64, macOS on Apple Silicon,
Linux (Ubuntu or Debian, under X11 and under Wayland). Windows on ARM, Intel Macs and other
desktops are welcome but not required.

## Before tagging

- [ ] CI is green on `main`, the non-blocking jobs included (Windows on ARM, the end-to-end
      tests on Linux and macOS, fuzzing); a red one is understood and written down
- [ ] `CHANGELOG.md`: the `[Unreleased]` section becomes the version's, with the date; the
      workspace version in `Cargo.toml` matches the tag
- [ ] `docs/ROADMAP.md` marks what the release delivers

## Installers and the first start

- [ ] Windows MSI (stable versions): installs, the Start menu shortcut starts Birchpad with its
      icon, Explorer shows its name and version in the executable's details; installing over the
      previous version keeps the settings and the session; uninstalling leaves the user's data
- [ ] Windows per-user installer: installs without administrator rights, starts, and appears in
      Settings > Apps
- [ ] Portable ZIPs: unpacked anywhere, Birchpad keeps everything in its `data` folder (Help >
      About says portable) and runs beside an installed copy
- [ ] macOS: the app opens (ad-hoc signed: Open from the context menu the first time), has its
      icon in the Dock, and quitting from the Dock keeps the session
- [ ] Linux: the binary starts from the ZIP on X11 and on Wayland; with the desktop file
      installed, the launcher shows the icon
- [ ] Opening a file from Explorer, Finder or the file manager while Birchpad runs opens it in
      the running window

## Updates

- [ ] Help > Check for Updates finds the previous release up to date and offers the new one
      once it is published (stable and nightly channels)
- [ ] After publishing, the release is in the signed update manifest (`update-manifest.yml`),
      and an installed previous version with `updates.mode = "auto"` updates itself and starts
      again with its session
- [ ] With `updates.mode = "off"`, nothing goes to the network (Help > Check for Updates is
      disabled)

## Input methods and keyboards

- [ ] Windows: Microsoft Japanese IME and Chinese Pinyin; macOS: Japanese and Chinese input;
      Linux: IBus and Fcitx 5. The composition shows at the caret with its underline, the
      candidate window opens next to it, Enter commits, Escape cancels, and Undo removes the
      committed text in one step
- [ ] Dead keys (US International: `'` then `e` gives `é`) and AltGr characters (German,
      Polish layouts) type their characters instead of running shortcuts
- [ ] With a Russian layout active, Ctrl+C, Ctrl+V, Ctrl+Z and Ctrl+F still work
- [ ] The menu bar from the keyboard: Alt or F10, Alt+letter, arrows into submenus

## Screen readers

- [ ] Windows Narrator and NVDA, macOS VoiceOver: what they read of the window title, the
      menus, the tabs, the text and the dialogs is noted in the release; a regression from the
      previous release blocks it

## Displays

- [ ] Windows at 100 %, 150 % and 200 % scaling, and a window moved between monitors of
      different scaling: text, margins, tabs and panels stay sharp and in proportion
- [ ] macOS on a Retina display and on a non-Retina external one; Linux with fractional scaling
      under Wayland
- [ ] Fonts: CJK text, emoji and combining marks render; the caret blinks and the selection
      shows in both views

## The rest of the desktop

- [ ] Files and a folder dragged from the file manager open as tabs and as Folder as Workspace
- [ ] Copy and paste to and from other programs, in plain text with Windows and Unix line
      endings; Clipboard History lists what other programs copied
- [ ] A file changed by another program is reloaded or asked about when the window comes back
- [ ] A 100 MB file opens, scrolls and searches without freezing the window
