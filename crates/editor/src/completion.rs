//! Notepad++'s auto-completion (ADR 0031): brackets and quotes typed in pairs, a list of the
//! document's words and the language's API keywords while typing, and a call tip with a
//! function's parameters after its opening bracket. API files are Notepad++'s, read from the
//! `autoCompletion` folder next to the settings.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use birchpad_config::{ApiFile, CompletionSettings};
use birchpad_core::completion::{WordOptions, is_word_char, word_candidates_with, word_start};
use birchpad_core::ops::{Pairs, close_tags, delete_pairs, type_with_pairs};
use gpui_kit::{
    AnyElement, App, Context, MouseButton, Window, anchored, deferred, div, point, prelude::*, px,
};

use super::{EditorView, LastEdit, theme};
use birchpad_ui::AppState;
use birchpad_ui::CommandRegistry;

use crate::EditorCommands as _;

/// The most words of the document the list shows.
const WORD_LIMIT: usize = 100;
/// Rows of the list shown at once.
const VISIBLE_ROWS: usize = 10;

/// An entry of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// A function's signature, from the API file.
    pub detail: Option<String>,
}

/// The completion list, for the word that starts at `start` and ends at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub start: usize,
    pub items: Vec<Item>,
    pub selected: usize,
}

/// A function's parameters, shown from its opening bracket at `open`: one overload at a time,
/// Up and Down going through them, as in Notepad++.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTip {
    pub open: usize,
    /// Each overload's signature, with its description on the lines below.
    pub overloads: Vec<String>,
    pub current: usize,
}

impl CallTip {
    /// What the tip shows: the overload, with `1 of 3` before it when there are several.
    pub fn text(&self) -> String {
        let overload = self
            .overloads
            .get(self.current)
            .cloned()
            .unwrap_or_default();
        if self.overloads.len() > 1 {
            format!(
                "\u{25B2} {} of {} \u{25BC} {overload}",
                self.current + 1,
                self.overloads.len()
            )
        } else {
            overload
        }
    }

    /// Goes `step` overloads on, around the ends.
    pub fn cycle(&mut self, step: isize) {
        let count = self.overloads.len() as isize;
        if count > 0 {
            self.current = (self.current as isize + step).rem_euclid(count) as usize;
        }
    }
}

/// Whether Auto-Completion closes the tags of the language `id`, and whether it is HTML (with
/// elements that have no closing tag).
fn tag_language(id: &str) -> Option<bool> {
    match id {
        "html" | "php" | "asp" | "vue" | "svelte" => Some(true),
        "xml" | "svg" | "xsl" | "xslt" => Some(false),
        _ => None,
    }
}

thread_local! {
    /// API files read so far, by language id; `None` when a language has none.
    static API_FILES: RefCell<HashMap<&'static str, Option<Rc<ApiFile>>>> = RefCell::default();
}

/// Birchpad's own API files, written for it (Notepad++'s are under the GPL), by language id.
const BUNDLED: &[(&str, &str)] = &[
    ("bash", include_str!("../api/bash.xml")),
    ("c", include_str!("../api/c.xml")),
    ("cpp", include_str!("../api/cpp.xml")),
    ("cs", include_str!("../api/cs.xml")),
    ("css", include_str!("../api/css.xml")),
    ("go", include_str!("../api/go.xml")),
    ("html", include_str!("../api/html.xml")),
    ("java", include_str!("../api/java.xml")),
    ("javascript", include_str!("../api/javascript.xml")),
    ("php", include_str!("../api/php.xml")),
    ("powershell", include_str!("../api/powershell.xml")),
    ("python", include_str!("../api/python.xml")),
    ("rust", include_str!("../api/rust.xml")),
    ("sql", include_str!("../api/sql.xml")),
    ("typescript", include_str!("../api/typescript.xml")),
    ("tsx", include_str!("../api/typescript.xml")),
];

/// The bundled API file of `language`, if Birchpad has one.
pub(crate) fn bundled_api(language: &str) -> Option<ApiFile> {
    let (_, xml) = BUNDLED.iter().find(|(id, _)| *id == language)?;
    ApiFile::parse(xml).ok()
}

/// The API file of `language`: `autoCompletion/<id>.xml` next to the settings, or else
/// Birchpad's own.
pub(crate) fn api_file(language: Option<&'static str>, cx: &App) -> Option<Rc<ApiFile>> {
    let language = language?;
    if let Some(found) = API_FILES.with(|files| files.borrow().get(language).cloned()) {
        return found;
    }
    let user = AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("autoCompletion").join(format!("{language}.xml")))
        .and_then(|path| std::fs::read_to_string(&path).ok().map(|text| (path, text)));
    let file = match user {
        Some((path, text)) => match ApiFile::parse(&text) {
            Ok(api) => Some(Rc::new(api)),
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                None
            }
        },
        None => bundled_api(language).map(Rc::new),
    };
    API_FILES.with(|files| files.borrow_mut().insert(language, file.clone()));
    file
}

