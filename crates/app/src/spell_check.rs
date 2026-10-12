//! Plugins > Spell Checker (ADR 0039), as Notepad++'s DSpellCheck: misspelled words
//! underlined while Spell Check Document Automatically is on, the next and the previous one,
//! suggestions in the completion list, the user's own words, and the dictionary's language.
//!
//! Dictionaries are Hunspell's `.aff` and `.dic`, from the `dictionaries` folder next to the
//! settings, then the system's folders. In a document with a language, only comments and
//! strings are checked.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_commands::Invocation;
use birchpad_core::{RevisionId, Rope};
use birchpad_tools::spell::{self, Speller};
use gpui_kit::{App, Context, EntityId};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::CommandRegistry;
use crate::editor::EditorView;

/// The file of the user's own words, in the dictionaries folder.
const USER_WORDS: &str = "user.txt";

/// A dictionary read, or why it could not be.
type Loaded = Result<Rc<RefCell<Speller>>, String>;

/// The dictionary in use, the words found in the frames drawn last.
#[derive(Default)]
struct Spelling {
    /// The language loaded, and its speller (or why it did not load).
    loaded: Option<(String, Loaded)>,
    /// Misspelled words of each document's last checked range.
    cache: HashMap<EntityId, Cached>,
    /// Changes when the dictionary does: words added, another language.
    generation: u64,
}

struct Cached {
    revision: RevisionId,
    /// The syntax tree's version: comments and strings move as it is parsed.
    syntax: Option<u64>,
    range: Range<usize>,
    generation: u64,
    words: Rc<Vec<Range<usize>>>,
}

thread_local! {
    static SPELLING: RefCell<Spelling> = RefCell::default();
}

/// The folders dictionaries are looked for in, the user's first.
pub(crate) fn folders(cx: &App) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = dictionaries_dir(cx).into_iter().collect();
    folders.extend(spell::system_folders());
    folders
}

fn dictionaries_dir(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("dictionaries"))
}

/// The language checked: the one chosen, or the system's (`en_US`).
pub(crate) fn language(cx: &App) -> String {
    AppState::global(cx)
        .state
        .spell_check
        .language
        .clone()
        .unwrap_or_else(|| {
            sys_locale::get_locale()
                .map(|locale| locale.replace('-', "_"))
                .filter(|locale| locale.contains('_'))
                .unwrap_or_else(|| "en_US".to_owned())
        })
}

pub(crate) fn is_enabled(cx: &App) -> bool {
    AppState::global(cx).state.spell_check.enabled
}

/// The speller of the language checked, read once.
fn speller(cx: &App) -> Result<Rc<RefCell<Speller>>> {
    let language = language(cx);
    let cached = SPELLING.with(|spelling| {
        spelling
            .borrow()
            .loaded
            .as_ref()
            .filter(|(loaded, _)| *loaded == language)
            .map(|(_, speller)| speller.clone())
    });
    let result = match cached {
        Some(result) => result,
        None => {
            let result = load(&language, cx).map(|speller| Rc::new(RefCell::new(speller)));
            let result = result.map_err(|error| format!("{error:#}"));
            SPELLING.with(|spelling| {
                let mut spelling = spelling.borrow_mut();
                spelling.loaded = Some((language, result.clone()));
                spelling.generation += 1;
            });
            result
        }
    };
    result.map_err(|error| anyhow!(error))
}

fn load(language: &str, cx: &App) -> Result<Speller> {
    let folders = folders(cx);
    let Some((aff, dic)) = spell::find_dictionary(&folders, language) else {
        let user = dictionaries_dir(cx)
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        bail!(
            "{}",
            t!(
                "spell-check.there-is-no-dictionary-put-aff-and-dic-hunspell",
                language = language,
                user = user
            )
        );
    };
    let mut speller = Speller::open(&aff, &dic)?;
    if let Some(dir) = dictionaries_dir(cx)
        && let Ok(words) = std::fs::read_to_string(dir.join(USER_WORDS))
    {
        for word in words.lines() {
            speller.add(word);
        }
    }
    Ok(speller)
}

