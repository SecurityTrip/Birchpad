//! Language > User Defined Language > Define your language...: Notepad++'s dialog that writes a
//! user defined language: its name and extensions, keywords, comments, operators, delimiters,
//! folders and numbers. The language's colors are not edited here; where it has none, the
//! theme's apply (ADR 0032).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_syntax::{DELIMITERS, LISTS, LineCommentAt, UdlDefinition, parse_udl, udl_file};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Selectable as _, Sizable as _};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px,
};
use serde::Deserialize;

use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// `{ "name": "..." }`: the language to edit; a new one without it, unless the active document
/// has one.
#[derive(Deserialize, Default)]
pub(crate) struct DefineArgs {
    #[serde(default)]
    pub(crate) name: Option<String>,
}

/// The file name a new language is written to: its name without the characters file systems
/// refuse.
pub(crate) fn file_name(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_control() || r#"<>:"/\|?*"#.contains(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let safe = safe.trim_matches(['.', ' ']);
    if safe.is_empty() {
        "language.xml".to_owned()
    } else {
        format!("{safe}.xml")
    }
}

/// The languages of each UDL file of `dir`, as written.
fn read_definitions(dir: &Path) -> Vec<(PathBuf, Vec<UdlDefinition>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let languages = UdlDefinition::parse_all(&text).ok()?;
            Some((path, languages))
        })
        .collect()
}

/// The language named `name` in the UDL files of `dir`, as written.
pub(crate) fn find_definition(dir: &Path, name: &str) -> Option<UdlDefinition> {
    read_definitions(dir)
        .into_iter()
        .flat_map(|(_, languages)| languages)
        .find(|language| language.name == name)
}

