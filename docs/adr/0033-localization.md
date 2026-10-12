# ADR 0033: Localization

- Status: accepted (revised 2026-10-12: texts by key, in YAML)
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
- **Locales are YAML files** in `crates/app/locales`, built into the program: `en.yaml` has
  every key and is the reference; `ru.yaml` translates them. A key a language lacks shows in
  English. Arguments are named (`{name}`), so a translation may put them in any order.
- **Plural forms** are a mapping of CLDR's forms (`one`, `few`, `many`, `other`) with
  `{count}`; `plural.<noun>` holds the counted nouns of the find panel and the search results.
- **Data gets keys from its ids:** command titles are `command.<id>` (menus without their own
  label, the Shortcut Mapper), menu labels `menu.<label>`, panels `panel.<id>`, Preferences
  `preferences.<label>`; the English in the code stays as the fallback.
- **The crates without a window speak English** (their errors are part of their API and their
  tests). A locale's `text` section translates their sentences, English to translation, with
  `{}` for any part; a sentence inside another (an error's cause) is translated too.
- **Tests keep the locales whole:** every key of `en.yaml` is in every locale with the same
  arguments, every `t!` key of the code is in `en.yaml`, every command, menu label and panel has
  its text.
- **`appearance.language`:** `auto` (the default) follows the system's language
  (`sys-locale`), `en` and `ru` choose one; Preferences > Appearance changes it at once.

## Consequences

- A new text needs a key in `en.yaml` (the test says which) and, to be translated, in the other
  locales (the test says which too).
- Names from files (themes, macros, languages, scripts, plugins) and the names of encodings
  and languages are shown as they are.
- More languages are one YAML file each, with no code to change but the list of languages.
