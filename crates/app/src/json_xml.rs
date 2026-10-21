//! Plugins > JSON Viewer and XML Tools (ADR 0039, ADR 0048): checking, formatting, sorting keys
//! and putting on one line the selection, or the whole text when nothing is selected; JSON
//! Viewer's tree (`panels::json_viewer`) and copying a value, its key or its path; XML Tools'
//! XPath, validation against a DTD or an XML Schema, escaping and unescaping, and closing tags
//! as they are typed.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::{Edit, Range, Selection, Transaction};
use birchpad_tools::json_tree::JsonTree;
use birchpad_tools::xml_schema::{self, Problem};
use birchpad_tools::xpath::{self, XPathMatch, XPathResult};
use birchpad_tools::{Indent, Layout, SyntaxError, json, json_tree, xml};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, Global, WeakEntity, Window, div,
    prelude::*, px,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::panels::json_viewer::{self, Copy};
use crate::panels::{JsonViewer, PanelKind};
use crate::workspace::{Workspace, report_error};

#[derive(Deserialize)]
struct PathArgs {
    path: String,
}

#[derive(Deserialize, Default)]
struct XPathArgs {
    /// The expression; none opens the dialog.
    #[serde(default)]
    expression: Option<String>,
}

#[derive(Deserialize, Default)]
struct ValidateArgs {
    /// An XML Schema to validate against, instead of what the document names.
    #[serde(default)]
    schema: Option<PathBuf>,
}

/// The last XPath expression evaluated, and what it gave.
#[derive(Default)]
pub struct LastXPath {
    pub expression: String,
    pub result: Option<Result<XPathResult, String>>,
}

impl Global for LastXPath {}

/// The problems the last validation found, in the document's order.
#[derive(Default)]
pub struct LastValidation {
    pub problems: Vec<Problem>,
}

impl Global for LastValidation {}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("json.format", |this, (), _, cx| {
        reformat(this, cx, json::format)
    });
    registry.editor("json.minify", |this, (), _, cx| {
        reformat(this, cx, |text, _| json::minify(text))
    });
    registry.editor("json.validate", |this, (), window, cx| {
        validate(this, "JSON", json::validate, window, cx)
    });
    registry.editor("xml.pretty-print", |this, (), _, cx| {
        reformat(this, cx, xml::pretty_print)
    });
    registry.editor("xml.linearize", |this, (), _, cx| {
        reformat(this, cx, |text, _| xml::linearize(text))
    });
    registry.editor("xml.check", |this, (), window, cx| {
        validate(this, "XML", xml::check, window, cx)
    });
    registry.editor("json.sort-keys", |this, (), _, cx| {
        reformat(this, cx, json_tree::sort_keys)
    });
    registry.editor("xml.escape", |this, (), _, cx| {
        reformat(this, cx, |text, _| Ok(xml::escape(text)))
    });
    registry.editor("xml.unescape", |this, (), _, cx| {
        reformat(this, cx, |text, _| Ok(xml::unescape(text)))
    });
    registry.workspace("json.find-path", |this, args: PathArgs, window, cx| {
        let viewer = this.json_viewer(window, cx);
        let view = this.active_view(cx);
        viewer.update(cx, |viewer, cx| viewer.find(&args.path, view, window, cx))
    });
    for (id, what) in [
        ("json.copy-value", Copy::Value),
        ("json.copy-key", Copy::Key),
        ("json.copy-path", Copy::Path),
    ] {
        registry.workspace(id, move |this, (), _, cx| {
            let text = this.json_copied(what, cx)?;
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            Ok(())
        });
    }
    registry.workspace(
        "xml.xpath",
        |this, args: XPathArgs, window, cx| match args.expression {
            Some(expression) => this.evaluate_xpath(&expression, window, cx),
            None => {
                open_xpath(cx.entity().downgrade(), window, cx);
                Ok(())
            }
        },
    );
    registry.workspace("xml.validate", |this, args: ValidateArgs, window, cx| {
        this.validate_xml(args.schema, window, cx)
    });
    registry.workspace("xml.tag-auto-close", |_, (), _, cx| {
        let on = AppState::global(cx).settings.completion.close_tags;
        crate::preferences::set("completion.close-tags", Some(toml::Value::Boolean(!on)), cx)
    });
}

