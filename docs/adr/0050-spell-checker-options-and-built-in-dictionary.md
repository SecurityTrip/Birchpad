# ADR 0050: Spell checker options and a built-in dictionary

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0039](0039-built-in-tools.md): the spell checker gets DSpellCheck's options and
  a dictionary of its own.

## Context

ADR 0039's spell checker reads Hunspell dictionaries the user or the system has, one language
at a time, with fixed rules for which words to leave alone, and in code only comments and
strings. DSpellCheck lets users check several languages at once, choose those rules, and
choose by syntax style what of code is checked. Most users have no dictionary at all.
Built from DSpellCheck's documentation and options as a user sees them.

## Decision

- **Settings of their own** (`spell` in `settings.toml`, Preferences > Spell Checker):
  `languages` (checked together: a word is right if one of the dictionaries has it; empty
  means the language chosen in the menu), `check` (`comments-and-strings`, `comments`,
  `strings` or `everything`: what of a document with a language, by syntax style), and the
  rules: words with digits, in capitals, with a capital inside, starting with a capital, with
  underscores, single letters, and addresses.
- **A speller holds several dictionaries**; suggestions come from each in turn, without
  repeats.
- **US English is built in**: the Hunspell `en_US` dictionary made from SCOWL (release
  2020.12.07), whose license permits redistribution with its notices, kept unchanged in
  `crates/app/assets/dictionaries` with them. A dictionary of the user or the system with the
  same language comes first.

## Consequences

- Birchpad checks English without anything installed; about 550 KB more in the program.
- Other languages still come from the user's or the system's dictionaries, or from the
  download of roadmap 6.47.