/// Forgets the API files read, so that changed ones are read again.
#[cfg(test)]
pub(crate) fn forget_api_files() {
    API_FILES.with(|files| files.borrow_mut().clear());
}

/// Which entries the list offers (Auto-Completion's options).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sources {
    pub(crate) api: bool,
    pub(crate) words: bool,
    pub(crate) options: WordOptions,
}

impl Sources {
    /// What the settings offer for a word of `typed` characters.
    pub(crate) fn for_typed(settings: &CompletionSettings, typed: usize) -> Self {
        Self {
            api: settings.api && typed >= usize::from(settings.api_min_chars).max(1),
            words: settings.words && typed >= usize::from(settings.min_chars).max(1),
            options: WordOptions {
                ignore_case: settings.ignore_case,
                ignore_numbers: settings.ignore_numbers,
            },
        }
    }

    /// Everything, as Ctrl+Space asks.
    pub(crate) fn all(settings: &CompletionSettings) -> Self {
        Self {
            api: true,
            words: true,
            ..Self::for_typed(settings, 0)
        }
    }
}

/// The list for the word `prefix` (at `start..pos`): the API's keywords, then the document's
/// words, each once, as far as `sources` offer them.
pub(crate) fn items(
    api: Option<&ApiFile>,
    text: &birchpad_core::Rope,
    start: usize,
    pos: usize,
    sources: Sources,
) -> Vec<Item> {
    let prefix = text.slice(start..pos).to_string();
    let api = api.filter(|_| sources.api);
    let mut items: Vec<Item> = api
        .map(|api| {
            api.completions(&prefix)
                .into_iter()
                .filter(|keyword| {
                    let number = keyword.name.starts_with(|ch: char| ch.is_ascii_digit());
                    !(sources.options.ignore_numbers && number)
                        && (sources.options.ignore_case || keyword.name.starts_with(&prefix))
                })
                .map(|keyword| Item {
                    label: keyword.name.clone(),
                    detail: keyword
                        .overloads
                        .first()
                        .map(|overload| overload.signature(&keyword.name)),
                })
                .collect()
        })
        .unwrap_or_default();
    let words = if sources.words {
        word_candidates_with(text, &prefix, start..pos, WORD_LIMIT, sources.options)
    } else {
        Vec::new()
    };
    for word in words {
        if !items.iter().any(|item| item.label == word) {
            items.push(Item {
                label: word,
                detail: None,
            });
        }
    }
    items
}

impl EditorView {
    fn pairs(cx: &App) -> Pairs {
        Pairs(AppState::global(cx).settings.completion.pairs())
    }

