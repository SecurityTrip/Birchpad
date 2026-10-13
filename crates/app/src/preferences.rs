//! Settings > Preferences..., as in Notepad++: the settings of `settings.toml` by section, each
//! change written to the file at once (keeping its comments) and applied to the open documents.
//! Settings an administrator's policy sets are shown but cannot be changed.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use birchpad_config::Settings;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::{Disableable as _, Sizable as _, WindowExt as _};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px,
};
use toml::Value;

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// How a setting is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Bool,
    /// A whole number from `min` to `max`.
    Number {
        min: i64,
        max: i64,
    },
    /// One of these values, with their labels.
    Choice(&'static [(&'static str, &'static str)]),
    /// Text; empty removes the setting.
    Text,
    /// Whole numbers separated by commas or spaces.
    Numbers {
        min: i64,
        max: i64,
    },
    /// Preferences > Language: tab size and spaces of each language (`languages`).
    LanguageTabs,
}

/// A setting the dialog shows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    pub(crate) section: &'static str,
    /// The dotted key in `settings.toml`.
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) kind: Kind,
}

const fn entry(section: &'static str, key: &'static str, label: &'static str, kind: Kind) -> Entry {
    Entry {
        section,
        key,
        label,
        kind,
    }
}

const OFF_ON: &[(&str, &str)] = &[("off", "Off"), ("margin", "In the margin")];