/// Reads the dictionary again (another language, a new file).
fn forget(cx: &mut App) {
    SPELLING.with(|spelling| {
        let mut spelling = spelling.borrow_mut();
        spelling.loaded = None;
        spelling.cache.clear();
        spelling.generation += 1;
    });
    redraw(cx);
}

fn redraw(cx: &mut App) {
    cx.refresh_windows();
}

/// The parts of `range` worth checking: comments and strings in a document with a language,
/// all of it otherwise.
fn checkable(buffer: &Buffer, text: &Rope, range: Range<usize>) -> Vec<Range<usize>> {
    let Some(syntax) = buffer.syntax() else {
        return vec![range];
    };
    let mut parts: Vec<Range<usize>> = Vec::new();
    for (span, highlight) in syntax.highlights(text, range) {
        let name = highlight.name();
        if !(name.starts_with("comment") || name.starts_with("string")) {
            continue;
        }
        match parts.last_mut() {
            Some(last) if last.end >= span.start => last.end = last.end.max(span.end),
            _ => parts.push(span),
        }
    }
    parts
}

/// The misspelled words in `range`.
fn find_misspelled(
    speller: &Speller,
    buffer: &Buffer,
    text: &Rope,
    range: Range<usize>,
) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    for part in checkable(buffer, text, range) {
        let piece = text.slice(part.clone()).to_string();
        found.extend(
            speller
                .misspelled(&piece)
                .into_iter()
                .map(|word| part.start + word.start..part.start + word.end),
        );
    }
    found
}

/// The misspelled words in `range` of `buffer`'s text, to underline: none when spell checking
/// is off or has no dictionary.
pub(crate) fn underlined(
    buffer: &Buffer,
    id: EntityId,
    range: Range<usize>,
    cx: &App,
) -> Rc<Vec<Range<usize>>> {
    if !is_enabled(cx) {
        return Rc::default();
    }
    let Ok(speller) = speller(cx) else {
        return Rc::default();
    };
    let revision = buffer.doc().revision();
    let syntax = buffer.syntax().map(|syntax| syntax.version());
    let generation = SPELLING.with(|spelling| spelling.borrow().generation);
    let cached = SPELLING.with(|spelling| {
        spelling.borrow().cache.get(&id).and_then(|cached| {
            (cached.revision == revision
                && cached.syntax == syntax
                && cached.range == range
                && cached.generation == generation)
                .then(|| cached.words.clone())
        })
    });
    if let Some(words) = cached {
        return words;
    }
    let words = Rc::new(find_misspelled(
        &speller.borrow(),
        buffer,
        buffer.doc().text(),
        range.clone(),
    ));
    SPELLING.with(|spelling| {
        spelling.borrow_mut().cache.insert(
            id,
            Cached {
                revision,
                syntax,
                range,
                generation,
                words: words.clone(),
            },
        );
    });
    words
}