/// Writes `language` into `dir`, in place of the language named `replacing` (or of one of the
/// same name): in its file if it was alone there, else in a file of its own, the others of its
/// old file written back without it. The path written to.
pub(crate) fn save_definition(
    dir: &Path,
    replacing: Option<&str>,
    language: &UdlDefinition,
) -> Result<PathBuf> {
    let name = language.name.trim();
    if name.is_empty() {
        bail!("{}", t!("user-languages.define.the-language-needs-a-name"));
    }
    let xml = udl_file(std::slice::from_ref(language));
    parse_udl(&xml).map_err(|error| anyhow!("{error}"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let replaced = |found: &UdlDefinition| {
        found.name == name || replacing.is_some_and(|old| found.name == old)
    };
    let mut target = None;
    for (path, languages) in read_definitions(dir) {
        if !languages.iter().any(replaced) {
            continue;
        }
        let others: Vec<UdlDefinition> = languages
            .into_iter()
            .filter(|found| !replaced(found))
            .collect();
        if others.is_empty() {
            if target.is_none() {
                target = Some(path);
            } else {
                std::fs::remove_file(&path)
                    .with_context(|| format!("cannot remove {}", path.display()))?;
            }
        } else {
            std::fs::write(&path, udl_file(&others))
                .with_context(|| format!("cannot write {}", path.display()))?;
        }
    }
    let target = target.unwrap_or_else(|| {
        let mut path = dir.join(file_name(name));
        let mut number = 2;
        while path.exists() {
            let stem = file_name(name);
            let stem = stem.trim_end_matches(".xml");
            path = dir.join(format!("{stem} {number}.xml"));
            number += 1;
        }
        path
    });
    std::fs::write(&target, xml).with_context(|| format!("cannot write {}", target.display()))?;
    Ok(target)
}

/// The Define your language dialog.
pub(crate) struct DefineDialog {
    workspace: WeakEntity<Workspace>,
    /// The language as it was, with what the dialog does not edit.
    base: UdlDefinition,
    /// The name it had, for a language being edited.
    original: Option<String>,
    name: Entity<InputState>,
    extensions: Entity<InputState>,
    ignore_case: bool,
    line_comment_at: LineCommentAt,
    prefix: [bool; 8],
    lists: Vec<(&'static str, Entity<InputState>)>,
    line_comment: Entity<InputState>,
    block_open: Entity<InputState>,
    block_close: Entity<InputState>,
    delimiters: Vec<[Entity<InputState>; 3]>,
    pub(crate) error: Option<String>,
}

impl DefineDialog {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        base: Option<UdlDefinition>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let original = base.as_ref().map(|base| base.name.clone());
        let base = base.unwrap_or_default();
        let mut input = |value: &str, cx: &mut Context<Self>| {
            let value = value.to_owned();
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let name = input(&base.name, cx);
        let extensions = input(&base.extensions, cx);
        let lists = LISTS
            .iter()
            .map(|list| (*list, input(base.list(list), cx)))
            .collect();
        let (line, open, close) = base.comments();
        let line_comment = input(&line, cx);
        let block_open = input(&open, cx);
        let block_close = input(&close, cx);
        let delimiters = (0..DELIMITERS)
            .map(|index| {
                let (open, escape, close) = base.delimiter(index);
                [input(&open, cx), input(&escape, cx), input(&close, cx)]
            })
            .collect();
        Self {
            workspace,
            ignore_case: base.ignore_case,
            line_comment_at: base.line_comment_at,
            prefix: base.prefix,
            base,
            original,
            name,
            extensions,
            lists,
            line_comment,
            block_open,
            block_close,
            delimiters,
            error: None,
        }
    }

    /// The language as the dialog shows it.
    pub(crate) fn definition(&self, cx: &App) -> UdlDefinition {
        let value = |input: &Entity<InputState>| input.read(cx).value().to_string();
        let mut language = self.base.clone();
        language.name = value(&self.name).trim().to_owned();
        language.extensions = value(&self.extensions).trim().to_owned();
        language.ignore_case = self.ignore_case;
        language.line_comment_at = self.line_comment_at;
        language.prefix = self.prefix;
        for (list, input) in &self.lists {
            language.set_list(list, &value(input));
        }
        language.set_comments(
            &value(&self.line_comment),
            &value(&self.block_open),
            &value(&self.block_close),
        );
        for (index, [open, escape, close]) in self.delimiters.iter().enumerate() {
            language.set_delimiter(index as u8, &value(open), &value(escape), &value(close));
        }
        language
    }

    /// Sets a field by its list name, or `name`, `extensions`, `line-comment`, `block-open`,
    /// `block-close`, `delimiter-N-open|escape|close` (for tests and scripts).
    #[cfg(test)]
    pub(crate) fn set_field(
        &mut self,
        field: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = match field {
            "name" => self.name.clone(),
            "extensions" => self.extensions.clone(),
            "line-comment" => self.line_comment.clone(),
            "block-open" => self.block_open.clone(),
            "block-close" => self.block_close.clone(),
            _ => {
                if let Some(rest) = field.strip_prefix("delimiter-") {
                    let (index, part) = rest.split_once('-').expect("delimiter-N-part");
                    let index: usize = index.parse().expect("a number");
                    let part = ["open", "escape", "close"]
                        .iter()
                        .position(|known| *known == part)
                        .expect("a part");
                    self.delimiters[index][part].clone()
                } else {
                    self.lists
                        .iter()
                        .find(|(list, _)| *list == field)
                        .map(|(_, input)| input.clone())
                        .expect("a list")
                }
            }
        };
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    }

    /// Writes the language and loads it; false keeps the dialog open to show what went wrong.
    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let language = self.definition(cx);
        let result = super::folder(cx)
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!("user-languages.there-is-no-settings-folder-to-keep-languages-in")
                )
            })
            .and_then(|dir| save_definition(&dir, self.original.as_deref(), &language));
        match result {
            Ok(_) => {
                self.error = None;
                super::init(cx);
                let original = self.original.clone();
                self.workspace
                    .update(cx, |workspace, cx| {
                        workspace.user_language_saved(
                            original.as_deref(),
                            &language.name,
                            window,
                            cx,
                        );
                    })
                    .ok();
                self.original = Some(language.name.clone());
                self.base = language;
                cx.notify();
                true
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
                false
            }
        }
    }

    fn section(title: String) -> impl IntoElement {
        div().pt_2().text_color(paint(ui().heading)).child(title)
    }

    fn row(label: String, input: &Entity<InputState>, id: String) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(div().w(px(170.)).flex_none().child(label))
            .child(div().flex_1().child(Input::new(input).id(id).small()))
            .into_any_element()
    }

    fn render_keywords(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.lists
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, (list, input))| {
                let this = cx.entity().downgrade();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(170.))
                            .flex_none()
                            .child(t!("user-languages.define.keywords", number = index + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(input).id(format!("udl-{list}")).small()),
                    )
                    .child(
                        Checkbox::new(("udl-prefix", index))
                            .label(t!("user-languages.define.prefix"))
                            .checked(self.prefix[index])
                            .small()
                            .on_click(move |checked, _, cx| {
                                let checked = *checked;
                                this.update(cx, |this, cx| {
                                    this.prefix[index] = checked;
                                    cx.notify();
                                })
                                .ok();
                            }),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn render_line_comment_at(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let choice = |id: &'static str, label: String, at: LineCommentAt| {
            Button::new(id)
                .small()
                .ghost()
                .label(label)
                .selected(self.line_comment_at == at)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.line_comment_at = at;
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .w(px(170.))
                    .flex_none()
                    .child(t!("user-languages.define.line-comments-start")),
            )
            .child(choice(
                "udl-comment-anywhere",
                t!("user-languages.define.anywhere"),
                LineCommentAt::Anywhere,
            ))
            .child(choice(
                "udl-comment-indent",
                t!("user-languages.define.after-indentation"),
                LineCommentAt::LineStartOrIndent,
            ))
            .child(choice(
                "udl-comment-start",
                t!("user-languages.define.first-column"),
                LineCommentAt::LineStart,
            ))
    }
}