/// Every setting the dialog shows, by section. View > Word Wrap and Show Symbol stay in the
/// View menu, which remembers them.
pub(crate) const ENTRIES: &[Entry] = &[
    entry(
        "Appearance",
        "appearance.language",
        "Interface language",
        Kind::Choice(&[
            ("auto", "Automatic (system)"),
            ("en", "English"),
            ("ru", "Русский"),
        ]),
    ),
    entry(
        "Appearance",
        "appearance.title-file-name-only",
        "Show only the file name in the title bar",
        Kind::Bool,
    ),
    entry("Toolbar", "toolbar.show", "Show the toolbar", Kind::Bool),
    entry(
        "Toolbar",
        "toolbar.size",
        "Icons",
        Kind::Choice(&[("small", "Small"), ("large", "Large")]),
    ),
    entry("Tab Bar", "tabs.vertical", "Vertical", Kind::Bool),
    entry("Tab Bar", "tabs.multi-line", "Multi-line", Kind::Bool),
    entry("Tab Bar", "tabs.reduce", "Reduce", Kind::Bool),
    entry(
        "Tab Bar",
        "tabs.lock",
        "Lock (no drag and drop)",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.close-button",
        "Show close button on each tab",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.double-click-close",
        "Double click to close document",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.peeker",
        "Show the start of a document on hover",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.switcher",
        "Document switcher (Ctrl+Tab)",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.switcher-mru",
        "Most recently used first",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.exit-on-last-close",
        "Exit on close the last tab",
        Kind::Bool,
    ),
    entry(
        "New Document",
        "new-document.encoding",
        "Encoding",
        Kind::Choice(&[
            ("utf-8", "UTF-8"),
            ("utf-8-bom", "UTF-8 with BOM"),
            ("utf-16le-bom", "UTF-16 LE BOM"),
            ("utf-16be-bom", "UTF-16 BE BOM"),
            ("ansi", "ANSI"),
        ]),
    ),
    entry(
        "New Document",
        "new-document.line-ending",
        "Line ending",
        Kind::Choice(&[
            ("system", "The system's"),
            ("crlf", "Windows (CR LF)"),
            ("lf", "Unix (LF)"),
            ("cr", "Macintosh (CR)"),
        ]),
    ),
    entry(
        "New Document",
        "new-document.language",
        "Language (its id: python, rust, text)",
        Kind::Text,
    ),
    entry("Language", "languages", "Tab settings", Kind::LanguageTabs),
    entry(
        "Default Directory",
        "files.default-directory",
        "Open and Save As start in",
        Kind::Choice(&[
            ("follow", "The current document's folder"),
            ("remember", "The folder last used"),
            ("custom", "This folder"),
        ]),
    ),
    entry(
        "Default Directory",
        "files.default-directory-path",
        "Folder",
        Kind::Text,
    ),
    entry(
        "Multi-Instance & Date",
        "instances.mode",
        "Instances",
        Kind::Choice(&[
            ("single", "Default (mono-instance)"),
            ("session", "Open session in a new instance"),
            ("multi", "Always in multi-instance mode"),
        ]),
    ),
    entry(
        "Multi-Instance & Date",
        "date.custom-format",
        "Customized date and time",
        Kind::Text,
    ),
    entry(
        "Multi-Instance & Date",
        "date.reverse",
        "Date before time (short and long)",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.enabled",
        "Enable clickable links",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.underline",
        "Underline them",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.schemes",
        "Schemes of one's own",
        Kind::Text,
    ),
    entry(
        "Cloud & Link",
        "cloud.folder",
        "Settings in this folder (from the next start)",
        Kind::Text,
    ),
    entry("Delimiter", "delimiters.open", "Open", Kind::Text),
    entry("Delimiter", "delimiters.close", "Close", Kind::Text),
    entry(
        "Delimiter",
        "delimiters.multi-line",
        "Allow on several lines",
        Kind::Bool,
    ),
    entry(
        "Search Engine",
        "search-engine.engine",
        "Search on Internet with",
        Kind::Choice(&[
            ("duckduckgo", "DuckDuckGo"),
            ("google", "Google"),
            ("bing", "Bing"),
            ("yahoo", "Yahoo!"),
            ("stack-overflow", "Stack Overflow"),
            ("custom", "Custom"),
        ]),
    ),
    entry(
        "Search Engine",
        "search-engine.custom-url",
        "Custom address ($(CURRENT_WORD))",
        Kind::Text,
    ),
    entry(
        "Editing",
        "editor.tab-width",
        "Tab size",
        Kind::Number { min: 1, max: 16 },
    ),
    entry(
        "Editing",
        "editor.insert-spaces",
        "Replace tabs by spaces",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.auto-indent",
        "Auto-indent",
        Kind::Choice(&[
            ("off", "None"),
            ("basic", "Basic"),
            ("advanced", "Advanced"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line",
        "Current line",
        Kind::Choice(&[
            ("off", "None"),
            ("background", "Background"),
            ("frame", "Frame"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line-frame-width",
        "Frame width",
        Kind::Number { min: 1, max: 6 },
    ),
    entry(
        "Auto-Completion",
        "completion.words",
        "Word and function completion",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.min-chars",
        "From character",
        Kind::Number { min: 1, max: 9 },
    ),
    entry(
        "Auto-Completion",
        "completion.call-tips",
        "Function parameters hint",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.auto-close",
        "Auto-close pairs",
        Kind::Text,
    ),
    entry("Margins", "editor.line-numbers", "Line numbers", Kind::Bool),
    entry("Margins", "editor.bookmark-margin", "Bookmarks", Kind::Bool),
    entry("Margins", "editor.fold-margin", "Folding", Kind::Bool),
    entry(
        "Margins",
        "editor.change-history",
        "Change history",
        Kind::Choice(OFF_ON),
    ),
    entry(
        "Margins",
        "editor.edge",
        "Vertical edge",
        Kind::Choice(&[
            ("off", "None"),
            ("line", "Line"),
            ("background", "Background"),
        ]),
    ),
    entry(
        "Margins",
        "editor.edge-columns",
        "Edge columns",
        Kind::Numbers { min: 1, max: 9999 },
    ),
    entry(
        "Highlighting",
        "highlighting.smart.enabled",
        "Smart highlighting",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.match-case",
        "Match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.whole-word",
        "Whole word only",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.use-find-options",
        "Use the find panel's options",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.match-case",
        "Style tokens: match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.whole-word",
        "Style tokens: whole word only",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.recent-limit",
        "Recent files",
        Kind::Number { min: 0, max: 100 },
    ),
    entry("Files", "files.ansi-encoding", "ANSI encoding", Kind::Text),
    entry(
        "Files",
        "files.large-file-limit-mb",
        "Large file limit (MB)",
        Kind::Number {
            min: 1,
            max: 1_048_576,
        },
    ),
    entry(
        "Files",
        "files.change-detection",
        "Detect changes by other programs",
        Kind::Choice(&[
            ("all", "All files"),
            ("current", "Current file"),
            ("off", "Off"),
        ]),
    ),
    entry(
        "Files",
        "files.reload-silently",
        "Reload without asking",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.reload-scrolls-to-end",
        "Go to the end after reloading",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "files.backup-on-save",
        "Backup on save",
        Kind::Choice(&[
            ("none", "None"),
            ("simple", "Simple (.bak)"),
            ("verbose", "Verbose (with the time)"),
        ]),
    ),
    entry(
        "Backup",
        "files.backup-folder",
        "Folder of verbose backups",
        Kind::Text,
    ),
    entry(
        "Backup",
        "session.remember-inaccessible",
        "Remember inaccessible files",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.remember",
        "Remember the session",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-unsaved",
        "Back up unsaved changes",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-interval-seconds",
        "Backup every (seconds)",
        Kind::Number { min: 1, max: 3600 },
    ),
    entry(
        "Updates",
        "updates.mode",
        "Updates",
        Kind::Choice(&[("off", "Off"), ("notify", "Notify"), ("auto", "Automatic")]),
    ),
    entry(
        "Updates",
        "updates.channel",
        "Channel",
        Kind::Choice(&[
            ("stable", "Stable"),
            ("beta", "Beta"),
            ("nightly", "Nightly"),
        ]),
    ),
];

/// The sections in the order listed.
pub(crate) fn sections() -> Vec<&'static str> {
    let mut sections: Vec<&str> = Vec::new();
    for entry in ENTRIES {
        if !sections.contains(&entry.section) {
            sections.push(entry.section);
        }
    }
    sections
}

/// The value of `key` in `settings`, if set.
pub(crate) fn current_value(settings: &Settings, key: &str) -> Option<Value> {
    let table = toml::Table::try_from(settings).expect("settings serialize to a table");
    let mut parts = key.split('.');
    let first = table.get(parts.next()?)?.clone();
    parts.try_fold(first, |value, part| value.get(part).cloned())
}

/// The text a field shows for `value`.
pub(crate) fn display(value: Option<&Value>) -> String {
    match value {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| display(Some(item)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(value) => value.to_string(),
    }
}

/// Reads what was typed into the field of a setting of `kind`. `None`: remove the setting.
pub(crate) fn parse_field(kind: Kind, text: &str) -> Result<Option<Value>> {
    let number = |text: &str, min: i64, max: i64| -> Result<i64> {
        let value: i64 = text.trim().parse().map_err(|_| {
            anyhow!(
                "{}",
                t!("preferences.is-not-a-whole-number", text = text.trim())
            )
        })?;
        if !(min..=max).contains(&value) {
            bail!(
                "{}",
                t!(
                    "preferences.is-not-between-and",
                    value = value,
                    min = min,
                    max = max
                )
            );
        }
        Ok(value)
    };
    match kind {
        Kind::Number { min, max } => Ok(Some(Value::Integer(number(text, min, max)?))),
        Kind::Numbers { min, max } => {
            let values = text
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|part| !part.is_empty())
                .map(|part| number(part, min, max).map(Value::Integer))
                .collect::<Result<Vec<_>>>()?;
            Ok(Some(Value::Array(values)))
        }
        Kind::Text => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| Value::String(text.to_owned())))
        }
        Kind::Bool | Kind::Choice(_) | Kind::LanguageTabs => {
            bail!("{}", t!("preferences.this-setting-has-no-text-field"))
        }
    }
}

/// Changes a setting: writes it to the user's settings file, reads the settings again and
/// redraws.
pub(crate) fn set(key: &str, value: Option<Value>, cx: &mut App) -> Result<()> {
    let state = AppState::global(cx);
    if state.is_locked(key) {
        bail!(
            "{}",
            t!("preferences.is-set-by-your-administrator", key = key)
        );
    }
    let path = state
        .paths
        .settings_file_for(key)
        .map(Path::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                t!("preferences.there-is-no-settings-file-to-write-to")
            )
        })?;
    birchpad_config::write_setting(&path, key, value.as_ref())?;
    let paths = AppState::global(cx).paths.clone();
    let resolved = birchpad_config::resolve(birchpad_config::load(&paths));
    for diagnostic in &resolved.diagnostics {
        eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
    }
    cx.global_mut::<AppState>().set_settings(resolved);
    crate::i18n::apply(cx);
    cx.refresh_windows();
    Ok(())
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.preferences", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

pub(crate) struct Preferences {
    workspace: WeakEntity<Workspace>,
    section: usize,
    /// The text fields, by key.
    inputs: HashMap<&'static str, Entity<InputState>>,
    /// What went wrong with a setting, by key.
    errors: HashMap<&'static str, String>,
    /// Preferences > Language: the language and the tab size to set.
    language_input: Entity<InputState>,
    language_width: Entity<InputState>,
    language_spaces: bool,
}

/// Opens the dialog.
pub(crate) fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let preferences = cx.new(|cx| Preferences::new(workspace, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("preferences.preferences"))
            .w(px(720.))
            .child(preferences.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

impl Preferences {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = AppState::global(cx).settings.clone();
        let mut inputs = HashMap::new();
        for entry in ENTRIES {
            if matches!(
                entry.kind,
                Kind::Bool | Kind::Choice(_) | Kind::LanguageTabs
            ) {
                continue;
            }
            let value = display(current_value(&settings, entry.key).as_ref());
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let entry = *entry;
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.field_changed(entry, cx);
                }
            })
            .detach();
            inputs.insert(entry.key, input);
        }
        Self {
            workspace,
            section: 0,
            inputs,
            errors: HashMap::new(),
            language_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(t!("preferences.language-id"))),
            language_width: cx.new(|cx| InputState::new(window, cx).default_value("4")),
            language_spaces: false,
        }
    }

    /// Sets `key`, or shows why not.
    fn apply(&mut self, key: &'static str, value: Result<Option<Value>>, cx: &mut Context<Self>) {
        match value.and_then(|value| set(key, value, cx)) {
            Ok(()) => {
                self.errors.remove(key);
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_views(cx))
                    .ok();
            }
            Err(error) => {
                self.errors.insert(key, format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// The text of a field changed.
    fn field_changed(&mut self, entry: Entry, cx: &mut Context<Self>) {
        let text = self.inputs[entry.key].read(cx).value().to_string();
        let result = parse_field(entry.kind, &text);
        self.apply(entry.key, result, cx);
    }

    /// Preferences > Language > Set: the language's own tab size and spaces.
    pub(crate) fn set_language_tabs(&mut self, cx: &mut Context<Self>) {
        let typed = self.language_input.read(cx).value().to_string();
        let width = self.language_width.read(cx).value().to_string();
        let result = language_id(&typed).and_then(|id| {
            let width = parse_field(Kind::Number { min: 1, max: 16 }, &width)?;
            set(&format!("languages.{id}.tab-width"), width, cx)?;
            set(
                &format!("languages.{id}.insert-spaces"),
                Some(Value::Boolean(self.language_spaces)),
                cx,
            )
        });
        self.after_language_change(result, cx);
    }

    /// Preferences > Language > Remove: the language takes the editor's tab settings again.
    pub(crate) fn remove_language_tabs(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = set(&format!("languages.{id}.tab-width"), None, cx)
            .and_then(|()| set(&format!("languages.{id}.insert-spaces"), None, cx));
        self.after_language_change(result, cx);
    }

    fn after_language_change(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.errors.remove("languages");
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_views(cx))
                    .ok();
            }
            Err(error) => {
                self.errors.insert("languages", format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// The language table: one row per language with its own settings, then the fields that
    /// set one.
    fn render_language_tabs(&self, locked: bool, cx: &mut Context<Self>) -> AnyElement {
        let settings = AppState::global(cx).settings.languages.clone();
        let rows = settings
            .iter()
            .filter(|(_, own)| own.tab_width.is_some() || own.insert_spaces.is_some())
            .enumerate()
            .map(|(index, (id, own))| {
                let id = id.clone();
                let describe = format!(
                    "{id}: {}{}",
                    own.tab_width
                        .map_or_else(String::new, |width| width.to_string()),
                    match own.insert_spaces {
                        Some(true) => format!(", {}", t!("preferences.spaces")),
                        Some(false) => format!(", {}", t!("preferences.tabs")),
                        None => String::new(),
                    }
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(260.)).child(describe))
                    .child(
                        Button::new(("language-remove", index))
                            .small()
                            .label(t!("common.remove"))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_language_tabs(&id, cx);
                            })),
                    )
            });
        let spaces = self.language_spaces;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(label("Tab settings"))
            .children(rows)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(160.)).child(Input::new(&self.language_input)))
                    .child(div().w(px(60.)).child(Input::new(&self.language_width)))
                    .child(
                        Checkbox::new("language-spaces")
                            .label(label("Replace tabs by spaces"))
                            .checked(spaces)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.language_spaces = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-set")
                            .small()
                            .label(t!("preferences.set"))
                            .disabled(locked)
                            .on_click(cx.listener(|this, _, _, cx| this.set_language_tabs(cx))),
                    ),
            )
            .into_any_element()
    }

    fn select_section(&mut self, index: usize, cx: &mut Context<Self>) {
        self.section = index;
        cx.notify();
    }
}

