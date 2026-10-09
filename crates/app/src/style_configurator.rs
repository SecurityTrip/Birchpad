//! Settings > Style Configurator..., as in Notepad++: choose a theme, and change its colors (the
//! editor's global styles, the interface, and the highlighting styles of every language or of
//! one) with the result shown as they change. Save & Close keeps the theme as a file of the
//! `themes` folder; Cancel goes back to the theme in use before.

use anyhow::{Context as _, Result, anyhow};
use birchpad_theme::{Color, EditorColors, Style, Theme, UiColors};
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, Context, Entity, SharedString, WeakEntity, Window, div, prelude::*, px,
};

use crate::theme::{paint, ui};
use crate::themes;
use crate::workspace::Workspace;

/// What a list entry on the left styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    /// The text area: Notepad++'s Global Styles.
    Editor,
    /// Everything around the text.
    Interface,
    /// Highlighting, for every language (`None`) or one.
    Syntax(Option<&'static str>),
}

/// The groups in the order listed, with their labels.
pub(crate) fn groups() -> Vec<(String, Group)> {
    let mut groups = vec![
        ("Global Styles".to_owned(), Group::Editor),
        ("Interface".to_owned(), Group::Interface),
        ("All languages".to_owned(), Group::Syntax(None)),
    ];
    groups.extend(
        birchpad_syntax::LANGUAGES
            .iter()
            .map(|language| (language.name.to_owned(), Group::Syntax(Some(language.id)))),
    );
    groups
}

/// The styles of a group.
pub(crate) fn items(group: Group) -> &'static [&'static str] {
    match group {
        Group::Editor => EditorColors::KEYS,
        Group::Interface => UiColors::KEYS,
        Group::Syntax(_) => birchpad_syntax::HIGHLIGHT_NAMES,
    }
}

/// What the fields on the right show for one style: colors as `#rrggbb` text, empty when
/// unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Fields {
    pub(crate) color: String,
    pub(crate) background: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
}

/// The fields of `item` in `group`. A highlighting style shows what this group sets itself;
/// empty fields take the style of the name's prefix, or of all languages.
pub(crate) fn read_fields(theme: &Theme, group: Group, item: &str) -> Fields {
    let color = |color: Option<Color>| color.map(|color| color.to_string()).unwrap_or_default();
    match group {
        Group::Editor => Fields {
            color: color(theme.editor.get(item)),
            ..Fields::default()
        },
        Group::Interface => Fields {
            color: color(theme.ui.get(item)),
            ..Fields::default()
        },
        Group::Syntax(language) => {
            let styles = match language {
                None => Some(&theme.syntax),
                Some(id) => theme.languages.get(id),
            };
            let style = styles
                .and_then(|styles| styles.get(item))
                .copied()
                .unwrap_or_default();
            Fields {
                color: color(style.color),
                background: color(style.background),
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
            }
        }
    }
}

/// Reads a color field: empty is unset.
fn parse(text: &str, what: &str) -> Result<Option<Color>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let text = if text.starts_with('#') {
        text.to_owned()
    } else {
        format!("#{text}")
    };
    text.parse()
        .map(Some)
        .map_err(|_| anyhow!("{what}: {text} is not a color; write #rrggbb or #rrggbbaa"))
}

/// Sets `item` of `group` from `fields`. A highlighting style with nothing set is removed, so
/// that the name takes its prefix's style again.
pub(crate) fn write_fields(
    theme: &mut Theme,
    group: Group,
    item: &str,
    fields: &Fields,
) -> Result<()> {
    let color = parse(&fields.color, "Color")?;
    match group {
        Group::Editor | Group::Interface => {
            let color = color.ok_or_else(|| anyhow!("Color: this style needs a color"))?;
            let set = match group {
                Group::Editor => theme.editor.set(item, color),
                _ => theme.ui.set(item, color),
            };
            if !set {
                return Err(anyhow!("there is no style {item}"));
            }
        }
        Group::Syntax(language) => {
            if !birchpad_syntax::HIGHLIGHT_NAMES.contains(&item) {
                return Err(anyhow!("there is no style {item}"));
            }
            let style = Style {
                color,
                background: parse(&fields.background, "Background")?,
                bold: fields.bold,
                italic: fields.italic,
                underline: fields.underline,
            };
            let styles = match language {
                None => &mut theme.syntax,
                Some(id) => theme.languages.entry(id.to_owned()).or_default(),
            };
            if style == Style::default() {
                styles.remove(item);
            } else {
                styles.insert(item.to_owned(), style);
            }
            if let Some(id) = language
                && theme
                    .languages
                    .get(id)
                    .is_some_and(|styles| styles.is_empty())
            {
                theme.languages.remove(id);
            }
        }
    }
    Ok(())
}