/// The label of a list in the dialog: `Folders in code1, open` reads as it is written in
/// UDL files, which users know from Notepad++.
fn list_label(list: &str) -> String {
    crate::i18n::get_or(
        &format!("user-languages.define.{}", crate::i18n::slug(list)),
        list,
    )
}

impl Render for DefineDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let lists = |from: usize, to: usize| {
            LISTS[from..to]
                .iter()
                .map(|list| {
                    let input = &self
                        .lists
                        .iter()
                        .find(|(known, _)| known == list)
                        .expect("every list has a field")
                        .1;
                    Self::row(list_label(list), input, format!("udl-{list}"))
                })
                .collect::<Vec<_>>()
        };
        let delimiters =
            self.delimiters
                .iter()
                .enumerate()
                .map(|(index, [open, escape, close])| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(170.))
                                .flex_none()
                                .child(t!("user-languages.define.delimiter", number = index + 1)),
                        )
                        .children([open, escape, close].into_iter().enumerate().map(
                            |(part, input)| {
                                div().flex_1().child(
                                    Input::new(input)
                                        .id(format!("udl-delimiter-{index}-{part}"))
                                        .small(),
                                )
                            },
                        ))
                        .into_any_element()
                })
                .collect::<Vec<_>>();
        div()
            .id("udl-define")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(560.))
            .overflow_y_scroll()
            .pr_2()
            .child(Self::row(
                t!("user-languages.define.name"),
                &self.name,
                "udl-name".into(),
            ))
            .child(Self::row(
                t!("user-languages.define.extensions"),
                &self.extensions,
                "udl-extensions".into(),
            ))
            .child(
                Checkbox::new("udl-ignore-case")
                    .label(t!("user-languages.define.ignore-case"))
                    .checked(self.ignore_case)
                    .small()
                    .on_click(move |checked, _, cx| {
                        let checked = *checked;
                        this.update(cx, |this, cx| {
                            this.ignore_case = checked;
                            cx.notify();
                        })
                        .ok();
                    }),
            )
            .child(Self::section(t!("user-languages.define.keyword-lists")))
            .children(self.render_keywords(cx))
            .child(Self::section(t!("user-languages.define.comments")))
            .child(Self::row(
                t!("user-languages.define.line-comment"),
                &self.line_comment,
                "udl-line-comment".into(),
            ))
            .child(self.render_line_comment_at(cx))
            .child(Self::row(
                t!("user-languages.define.block-comment-open"),
                &self.block_open,
                "udl-block-open".into(),
            ))
            .child(Self::row(
                t!("user-languages.define.block-comment-close"),
                &self.block_close,
                "udl-block-close".into(),
            ))
            .child(Self::section(t!(
                "user-languages.define.operators-and-delimiters"
            )))
            .children(lists(8, 10))
            .child(
                div()
                    .text_sm()
                    .text_color(paint(ui().muted))
                    .child(t!("user-languages.define.delimiter-parts")),
            )
            .children(delimiters)
            .child(Self::section(t!("user-languages.define.folding")))
            .children(lists(10, 19))
            .child(Self::section(t!("user-languages.define.numbers")))
            .children(lists(19, LISTS.len()))
            .children(
                self.error
                    .clone()
                    .map(|error| div().pt_2().text_color(paint(ui().error)).child(error)),
            )
    }
}

/// Opens the dialog on `base`, or on a new language.
pub(crate) fn open(
    workspace: WeakEntity<Workspace>,
    base: Option<UdlDefinition>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<DefineDialog> {
    let dialog = cx.new(|cx| DefineDialog::new(workspace, base, window, cx));
    let focus = dialog.read(cx).name.clone();
    let shown = dialog.clone();
    window.open_dialog(cx, move |modal, _, _| {
        let save = shown.clone();
        modal
            .title(t!("user-languages.define.title"))
            .w(px(720.))
            .child(shown.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("udl-save").label(t!("common.save")),
                    )),
            )
            .on_ok(move |_, window, cx| save.update(cx, |this, cx| this.save(window, cx)))
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
    dialog
}

#[cfg(test)]
#[path = "../../tests/unit/user_languages/define_tests.rs"]
mod tests;