/// The id of a language as typed: its id or its name, without regard to case.
pub(crate) fn language_id(typed: &str) -> Result<String> {
    let typed = typed.trim();
    if typed.eq_ignore_ascii_case("text") || typed.eq_ignore_ascii_case("normal text") {
        return Ok("text".to_owned());
    }
    birchpad_syntax::LANGUAGES
        .iter()
        .find(|language| {
            language.id.eq_ignore_ascii_case(typed) || language.name.eq_ignore_ascii_case(typed)
        })
        .map(|language| language.id.to_owned())
        .ok_or_else(|| anyhow!("{}", t!("preferences.there-is-no-language", name = typed)))
}

/// A section's, a setting's or a choice's text: `preferences.<text>`.
fn label(text: &str) -> String {
    crate::i18n::get_or(&format!("preferences.{}", crate::i18n::slug(text)), text)
}

impl Render for Preferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let sections = sections();
        let state = AppState::global(cx);
        let settings = state.settings.clone();
        let locked_keys: Vec<&str> = ENTRIES
            .iter()
            .map(|entry| entry.key)
            .filter(|key| state.is_locked(key))
            .collect();
        let section = sections[self.section.min(sections.len() - 1)];
        let mut language_tabs = (section == "Language")
            .then(|| self.render_language_tabs(locked_keys.contains(&"languages"), cx));
        let list = sections.iter().enumerate().map(|(index, name)| {
            div()
                .id(("preferences-section", index))
                .px_2()
                .py_1()
                .when(index == self.section, |row| row.bg(paint(colors.selected)))
                .hover(|row| row.bg(paint(colors.hovered)))
                .child(label(name))
                .on_click(cx.listener(move |this, _, _, cx| this.select_section(index, cx)))
        });
        let rows = ENTRIES
            .iter()
            .filter(|entry| entry.section == section)
            .enumerate()
            .map(|(index, entry)| {
                let key = entry.key;
                let locked = locked_keys.contains(&key);
                let value = current_value(&settings, key);
                let control = match entry.kind {
                    Kind::Bool => {
                        let checked = value.as_ref().and_then(Value::as_bool).unwrap_or(false);
                        Checkbox::new(("preferences-check", index))
                            .label(label(entry.label))
                            .checked(checked)
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.apply(key, Ok(Some(Value::Boolean(!checked))), cx);
                            }))
                            .into_any_element()
                    }
                    Kind::Choice(choices) => {
                        let current = value.as_ref().and_then(Value::as_str).unwrap_or_default();
                        let selected = choices.iter().position(|(value, _)| *value == current);
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(200.)).child(label(entry.label)))
                            .child(
                                RadioGroup::horizontal(("preferences-choice", index))
                                    .children(choices.iter().map(|(_, text)| label(text)))
                                    .selected_index(selected)
                                    .disabled(locked)
                                    .on_click(cx.listener(move |this, index: &usize, _, cx| {
                                        let value = Value::String(choices[*index].0.to_owned());
                                        this.apply(key, Ok(Some(value)), cx);
                                    })),
                            )
                            .into_any_element()
                    }
                    Kind::LanguageTabs => language_tabs
                        .take()
                        .unwrap_or_else(|| div().into_any_element()),
                    Kind::Number { .. } | Kind::Numbers { .. } | Kind::Text => div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(div().w(px(200.)).child(label(entry.label)))
                        .child(
                            div()
                                .w(px(200.))
                                .child(Input::new(&self.inputs[key]).disabled(locked)),
                        )
                        .into_any_element(),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(control)
                    .when(locked, |row| {
                        row.child(
                            div()
                                .text_sm()
                                .text_color(paint(colors.muted))
                                .child(t!("preferences.set-by-your-administrator")),
                        )
                    })
                    .children(self.errors.get(key).map(|error| {
                        div()
                            .text_sm()
                            .text_color(paint(colors.error))
                            .child(error.clone())
                    }))
            });
        div()
            .flex()
            .flex_row()
            .gap_4()
            .h(px(380.))
            .child(
                div()
                    .flex_none()
                    .w(px(150.))
                    .border_1()
                    .border_color(paint(colors.border))
                    .children(list),
            )
            .child(div().flex().flex_col().flex_1().gap_3().children(rows))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod more_tests;
