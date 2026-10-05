# ADR 0019: The menu bar from the keyboard

- Status: accepted
- Date: 2026-10-05

## Context

On Windows and Linux the menu bar is drawn in the window (macOS has its native one). Phase 1 used
gpui-component's `AppMenuBar`. It opens menus with the mouse, and the arrows work inside an
open menu, but nothing gets there from the keyboard: Alt and F10 do nothing, there are no
mnemonics, and its popup menus cannot be told which item to select, so a letter cannot choose
one. Notepad++ users rely on Windows menu keys: Alt or F10 to the menu bar, Alt+F for File,
letters for items.

## Decision

- **Birchpad's own menu bar** (crate `birchpad-menu-bar`, which knows only GPUI: the application
  installs the menus and routes the window's input to it), fed from the same menu data as
  before, draws the titles and the menus and keeps one state: closed, a title selected, or a
  menu open with the selected item of each open level. The menu bar has a single focus handle
  while it is in use; the focus it took is given back when it is done, and commands are
  dispatched from there, so they reach the editor that was active.
- **Keys, as in Windows menus:**
  - Alt pressed and released alone, or F10, selects the first title; again, or Esc, gives the
    focus back. GPUI reports a modifier released without other input as a keystroke, so Alt is
    an ordinary key binding. A mouse click while Alt is held (Alt+drag selects a rectangle) or a
    window switch (Alt+Tab) is not a tap.
  - Alt+letter opens the menu with that mnemonic, from the editor or from another open menu.
    Key bindings come first: Alt+C stays the Column Editor.
  - In the menus: the arrows (Right opens a submenu, or the next menu; Left closes a submenu, or
    goes to the previous menu), Home, End, Enter and Space, Esc one level back. A letter chooses
    the item it is the mnemonic of; if several share it, it moves between them.
- **Mnemonics are picked, not written in the labels**: the first letter of the label, of a later
  word, or any later letter or digit that no earlier item in the same menu has. Notepad++'s
  titles come out the same: File, Edit, Search, View, E*n*coding, Language, Help. Labels stay
  plain text for other uses (a command palette later). Mnemonics are underlined while Alt is
  held or the keyboard is in use, as Windows shows them.
- **Mouse:** a click on a title opens or closes its menu, hovering another title while one is open
  switches, hovering a submenu opens it, a release on an item chooses it (so press on a title,
  drag and release works), a click outside closes the menus and goes nowhere else.

## Consequences

- About a thousand lines replace a component from the library; the popups elsewhere (Reopen with
  Encoding, the status bar menus) still use gpui-component's.
- Letters are matched by the key GPUI reports. On Windows that is the Latin letter of the key
  whatever the layout, so Alt+F opens File with a Russian layout too (checked in the running
  application). On Linux GPUI reports the layout's character: with a non-Latin layout, mnemonics
  do not match and the arrows remain.
- Tested with GPUI's test platform on Windows and Linux CI; the keys were checked in the running
  application on Windows. Linux desktops differ (GNOME applications use F10 and do not focus the
  menu bar on Alt); Birchpad behaves the same everywhere.