impl Workspace {
    /// The JSON Viewer panel, opened.
    fn json_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<JsonViewer> {
        self.open_panel(PanelKind::JsonViewer, window, cx);
        self.docks
            .view(PanelKind::JsonViewer)
            .and_then(|view| view.clone().downcast::<JsonViewer>().ok())
            .expect("the panel just opened")
    }

    /// What Copy Value, Copy Key or Copy Path copies: of the value chosen in JSON Viewer, or of
    /// the value at the caret.
    fn json_copied(&mut self, what: Copy, cx: &mut Context<Self>) -> Result<String> {
        if self.docks.is_open(PanelKind::JsonViewer)
            && let Some(viewer) = self
                .docks
                .view(PanelKind::JsonViewer)
                .and_then(|view| view.clone().downcast::<JsonViewer>().ok())
        {
            let view = self.active_view(cx);
            return viewer.update(cx, |viewer, cx| viewer.copied(what, view, cx));
        }
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("panels.json-viewer.no-document")))?;
        let parsed = json_viewer::parse(&view, None, cx);
        let tree: &JsonTree = parsed
            .tree
            .as_ref()
            .map_err(|error| anyhow!("{}", error.message))?;
        let caret = view.read(cx).selection.primary().head;
        let at = json_viewer::node_at(tree, caret)
            .ok_or_else(|| anyhow!("{}", t!("panels.json-viewer.nothing-chosen")))?;
        Ok(match what {
            Copy::Value => tree.value(at, &parsed.text).into_owned(),
            Copy::Path => tree.path(at),
            Copy::Key => match &tree.nodes[at].key {
                Some(json_tree::JsonKey::Name(name)) => name.clone(),
                Some(json_tree::JsonKey::Index(index)) => index.to_string(),
                None => String::new(),
            },
        })
    }

    /// Evaluates `expression` over the active document; the first node it selects is selected.
    fn evaluate_xpath(
        &mut self,
        expression: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("json-xml.no-document")))?;
        let text = view.read(cx).buffer.read(cx).doc().text().to_string();
        let result = xpath::evaluate(&text, expression);
        if let Ok(XPathResult::Nodes(nodes)) = &result
            && let Some(first) = nodes.first()
        {
            let range = first.range.clone();
            view.update(cx, |view, cx| view.select_range(range, cx));
        }
        let failed = result.as_ref().err().map(ToString::to_string);
        cx.set_global(LastXPath {
            expression: expression.to_owned(),
            result: Some(result.map_err(|error| error.to_string())),
        });
        cx.refresh_windows();
        let _ = window;
        failed.map_or(Ok(()), |error| Err(anyhow!(error)))
    }

    /// XML Tools > Validate Now: against `schema`, or the DTD the document declares, or the
    /// schema it names; without either, asks for a schema.
    fn validate_xml(
        &mut self,
        schema: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("json-xml.no-document")))?;
        let buffer = view.read(cx).buffer.read(cx);
        let text = buffer.doc().text().to_string();
        let folder = buffer.path().and_then(Path::parent).map(Path::to_owned);
        let resolve = |name: &str| -> Result<PathBuf> {
            if name.contains("://") && !name.starts_with("file://") {
                bail!("{}", t!("json-xml.schema-not-a-file", name = name));
            }
            let name = name.strip_prefix("file://").unwrap_or(name);
            let path = Path::new(name);
            Ok(match (&folder, path.is_absolute()) {
                (Some(folder), false) => folder.join(path),
                _ => path.to_owned(),
            })
        };
        let read = |path: &Path| -> Result<String> {
            std::fs::read_to_string(path)
                .with_context(|| t!("json-xml.cannot-read-schema", path = path.display()))
        };
        let problems = if let Some(schema) = schema {
            xml_schema::validate_xsd(&text, &read(&schema)?)?
        } else if let Some(doctype) = xml_schema::doctype(&text) {
            let external = match &doctype.system {
                Some(system) => Some(read(&resolve(system)?)?),
                None => None,
            };
            xml_schema::validate_dtd(&text, external.as_deref())?
        } else if let Some(location) = xml_schema::schema_location(&text) {
            xml_schema::validate_xsd(&text, &read(&resolve(&location)?)?)?
        } else {
            self.ask_schema(window, cx);
            return Ok(());
        };
        cx.set_global(LastValidation {
            problems: problems.clone(),
        });
        let Some(first) = problems.first() else {
            crate::workspace::report_success(t!("json-xml.valid", kind = "XML"), window, cx);
            return Ok(());
        };
        let range = xml_schema::tag_at(&text, first.offset);
        view.update(cx, |view, cx| view.select_range(range, cx));
        let error = SyntaxError {
            offset: first.offset,
            message: first.message.clone(),
        };
        let (line, column) = error.line_column(&text);
        let more = problems.len() - 1;
        let message = t!(
            "json-xml.line-column",
            line = line,
            column = column,
            error = first.message
        );
        Err(if more == 0 {
            anyhow!(message)
        } else {
            anyhow!(
                "{}",
                t!("json-xml.and-more", message = message, count = more)
            )
        })
    }

    /// Validate Now without a DTD or a schema named: asks for the schema's file.
    fn ask_schema(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(start, cx).await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.validate_xml(Some(path), window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

/// XML Tools > Evaluate XPath Expression...: a field, Evaluate, and the nodes it selects.
pub struct XPathDialog {
    workspace: WeakEntity<Workspace>,
    pub input: Entity<InputState>,
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The dialog last opened, for tests to drive.
    pub static XPATH_OPENED: std::cell::RefCell<Option<WeakEntity<XPathDialog>>> =
        std::cell::RefCell::default();
}

fn open_xpath(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let last = cx
        .try_global::<LastXPath>()
        .map(|last| last.expression.clone())
        .unwrap_or_default();
    let dialog = cx.new(|cx| {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("/root/item[@id='1']")
                .default_value(last)
        });
        cx.subscribe_in(
            &input,
            window,
            |this: &mut XPathDialog, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.evaluate(window, cx);
                }
            },
        )
        .detach();
        XPathDialog { workspace, input }
    });
    #[cfg(feature = "test-support")]
    XPATH_OPENED.with(|opened| *opened.borrow_mut() = Some(dialog.downgrade()));
    let evaluate = dialog.downgrade();
    window.open_dialog(cx, move |modal, _, _| {
        let evaluate = evaluate.clone();
        modal
            .title(t!("json-xml.evaluate-xpath"))
            .w(px(560.))
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("xpath-evaluate")
                            .primary()
                            .label(t!("json-xml.evaluate"))
                            .on_click(move |_, window, cx| {
                                evaluate
                                    .update(cx, |dialog, cx| dialog.evaluate(window, cx))
                                    .ok();
                            }),
                    )
                    .child(crate::workspace::dialog_close("common.close")),
            )
    });
}