/// Saves `theme` in `dir` as `<name>.toml`, which comes before a Notepad++ file of the same name
/// and replaces a built-in theme of that name.
pub(crate) fn save(theme: &Theme, dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(format!("{}.toml", theme.name));
    std::fs::write(&path, theme.to_toml())
        .with_context(|| format!("cannot save the theme as {}", path.display()))
}

pub(crate) struct StyleConfigurator {
    workspace: WeakEntity<Workspace>,
    names: Vec<String>,
    /// The theme in use when the dialog opened, for Cancel.
    original: Theme,
    theme: Theme,
    groups: Vec<(String, Group)>,
    group: usize,
    item: usize,
    fields: Fields,
    color: Entity<InputState>,
    background: Entity<InputState>,
    /// Set while the fields are filled in, so that it is not taken as an edit.
    filling: bool,
    error: Option<String>,
}

/// Opens the Style Configurator.
pub(crate) fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let configurator = cx.new(|cx| StyleConfigurator::new(workspace, window, cx));
    let save = configurator.clone();
    let cancel = configurator.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let save = save.clone();
        let cancel = cancel.clone();
        dialog
            .title("Style Configurator")
            .w(px(820.))
            .child(configurator.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("Cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("style-save").label("Save & Close"),
                    )),
            )
            .on_ok(move |_, _, cx| save.update(cx, |this, cx| this.save(cx)))
            .on_cancel(move |_, _, cx| {
                cancel.update(cx, |this, cx| this.revert(cx));
                true
            })
    });
}