    /// The language at the caret: one embedded in the document's (a script in HTML) has its
    /// own API file.
    fn language_id(&self, cx: &App) -> Option<&'static str> {
        let head = self.selection.primary().head;
        self.buffer
            .read(cx)
            .language_at(head)
            .map(|language| language.id)
    }

    /// After `>` typed in HTML or XML: the closing tag of the opening tag it ended.
    fn close_tag(&mut self, cx: &mut Context<Self>) {
        let Some(html) = self.language_id(cx).and_then(tag_language) else {
            return;
        };
        let text = self.text(cx).clone();
        if let Some(transaction) = close_tags(&text, &self.selection, html) {
            self.apply(transaction, LastEdit::None, cx);
        }
    }

    /// Typing `typed` with auto-closing: true when it paired or stepped over a partner.
    pub(crate) fn type_paired(&mut self, typed: &str, cx: &mut Context<Self>) -> bool {
        let mut chars = typed.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            return false;
        };
        if self.overwrite || !self.is_editable(cx) {
            return false;
        }
        let pairs = Self::pairs(cx);
        let text = self.text(cx).clone();
        match type_with_pairs(&text, &self.selection, ch, &pairs) {
            Some(transaction) => {
                self.apply(transaction, LastEdit::Typing, cx);
                true
            }
            None => false,
        }
    }

    /// Backspace between an empty pair: true when it deleted both.
    pub(crate) fn backspace_pair(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_editable(cx) {
            return false;
        }
        let pairs = Self::pairs(cx);
        let text = self.text(cx).clone();
        match delete_pairs(&text, &self.selection, &pairs) {
            Some(transaction) => {
                self.apply(transaction, LastEdit::None, cx);
                true
            }
            None => false,
        }
    }

    /// After text was typed: the list and the call tip follow it.
    pub(crate) fn after_typing(&mut self, typed: &str, cx: &mut Context<Self>) {
        let settings = AppState::global(cx).settings.completion.clone();
        let api = api_file(self.language_id(cx), cx);
        let head = self.selection.primary().head;
        if let Some(tip) = &self.call_tip {
            let closes = api
                .as_ref()
                .is_some_and(|api| typed.contains(api.stop_func));
            if closes || head <= tip.open {
                self.call_tip = None;
            }
        }
        if settings.call_tips
            && let Some(api) = &api
            && typed.ends_with(api.start_func)
        {
            self.show_call_tip(api, cx);
        }
        if settings.close_tags && typed == ">" {
            self.close_tag(cx);
        }
        let word_typed = typed.chars().last().is_some_and(is_word_char);
        if word_typed && (settings.words || settings.api || self.completion.is_some()) {
            self.update_completion(false, cx);
        } else {
            self.completion = None;
        }
        cx.notify();
    }

    /// Shows or refreshes the list for the word before the caret: what the settings offer for
    /// its length, or everything when `asked` (Ctrl+Space).
    pub fn update_completion(&mut self, asked: bool, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let head = self.selection.primary().head;
        let start = word_start(&text, head);
        let typed = text.slice(start..head).chars().count();
        let settings = &AppState::global(cx).settings.completion;
        let sources = if asked {
            Sources::all(settings)
        } else {
            Sources::for_typed(settings, typed)
        };
        if typed == 0 || !(sources.api || sources.words) {
            self.completion = None;
            return;
        }
        let api = api_file(self.language_id(cx), cx);
        let items = items(api.as_deref(), &text, start, head, sources);
        if items.is_empty() {
            self.completion = None;
            return;
        }
        let selected = self
            .completion
            .as_ref()
            .and_then(|popup| popup.items.get(popup.selected))
            .and_then(|current| items.iter().position(|item| item.label == current.label))
            .unwrap_or(0);
        self.completion = Some(Popup {
            start,
            items,
            selected,
        });
        cx.notify();
    }

    /// The function before an opening bracket at the caret, with its parameters.
    fn show_call_tip(&mut self, api: &ApiFile, cx: &mut Context<Self>) {
        let text = self.text(cx).clone();
        let head = self.selection.primary().head;
        let open = head.saturating_sub(api.start_func.len_utf8());
        let name_start = word_start(&text, open);
        let name = text.slice(name_start..open).to_string();
        if let Some(function) = api.function(&name) {
            let overloads: Vec<String> = function
                .overloads
                .iter()
                .map(|overload| {
                    let signature = overload.signature(&function.name);
                    if overload.description.is_empty() {
                        signature
                    } else {
                        format!("{signature}\n{}", overload.description)
                    }
                })
                .collect();
            self.call_tip = Some(CallTip {
                open: head,
                overloads,
                current: 0,
            });
        }
    }

    /// Offers `choices` for the text of `range` (a misspelled word) in the completion list,
    /// with the caret at its end; the one taken replaces it.
    pub fn show_choices(
        &mut self,
        range: std::ops::Range<usize>,
        choices: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.go_to(range.end, cx);
        self.call_tip = None;
        self.completion = Some(Popup {
            start: range.start,
            items: choices
                .into_iter()
                .map(|label| Item {
                    label,
                    detail: None,
                })
                .collect(),
            selected: 0,
        });
        cx.notify();
    }

    /// Takes the selected entry in place of the word typed.
    pub fn accept_completion(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.completion.take() else {
            return;
        };
        let Some(item) = popup.items.get(popup.selected) else {
            return;
        };
        let head = self.selection.primary().head;
        if head < popup.start {
            return;
        }
        self.replace_range(popup.start..head, &item.label, cx);
        cx.notify();
    }

    /// Closes the list and the call tip when the caret goes elsewhere: any command but typing,
    /// deleting and the list's own.
    pub(crate) fn close_completion_for(&mut self, command: &str) {
        let keeps =
            command.starts_with("completion.") || matches!(command, "edit.type" | "edit.backspace");
        if !keeps {
            self.completion = None;
            self.call_tip = None;
        }
    }

    /// The key context of the editor: the list and the call tip take some keys.
    pub(crate) fn key_context(&self) -> &'static str {
        match (&self.completion, &self.call_tip) {
            (Some(_), _) => "Editor Completion",
            (None, Some(_)) => "Editor CallTip",
            (None, None) => "Editor",
        }
    }

    /// The list or the call tip, drawn below the caret.
    pub(crate) fn render_completion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.completion.is_none() && self.call_tip.is_none() {
            return None;
        }
        let layout = self.layout.as_ref()?;
        let head = self.selection.primary().head;
        let caret = layout.bounds_for_range(head..head)?;
        let colors = theme::ui();
        let below = point(caret.left(), caret.bottom() + px(2.));
        let content = if let Some(popup) = &self.completion {
            let first = popup.selected.saturating_sub(VISIBLE_ROWS - 1);
            let rows = popup
                .items
                .iter()
                .enumerate()
                .skip(first)
                .take(VISIBLE_ROWS)
                .map(|(index, item)| {
                    div()
                        .id(("completion-item", index))
                        .px_2()
                        .flex()
                        .flex_row()
                        .gap_3()
                        .when(index == popup.selected, |row| {
                            row.bg(theme::paint(colors.selected))
                        })
                        .child(item.label.clone())
                        .children(item.detail.clone().map(|detail| {
                            div().text_color(theme::paint(colors.muted)).child(detail)
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(popup) = &mut this.completion {
                                    popup.selected = index;
                                }
                                this.accept_completion(cx);
                                cx.stop_propagation();
                            }),
                        )
                });
            div()
                .debug_selector(|| "completion".into())
                .min_w(px(160.))
                .py_1()
                .children(rows)
                .into_any_element()
        } else {
            let tip = self.call_tip.as_ref()?;
            div()
                .debug_selector(|| "call-tip".into())
                .px_2()
                .py_1()
                .max_w(px(600.))
                .children(tip.text().lines().map(|line| div().child(line.to_owned())))
                .into_any_element()
        };
        Some(
            deferred(
                anchored().position(below).snap_to_window().child(
                    div()
                        .occlude()
                        .bg(theme::paint(colors.background))
                        .border_1()
                        .border_color(theme::paint(colors.border))
                        .rounded(px(4.))
                        .shadow_md()
                        .text_sm()
                        .text_color(theme::paint(colors.text))
                        .child(content),
                ),
            )
            .into_any_element(),
        )
    }
}