impl XPathDialog {
    pub fn evaluate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let expression = self.input.read(cx).value().to_string();
        let invocation = birchpad_commands::Invocation::with_args(
            "xml.xpath",
            serde_json::json!({ "expression": expression }),
        );
        self.workspace
            .update(cx, |workspace, cx| {
                // The error shows in the list too.
                let _ = workspace.dispatch(&invocation, window, cx);
            })
            .ok();
        cx.notify();
    }

    /// Selects a node the expression found.
    fn go_to(&self, found: &XPathMatch, window: &mut Window, cx: &mut Context<Self>) {
        let range = found.range.clone();
        self.workspace
            .update(cx, |workspace, cx| {
                if let Some(view) = workspace.active_view(cx) {
                    view.update(cx, |view, cx| view.select_range(range, cx));
                }
            })
            .ok();
        let _ = window;
    }
}

impl Render for XPathDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::ui();
        let result = cx
            .try_global::<LastXPath>()
            .and_then(|last| last.result.clone());
        let results: Vec<gpui_kit::AnyElement> = match result {
            None => Vec::new(),
            Some(Err(error)) => vec![
                div()
                    .text_color(crate::theme::paint(colors.error))
                    .child(error)
                    .into_any_element(),
            ],
            Some(Ok(XPathResult::Value(value))) => vec![div().child(value).into_any_element()],
            Some(Ok(XPathResult::Nodes(nodes))) if nodes.is_empty() => vec![
                div()
                    .text_color(crate::theme::paint(colors.muted))
                    .child(t!("json-xml.no-nodes"))
                    .into_any_element(),
            ],
            Some(Ok(XPathResult::Nodes(nodes))) => nodes
                .into_iter()
                .enumerate()
                .map(|(index, found)| {
                    let label = found.label.clone();
                    div()
                        .id(("xpath-result", index))
                        .debug_selector(move || format!("xpath-result-{index}"))
                        .px_1()
                        .cursor_pointer()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .hover(|row| row.bg(crate::theme::paint(colors.hovered)))
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.go_to(&found, window, cx);
                        }))
                        .into_any_element()
                })
                .collect(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "xpath-expression".into())
                    .child(Input::new(&self.input)),
            )
            .child(
                div()
                    .id("xpath-results")
                    .flex()
                    .flex_col()
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .children(results),
            )
    }
}