#[derive(Deserialize)]
struct LanguageArgs {
    language: String,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("spell.auto", |this, (), _, cx| {
        let on = !is_enabled(cx);
        if on {
            speller(cx)?;
        }
        AppState::update_state(cx, |state, _| state.spell_check.enabled = on);
        redraw(cx);
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("spell.language", |this, args: LanguageArgs, _, cx| {
        let language = args.language.trim().to_owned();
        if language.is_empty() || language.contains(['/', '\\', '.']) {
            bail!(
                "{}",
                t!(
                    "spell-check.is-not-a-dictionary-s-language",
                    language = format!("{:?}", args.language)
                )
            );
        }
        AppState::update_state(cx, |state, _| state.spell_check.language = Some(language));
        forget(cx);
        this.refresh_menus(cx);
        speller(cx).map(|_| ())
    });
    registry.workspace("spell.open-dictionaries-folder", |_, (), _, cx| {
        let dir = dictionaries_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("spell-check.there-is-no-settings-folder")))?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        cx.open_with_system(&dir);
        forget(cx);
        Ok(())
    });
    registry.editor("spell.next", |this, (), _, cx| {
        go_to_misspelled(this, true, cx)
    });
    registry.editor("spell.previous", |this, (), _, cx| {
        go_to_misspelled(this, false, cx)
    });
    registry.editor("spell.suggestions", |this, (), _, cx| {
        let speller = speller(cx)?;
        let (range, word) = word_at_caret(this, cx)?;
        let speller = speller.borrow();
        if speller.check(&word) {
            bail!("{}", t!("spell-check.is-spelled-right", word = word));
        }
        let suggestions = speller.suggest(&word);
        if suggestions.is_empty() {
            bail!(
                "{}",
                t!("spell-check.there-are-no-suggestions-for", word = word)
            );
        }
        this.show_choices(range, suggestions, cx);
        Ok(())
    });
    registry.editor("spell.add-word", |this, (), _, cx| {
        let speller = speller(cx)?;
        let (_, word) = word_at_caret(this, cx)?;
        let dir = dictionaries_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("spell-check.there-is-no-settings-folder")))?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        let path = dir.join(USER_WORDS);
        let mut words = std::fs::read_to_string(&path).unwrap_or_default();
        if !words.is_empty() && !words.ends_with('\n') {
            words.push('\n');
        }
        words.push_str(&word);
        words.push('\n');
        std::fs::write(&path, words).with_context(|| format!("cannot write {}", path.display()))?;
        speller.borrow_mut().add(&word);
        SPELLING.with(|spelling| {
            let mut spelling = spelling.borrow_mut();
            spelling.generation += 1;
            spelling.cache.clear();
        });
        redraw(cx);
        Ok(())
    });
}

/// Whether a Spell Checker item shows a check mark.
pub(crate) fn is_checked(invocation: &Invocation, enabled: bool, language: &str) -> Option<bool> {
    match invocation.command.as_str() {
        "spell.auto" => Some(enabled),
        "spell.language" => Some(
            invocation
                .args
                .get("language")
                .and_then(|value| value.as_str())
                == Some(language),
        ),
        _ => None,
    }
}

/// The languages of the dictionaries found, for the Language submenu.
pub(crate) fn languages(cx: &App) -> Vec<String> {
    spell::languages(&folders(cx))
}

/// The word at the caret (or touching it), and its range.
fn word_at_caret(view: &EditorView, cx: &App) -> Result<(Range<usize>, String)> {
    let text = view.text(cx);
    let head = view.selection.primary().head;
    let line = birchpad_core::motion::line_of(text, head);
    let line_range = birchpad_core::motion::line_range(text, line);
    let line_text = text.slice(line_range.clone()).to_string();
    let at = head - line_range.start;
    spell::words(&line_text)
        .find(|word| word.start <= at && at <= word.end)
        .map(|word| {
            let range = line_range.start + word.start..line_range.start + word.end;
            (range, line_text[word].to_owned())
        })
        .ok_or_else(|| anyhow!("{}", t!("spell-check.there-is-no-word-at-the-caret")))
}

/// Selects the next (or previous) misspelled word, wrapping around.
fn go_to_misspelled(
    view: &mut EditorView,
    forward: bool,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    let speller = speller(cx)?;
    let text = view.text(cx).clone();
    let words = find_misspelled(
        &speller.borrow(),
        view.buffer.read(cx),
        &text,
        0..text.len(),
    );
    let primary = view.selection.primary();
    let target = if forward {
        words
            .iter()
            .find(|word| word.start >= primary.to() && word.start != primary.from())
            .or(words.first())
    } else {
        words
            .iter()
            .rev()
            .find(|word| word.end <= primary.from() && word.start != primary.from())
            .or(words.last())
    };
    let target =
        target.ok_or_else(|| anyhow!("{}", t!("spell-check.there-are-no-misspelled-words")))?;
    view.select_range(target.clone(), cx);
    Ok(())
}

#[cfg(test)]
mod tests;