fn move_selection(view: &mut EditorView, step: isize) {
    if let Some(popup) = &mut view.completion {
        let count = popup.items.len() as isize;
        popup.selected = (popup.selected as isize + step).rem_euclid(count) as usize;
    }
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("completion.complete", |this, (), _, cx| {
        this.update_completion(true, cx);
        Ok(())
    });
    registry.editor("completion.next-overload", |this, (), _, cx| {
        if let Some(tip) = &mut this.call_tip {
            tip.cycle(1);
        }
        cx.notify();
        Ok(())
    });
    registry.editor("completion.previous-overload", |this, (), _, cx| {
        if let Some(tip) = &mut this.call_tip {
            tip.cycle(-1);
        }
        cx.notify();
        Ok(())
    });
    registry.editor(
        "completion.parameter-hint",
        |this, (), _: &mut Window, cx| {
            let api = api_file(this.language_id(cx), cx);
            let Some(api) = api else {
                return Ok(());
            };
            // The call the caret is in: its last unclosed opening bracket.
            let text = this.text(cx).clone();
            let head = this.selection.primary().head;
            let line_start = birchpad_core::motion::line_range(
                &text,
                birchpad_core::motion::line_of(&text, head),
            )
            .start;
            let before = text.slice(line_start..head).to_string();
            let mut depth = 0;
            for (offset, ch) in before.char_indices().rev() {
                if ch == api.stop_func {
                    depth += 1;
                } else if ch == api.start_func {
                    if depth == 0 {
                        let saved = this.selection.clone();
                        this.selection =
                            birchpad_core::Selection::point(line_start + offset + ch.len_utf8());
                        this.show_call_tip(&api, cx);
                        this.selection = saved;
                        break;
                    }
                    depth -= 1;
                }
            }
            cx.notify();
            Ok(())
        },
    );
    registry.editor("completion.next", |this, (), _, cx| {
        move_selection(this, 1);
        cx.notify();
        Ok(())
    });
    registry.editor("completion.previous", |this, (), _, cx| {
        move_selection(this, -1);
        cx.notify();
        Ok(())
    });
    registry.editor("completion.accept", |this, (), _, cx| {
        this.accept_completion(cx);
        Ok(())
    });
    registry.editor("completion.cancel", |this, (), _, cx| {
        this.completion = None;
        this.call_tip = None;
        cx.notify();
        Ok(())
    });
}

#[cfg(test)]
#[path = "../tests/unit/completion/tests.rs"]
mod tests;