/// The part of the text the tools work on: the primary selection, or all of it.
fn target(view: &EditorView, cx: &Context<EditorView>) -> (std::ops::Range<usize>, String) {
    let text = view.text(cx);
    let primary = view.selection.primary();
    let range = if primary.is_empty() {
        0..text.len()
    } else {
        primary.from()..primary.to()
    };
    let part = text.slice(range.clone()).to_string();
    (range, part)
}

/// Puts the caret where `error` is and says what it is, with its line and column in the text.
fn locate(
    view: &mut EditorView,
    start: usize,
    part: &str,
    error: &SyntaxError,
    cx: &mut Context<EditorView>,
) -> anyhow::Error {
    view.go_to(start + error.offset.min(part.len()), cx);
    let text = view.text(cx).clone();
    let pos = view.selection.primary().head;
    let line = birchpad_core::motion::line_of(&text, pos);
    let column = text
        .slice(birchpad_core::motion::line_range(&text, line).start..pos)
        .chars()
        .count();
    anyhow!(
        "{}",
        t!(
            "json-xml.line-column",
            line = line + 1,
            column = column + 1,
            error = error.message
        )
    )
}

fn reformat(
    view: &mut EditorView,
    cx: &mut Context<EditorView>,
    tool: impl FnOnce(&str, Layout<'_>) -> Result<String, SyntaxError>,
) -> Result<()> {
    if !view.is_editable(cx) {
        bail!("{}", t!("json-xml.the-document-cannot-be-changed"));
    }
    let (range, part) = target(view, cx);
    let settings = view.view_settings(cx);
    let eol = view.buffer.read(cx).doc().line_ending();
    let layout = Layout {
        indent: if settings.insert_spaces {
            Indent::Spaces(u8::try_from(settings.tab_width).unwrap_or(u8::MAX))
        } else {
            Indent::Tab
        },
        line_ending: eol.as_str(),
    };
    let out = match tool(&part, layout) {
        Ok(out) => out,
        Err(error) => return Err(locate(view, range.start, &part, &error, cx)),
    };
    if out == part {
        return Ok(());
    }
    let whole = range.start == 0 && range.end == view.text(cx).len();
    let selection = if whole {
        Selection::point(0)
    } else {
        Selection::single(Range::new(range.start, range.start + out.len()))
    };
    let transaction = Transaction::from_edits(view.text(cx), [Edit::replace(range, out)])?
        .with_selection(selection);
    view.apply_command_edit(transaction, cx);
    Ok(())
}

fn validate(
    view: &mut EditorView,
    kind: &str,
    check: impl FnOnce(&str) -> Result<(), SyntaxError>,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    let (range, part) = target(view, cx);
    match check(&part) {
        Ok(()) => {
            crate::workspace::report_success(t!("json-xml.valid", kind = kind), window, cx);
            Ok(())
        }
        Err(error) => Err(locate(view, range.start, &part, &error, cx)),
    }
}