impl StyleConfigurator {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let original = (*crate::theme::current()).clone();
        let color = cx.new(|cx| InputState::new(window, cx).placeholder("#rrggbb"));
        let background = cx.new(|cx| InputState::new(window, cx).placeholder("none"));
        for input in [&color, &background] {
            cx.subscribe_in(input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) && !this.filling {
                    this.edit(cx);
                }
            })
            .detach();
        }
        let mut this = Self {
            workspace,
            names: themes::names(themes::themes_dir(cx).as_deref()),
            theme: original.clone(),
            original,
            groups: groups(),
            group: 0,
            item: 0,
            fields: Fields::default(),
            color,
            background,
            filling: false,
            error: None,
        };
        this.fill(window, cx);
        this
    }

    fn current_group(&self) -> Group {
        self.groups[self.group].1
    }

    fn current_item(&self) -> &'static str {
        items(self.current_group())[self.item]
    }

    /// Shows the selected style in the fields.
    fn fill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fields = read_fields(&self.theme, self.current_group(), self.current_item());
        self.error = None;
        self.filling = true;
        let (color, background) = (self.fields.color.clone(), self.fields.background.clone());
        self.color
            .update(cx, |input, cx| input.set_value(color, window, cx));
        self.background
            .update(cx, |input, cx| input.set_value(background, window, cx));
        self.filling = false;
        cx.notify();
    }

    /// Takes the fields into the theme, and shows the result.
    fn edit(&mut self, cx: &mut Context<Self>) {
        self.fields.color = self.color.read(cx).value().to_string();
        self.fields.background = self.background.read(cx).value().to_string();
        let (group, item) = (self.current_group(), self.current_item());
        match write_fields(&mut self.theme, group, item, &self.fields) {
            Ok(()) => {
                self.error = None;
                themes::apply(self.theme.clone(), cx);
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
        cx.notify();
    }

    fn select_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match themes::load(themes::themes_dir(cx).as_deref(), &self.names[index]) {
            Ok(theme) => {
                self.theme = theme;
                themes::apply(self.theme.clone(), cx);
                self.fill(window, cx);
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }

    fn select_group(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.group = index;
        self.item = 0;
        self.fill(window, cx);
    }

    fn select_item(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.item = index;
        self.fill(window, cx);
    }

    /// Save & Close: false keeps the dialog open to show what went wrong.
    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        if self.error.is_some() {
            return false;
        }
        let result = themes::themes_dir(cx)
            .ok_or_else(|| anyhow!("there is no settings folder to keep themes in"))
            .and_then(|dir| save(&self.theme, &dir));
        if let Err(error) = result {
            self.error = Some(format!("{error:#}"));
            cx.notify();
            return false;
        }
        let theme = self.theme.clone();
        self.workspace
            .update(cx, |workspace, cx| themes::choose(workspace, theme, cx))
            .ok();
        true
    }

    /// Cancel: the theme in use before.
    fn revert(&mut self, cx: &mut Context<Self>) {
        themes::apply(self.original.clone(), cx);
    }

    fn toggle(&mut self, change: fn(&mut Fields), cx: &mut Context<Self>) {
        change(&mut self.fields);
        self.edit(cx);
    }
}

/// A clickable row of a list.
fn row(id: SharedString, label: String, selected: bool) -> gpui_kit::Stateful<gpui_kit::Div> {
    let colors = ui();
    div()
        .id(id)
        .px_2()
        .py(px(2.))
        .whitespace_nowrap()
        .when(selected, |row| row.bg(paint(colors.selected)))
        .hover(|row| row.bg(paint(colors.hovered)))
        .child(label)
}

/// A scrolling list.
fn list(id: &'static str) -> gpui_kit::Stateful<gpui_kit::Div> {
    let colors = ui();
    div()
        .id(id)
        .flex_none()
        .h(px(340.))
        .overflow_y_scroll()
        .border_1()
        .border_color(paint(colors.border))
        .text_sm()
}

impl Render for StyleConfigurator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let current = self.theme.name.clone();
        let themes = self.names.iter().enumerate().map(|(index, name)| {
            let selected = name.eq_ignore_ascii_case(&current);
            row(
                format!("style-theme-{index}").into(),
                name.clone(),
                selected,
            )
            .border_1()
            .border_color(paint(colors.border))
            .rounded(px(4.))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_theme(index, window, cx);
            }))
        });
        let groups = self.groups.iter().enumerate().map(|(index, (label, _))| {
            row(
                format!("style-group-{index}").into(),
                label.clone(),
                index == self.group,
            )
            .on_click(cx.listener(move |this, _, window, cx| this.select_group(index, window, cx)))
        });
        let items = items(self.current_group())
            .iter()
            .enumerate()
            .map(|(index, name)| {
                row(
                    format!("style-item-{index}").into(),
                    (*name).to_owned(),
                    index == self.item,
                )
                .on_click(
                    cx.listener(move |this, _, window, cx| this.select_item(index, window, cx)),
                )
            });
        let is_style = matches!(self.current_group(), Group::Syntax(_));
        let swatch = parse(&self.fields.color, "").ok().flatten().map(|color| {
            div()
                .size(px(18.))
                .border_1()
                .border_color(paint(colors.border))
                .bg(paint(color))
        });
        let label = |text: &'static str| div().flex_none().w(px(90.)).child(text);
        let field_row = || div().flex().flex_row().items_center().gap_2();
        let checkbox =
            |id: &'static str, text: &'static str, checked: bool, change: fn(&mut Fields)| {
                Checkbox::new(id)
                    .label(text)
                    .checked(checked)
                    .disabled(!is_style)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle(change, cx)))
            };
        let hint = match self.current_group() {
            Group::Syntax(None) => "Empty fields take the style of the name before its last dot.",
            Group::Syntax(Some(_)) => "Empty fields take the style for all languages.",
            _ => "",
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(div().mr_2().child("Theme:"))
                    .children(themes),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(list("style-groups").w(px(180.)).children(groups))
                    .child(list("style-items").w(px(220.)).children(items))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap_2()
                            .child(
                                field_row()
                                    .child(label(if is_style { "Foreground:" } else { "Color:" }))
                                    .child(Input::new(&self.color).id("style-color"))
                                    .children(swatch),
                            )
                            .child(
                                field_row().child(label("Background:")).child(
                                    Input::new(&self.background)
                                        .id("style-background")
                                        .disabled(!is_style),
                                ),
                            )
                            .child(checkbox("style-bold", "Bold", self.fields.bold, |fields| {
                                fields.bold = !fields.bold;
                            }))
                            .child(checkbox(
                                "style-italic",
                                "Italic",
                                self.fields.italic,
                                |fields| {
                                    fields.italic = !fields.italic;
                                },
                            ))
                            .child(checkbox(
                                "style-underline",
                                "Underline",
                                self.fields.underline,
                                |fields| fields.underline = !fields.underline,
                            ))
                            .child(div().text_sm().text_color(paint(colors.muted)).child(hint))
                            .children(
                                self.error.clone().map(|error| {
                                    div().text_color(paint(colors.error)).child(error)
                                }),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests;
