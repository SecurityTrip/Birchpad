# ADR 0033: Localization

- Status: accepted
- Date: 2026-10-12

## Context

Birchpad's interface was English only. Notepad++ ships many translations as XML files keyed by
control ids. Phase 4 starts with English and Russian.

## Decision

- **The English text is the key,** as in gettext: code keeps writing English, and
  `i18n::tr("Save")` gives the translation of the language in use, or the English text when
  there is none. Adding a string never breaks a translation; it only shows in English until
  translated.
- **Catalogs are TOML files** in `crates/app/locales` (`ru.toml`: `"Save" = "Сохранить"`),
  built into the program. A test checks that every menu title and every command a menu lists
  without arguments has a translation in each catalog.
- **What is translated:** menus (built through the menu model, so the macOS menu bar and the
  in-window one both), dialog titles, buttons and labels, Preferences, the Style Configurator,
  the status bar's language. Command titles in the Shortcut Mapper, error messages and names
  from files (themes, macros, languages) stay as they are.
- **`appearance.language`:** `auto` (the default) follows the system's language
  (`sys-locale`), `en` and `ru` choose one; Preferences > Appearance changes it at once.

## Consequences

- Strings built with `format!` are translated piece by piece, or not at all yet.
- More languages are one catalog each; a missing string falls back to English.
