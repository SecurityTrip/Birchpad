# ADR 0033: Localization

- Status: accepted (revised 2026-10-12: texts by key, in YAML; 2026-10-13: a crate of its own,
  the crates without a window translate their messages)
- Date: 2026-10-12

## Context

Birchpad's interface was English only. Notepad++ ships many translations as XML files keyed by
control ids. Phase 4 started with English and Russian, using the English text as the key; that
left composed texts (`format!`), error messages and command titles untranslated, and made every
change to an English sentence a silent loss of its translations. Phase 5 translates the whole
interface.

## Decision

- **Every text of the interface is looked up by key:** `t!("menu.file")`, or
  `t!("macros.there-is-no-macro-named", name = name)` for a text with arguments. Keys are
  grouped by the part of the program that shows them (`find.*`, `status-bar.*`,
  `preferences.*`); texts used in several places are `common.*`.
- **A crate of its own, `birchpad-i18n`,** holds the locales, their parsing, `t!`, plural forms
  and the language in use, with no window, so that every crate translates its texts.
- **Locales are YAML files** in `crates/i18n/locales`, built into the program: `en.yaml` has
  every key and is the reference; `ru.yaml` translates them. A key a language lacks shows in
  English. Arguments are named (`{name}`), so a translation may put them in any order.
- **Plural forms** are a mapping of CLDR's forms (`one`, `few`, `many`, `other`) with
  `{count}`; `plural.<noun>` holds the counted nouns of the find panel and the search results.
- **Data gets keys from its ids:** command titles are `command.<id>` (menus without their own
  label, the Shortcut Mapper), menu labels `menu.<label>`, panels `panel.<id>`, Preferences
  `preferences.<label>`; the English in the code stays as the fallback.
- **The crates without a window translate their messages where they make them**, with keys
  that start with the crate's name (`io.*`, `config.*`, `plugin.*`, `update.*`); an error's
  `Display` is its text in the interface's language. (Until 2026-10-13 they spoke English and a
  locale's `text` section matched their sentences, which broke silently when a sentence changed.)
  Messages of the system and of other libraries are shown as they come.
- **The language is the process's**, so that a message made on a background thread is in it
  too; for tests the `per-thread` feature makes it each thread's, so tests that switch it run
  side by side.
- **Tests keep the locales whole:** every key of `en.yaml` is in every locale with the same
  arguments, every `t!` key of the code of every crate is in `en.yaml`, every command, menu label and panel has
  its text.
- **`appearance.language`:** `auto` (the default) follows the system's language
  (`sys-locale`), `en`, `de` and `ru` choose one; Preferences > Appearance changes it at once.
  German (`de.yaml`, since 0.4.0) is translated for Birchpad, as Russian is.
- **Notepad++'s translations are read, not bundled.** Its `nativeLang` files (`localization/
  german.xml` in a Notepad++ folder) are under the GPL, so Birchpad ships none; a user who has
  Notepad++ can point `appearance.native-lang` at one. Its main menu names (by `menuId`) and
  the titles of the File, Edit and Search commands both editors have (by Notepad++'s command
  ids, `NATIVE_LANG_COMMANDS`) then come before the language's own texts
  (`birchpad_i18n::set_native_texts`), so menus read as the user knows them from Notepad++;
  everything else stays in the interface's language. A file that does not read is ignored.

## Consequences

- A new text needs a key in `en.yaml` (the test says which) and, to be translated, in the other
  locales (the test says which too).
- Names from files (themes, macros, languages, scripts, plugins) and the names of encodings
  and languages are shown as they are.
- More languages are one YAML file each, with no code to change but the list of languages.
