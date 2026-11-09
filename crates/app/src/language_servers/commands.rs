//! Tools > Language Server's commands (ADR 0065): Show Hover, Go to Definition, Find
//! References, Rename Symbol, Format Document, the next and the previous diagnostic and their
//! list, and Restart.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::{Rope, Transaction};
use birchpad_editor::buffer::{Buffer, ReadOnly};
use birchpad_editor::{DiagnosticLevel, EditorView};
use birchpad_lsp::PositionEncoding;
use birchpad_lsp::convert::{self, FileRange, Severity};
use birchpad_lsp::lsp_types;
use birchpad_lsp::position::byte_range;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px};
use serde::Deserialize;
use serde_json::json;

use super::{Target, diagnostics, target, timeout};
use crate::commands::{CommandRegistry, Register as _};
use crate::search_results::{FileResults, ResultLocation, SearchRun};
use crate::workspace::{Workspace, report_error, report_info};

/// How long a document opened for a definition or an edit may take to be read.
const LOAD_WAIT: Duration = Duration::from_secs(10);

#[derive(Deserialize, Default)]
struct RenameArgs {
    #[serde(default)]
    name: Option<String>,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("lsp.hover", |this, (), window, cx| hover(this, window, cx));
    registry.workspace("lsp.definition", |this, (), window, cx| {
        definition(this, window, cx)
    });
    registry.workspace("lsp.references", |this, (), window, cx| {
        references(this, window, cx)
    });
    registry.workspace(
        "lsp.rename",
        |this, args: RenameArgs, window, cx| match args.name {
            Some(name) => rename(this, name, window, cx),
            None => {
                ask_new_name(this, window, cx)?;
                Ok(())
            }
        },
    );
    registry.workspace("lsp.format", |this, (), window, cx| {
        format(this, window, cx)
    });
    registry.workspace("lsp.next-diagnostic", |this, (), _, cx| {
        go_to_diagnostic(this, true, cx)
    });
    registry.workspace("lsp.previous-diagnostic", |this, (), _, cx| {
        go_to_diagnostic(this, false, cx)
    });
    registry.workspace("lsp.diagnostics", |this, (), _, cx| {
        list_diagnostics(this, cx)
    });
    registry.workspace("lsp.restart", |this, (), window, cx| {
        let (_, buffer) = active(this, cx)?;
        super::restart(&buffer, cx)?;
        this.sync_document_tools(window, cx);
        Ok(())
    });
}

/// The active document's view and buffer.
fn active(
    this: &Workspace,
    cx: &Context<Workspace>,
) -> Result<(Entity<EditorView>, Entity<Buffer>)> {
    let view = this
        .active_view(cx)
        .ok_or_else(|| anyhow!("{}", t!("language-servers.no-document")))?;
    let buffer = view.read(cx).buffer.clone();
    Ok((view, buffer))
}

/// Fails with what the server lacks when it does not offer `feature`.
fn offers(target: &Target, offered: bool, feature: &str) -> Result<()> {
    if offered {
        Ok(())
    } else {
        bail!(
            "{}",
            t!(
                "language-servers.does-not-offer",
                language = target.key.language,
                feature = feature
            )
        )
    }
}

/// A diagnostic's line as shown: its severity and message.
pub fn describe(level: DiagnosticLevel, message: &str) -> String {
    let severity = match level {
        DiagnosticLevel::Error => Severity::Error,
        DiagnosticLevel::Warning => Severity::Warning,
        DiagnosticLevel::Information => Severity::Information,
        DiagnosticLevel::Hint => Severity::Hint,
    };
    format!("{}: {message}", severity.label())
}

/// Show Hover: the server's text and the diagnostics at the caret, in the call tip.
fn hover(this: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    let head = view.read(cx).selection.primary().head;
    let messages: Vec<String> = diagnostics(buffer.entity_id(), cx)
        .iter()
        .filter(|entry| entry.range.start <= head && head <= entry.range.end)
        .map(|entry| describe(entry.level, &entry.message))
        .collect();
    let asked = match target(&buffer, cx) {
        Ok(target) if target.abilities.hover => Some(target),
        Ok(_) | Err(_) if !messages.is_empty() => None,
        Ok(target) => {
            offers(&target, false, &t!("language-servers.hover"))?;
            None
        }
        Err(error) => return Err(error),
    };
    let Some(target) = asked else {
        view.update(cx, |view, cx| view.show_tip(messages.join("\n"), cx));
        return Ok(());
    };
    let answer = target
        .client
        .request("textDocument/hover", target.document_position(head));
    cx.spawn_in(window, async move |_, cx| {
        let result = timeout(answer, cx).await;
        cx.update(|window, cx| {
            let hover = match result {
                Ok(value) => serde_json::from_value::<Option<lsp_types::Hover>>(value)
                    .ok()
                    .flatten()
                    .map(|hover| convert::hover_text(&hover.contents))
                    .unwrap_or_default(),
                Err(error) => {
                    report_error(&anyhow!(error), window, cx);
                    return;
                }
            };
            let mut parts = messages;
            if !hover.is_empty() {
                parts.push(hover);
            }
            if parts.is_empty() {
                report_info(t!("language-servers.nothing-to-show"), window, cx);
                return;
            }
            view.update(cx, |view, cx| view.show_tip(parts.join("\n\n"), cx));
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// The word at the caret, for headings.
fn word_at(view: &Entity<EditorView>, cx: &gpui_kit::App) -> String {
    let view = view.read(cx);
    let text = view.text(cx);
    let head = view.selection.primary().head;
    let range = birchpad_core::motion::word_at(text, head);
    text.slice(range).to_string()
}

/// Go to Definition: the file opened at it, or a list of them.
fn definition(
    this: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    let target = target(&buffer, cx)?;
    offers(
        &target,
        target.abilities.definition,
        &t!("language-servers.definitions"),
    )?;
    let head = view.read(cx).selection.primary().head;
    let word = word_at(&view, cx);
    let encoding = target.abilities.encoding;
    let answer = target
        .client
        .request("textDocument/definition", target.document_position(head));
    cx.spawn_in(window, async move |this, cx| {
        let result = timeout(answer, cx).await;
        this.update_in(cx, |this, window, cx| {
            let places = match result {
                Ok(value) => convert::definitions(serde_json::from_value(value).ok().flatten()),
                Err(error) => {
                    report_error(&anyhow!(error), window, cx);
                    return;
                }
            };
            match places.as_slice() {
                [] => report_info(
                    t!("language-servers.no-definition", name = word),
                    window,
                    cx,
                ),
                [place] => open_place(this, place.clone(), encoding, window, cx),
                _ => {
                    let heading = t!("language-servers.definitions-of", name = word);
                    list_places(this, &heading, places, encoding, cx);
                }
            }
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// Find References: every place in the search results.
fn references(
    this: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    let target = target(&buffer, cx)?;
    offers(
        &target,
        target.abilities.references,
        &t!("language-servers.references"),
    )?;
    let head = view.read(cx).selection.primary().head;
    let word = word_at(&view, cx);
    let encoding = target.abilities.encoding;
    let mut params = target.document_position(head);
    params["context"] = json!({ "includeDeclaration": true });
    let answer = target.client.request("textDocument/references", params);
    cx.spawn_in(window, async move |this, cx| {
        let result = timeout(answer, cx).await;
        this.update_in(cx, |this, window, cx| {
            let places = match result {
                Ok(value) => convert::references(serde_json::from_value(value).ok().flatten()),
                Err(error) => {
                    report_error(&anyhow!(error), window, cx);
                    return;
                }
            };
            if places.is_empty() {
                report_info(
                    t!("language-servers.no-references", name = word),
                    window,
                    cx,
                );
                return;
            }
            let heading = t!("language-servers.references-of", name = word);
            list_places(this, &heading, places, encoding, cx);
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// The text of `path`: its open document's, or the file's.
fn text_of(this: &Workspace, path: &Path, cx: &gpui_kit::App) -> Option<(Rope, ResultLocation)> {
    if let Some(view) = this.view_for_path(path, cx) {
        let buffer = view.read(cx).buffer.clone();
        let text = buffer.read(cx).doc().text().clone();
        return Some((text, ResultLocation::Buffer(buffer.downgrade())));
    }
    let bytes = std::fs::read(path).ok()?;
    let text = Rope::from_str(&String::from_utf8_lossy(&bytes));
    Some((text, ResultLocation::File(path.to_owned())))
}

/// Places in the search results, under `heading`, by file and line.
pub fn list_places(
    this: &mut Workspace,
    heading: &str,
    places: Vec<FileRange>,
    encoding: PositionEncoding,
    cx: &mut Context<Workspace>,
) {
    let mut by_file: BTreeMap<PathBuf, Vec<lsp_types::Range>> = BTreeMap::new();
    for place in places {
        by_file.entry(place.path).or_default().push(place.range);
    }
    let mut files = Vec::new();
    for (path, ranges) in by_file {
        let Some((text, location)) = text_of(this, &path, cx) else {
            continue;
        };
        let ranges: Vec<_> = ranges
            .into_iter()
            .map(|range| byte_range(&text, range, encoding))
            .collect();
        let lines = line_hits(&text, &ranges);
        let hits = ranges.len();
        files.push(FileResults::new(
            location,
            path.display().to_string(),
            lines,
            hits,
        ));
    }
    let searched = files.len();
    let run = SearchRun::titled(heading, files, searched);
    this.search_results
        .update(cx, |results, cx| results.add(run, cx));
    cx.notify();
}

/// The lines of `ranges` in `text`, each once with all of its ranges.
pub fn line_hits(
    text: &Rope,
    ranges: &[std::ops::Range<usize>],
) -> Vec<birchpad_core::search::LineHit> {
    use birchpad_core::motion::{line_of, line_range};
    use birchpad_core::search::{LineHit, MAX_LINE_SHOWN};
    let mut sorted = ranges.to_vec();
    sorted.sort_by_key(|range| range.start);
    let mut lines: Vec<LineHit> = Vec::new();
    for range in sorted {
        let line = line_of(text, range.start);
        let bounds = line_range(text, line);
        let start = range.start - bounds.start;
        let end = range.end.min(bounds.end).max(range.start) - bounds.start;
        match lines.last_mut() {
            Some(hit) if hit.line == line => {
                if start < hit.text.len() {
                    hit.highlights.push(start..end.min(hit.text.len()));
                }
                hit.matches += 1;
            }
            _ => {
                let mut cut = bounds.end.min(bounds.start + MAX_LINE_SHOWN);
                cut = text.floor_char_boundary(cut);
                let shown = text.slice(bounds.start..cut).to_string();
                let highlights = if start < shown.len() {
                    std::iter::once(start..end.min(shown.len())).collect()
                } else {
                    Vec::new()
                };
                lines.push(LineHit {
                    line,
                    text: shown,
                    highlights,
                    target: start..end,
                    matches: 1,
                });
            }
        }
    }
    lines
}

/// Opens `place`'s file and selects it once the file has been read.
pub fn open_place(
    this: &mut Workspace,
    place: FileRange,
    encoding: PositionEncoding,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    this.open_path(&place.path, window, cx);
    let Some(view) = this.view_for_path(&place.path, cx) else {
        return;
    };
    when_loaded(view, window, cx, move |view, window, cx| {
        let text = view.text(cx).clone();
        let range = byte_range(&text, place.range, encoding);
        view.select_range(range, cx);
        let focus = view.focus_handle.clone();
        window.focus(&focus, cx);
    });
}

/// Runs `then` on `view` once its file has been read (at once if it has).
fn when_loaded(
    view: Entity<EditorView>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    then: impl FnOnce(&mut EditorView, &mut Window, &mut Context<EditorView>) + 'static,
) {
    cx.spawn_in(window, async move |_, cx| {
        let started = std::time::Instant::now();
        loop {
            let loading = view.read_with(cx, |view, cx| {
                view.buffer.read(cx).read_only() == Some(ReadOnly::Loading)
            });
            if !loading || started.elapsed() > LOAD_WAIT {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(20))
                .await;
        }
        view.update_in(cx, then).ok();
    })
    .detach();
}

/// Applies a server's edits of `view`'s text in one undoable step.
fn apply_edits(
    view: &mut EditorView,
    edits: &[lsp_types::TextEdit],
    encoding: PositionEncoding,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    if !view.is_editable(cx) {
        bail!(
            "{}",
            t!(
                "language-servers.cannot-change",
                name = view.buffer.read(cx).display_name()
            )
        );
    }
    let text = view.text(cx).clone();
    let edits = convert::text_edits(&text, edits, encoding)?;
    if edits.is_empty() {
        return Ok(());
    }
    let transaction = Transaction::from_edits(&text, edits)?;
    view.apply_command_edit(transaction, cx);
    Ok(())
}

/// Format Document: the server's edits, with the document's tab size and spaces.
fn format(this: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    if !view.read(cx).is_editable(cx) {
        bail!(
            "{}",
            t!(
                "language-servers.cannot-change",
                name = buffer.read(cx).display_name()
            )
        );
    }
    let target = target(&buffer, cx)?;
    offers(
        &target,
        target.abilities.formatting,
        &t!("language-servers.formatting"),
    )?;
    let settings = view.read(cx).view_settings(cx);
    let revision = buffer.read(cx).doc().revision();
    let encoding = target.abilities.encoding;
    let answer = target.client.request(
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": target.uri.as_str() },
            "options": {
                "tabSize": settings.tab_width,
                "insertSpaces": settings.insert_spaces,
            },
        }),
    );
    cx.spawn_in(window, async move |_, cx| {
        let result = timeout(answer, cx).await;
        cx.update(|window, cx| {
            let edits: Vec<lsp_types::TextEdit> = match result {
                Ok(value) => serde_json::from_value::<Option<Vec<lsp_types::TextEdit>>>(value)
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                Err(error) => {
                    report_error(&anyhow!(error), window, cx);
                    return;
                }
            };
            if buffer.read(cx).doc().revision() != revision {
                report_error(
                    &anyhow!("{}", t!("language-servers.changed-meanwhile")),
                    window,
                    cx,
                );
                return;
            }
            if let Err(error) = view.update(cx, |view, cx| apply_edits(view, &edits, encoding, cx))
            {
                report_error(&error, window, cx);
            }
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// Rename Symbol: the server's edit of every file it names, each in one undoable step, none
/// saved.
fn rename(
    this: &mut Workspace,
    name: String,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        bail!("{}", t!("language-servers.empty-name"));
    }
    let (view, buffer) = active(this, cx)?;
    let target = target(&buffer, cx)?;
    offers(
        &target,
        target.abilities.rename,
        &t!("language-servers.renaming"),
    )?;
    let head = view.read(cx).selection.primary().head;
    let revision = buffer.read(cx).doc().revision();
    let encoding = target.abilities.encoding;
    let mut params = target.document_position(head);
    params["newName"] = json!(name);
    let answer = target.client.request("textDocument/rename", params);
    cx.spawn_in(window, async move |this, cx| {
        let result = timeout(answer, cx).await;
        this.update_in(cx, |this, window, cx| {
            let outcome = result.map_err(|error| anyhow!(error)).and_then(|value| {
                if buffer.read(cx).doc().revision() != revision {
                    bail!("{}", t!("language-servers.changed-meanwhile"));
                }
                let edit: Option<lsp_types::WorkspaceEdit> =
                    serde_json::from_value(value).context("the rename's answer")?;
                let edit = edit.unwrap_or_default();
                apply_workspace_edit(this, &edit, encoding, window, cx)
            });
            match outcome {
                Ok(0) => report_info(t!("language-servers.nothing-renamed"), window, cx),
                Ok(files) => report_info(
                    t!(
                        "language-servers.renamed",
                        name = name,
                        files = birchpad_i18n::count(files, "file", "files")
                    ),
                    window,
                    cx,
                ),
                Err(error) => report_error(&error, window, cx),
            }
        })
        .ok();
    })
    .detach();
    Ok(())
}

/// Applies a workspace edit: open documents change at once, other files are opened and change
/// once read. Refused as a whole when it creates, renames or deletes files, or an open
/// document it changes cannot be changed. The number of files it changes.
fn apply_workspace_edit(
    this: &mut Workspace,
    edit: &lsp_types::WorkspaceEdit,
    encoding: PositionEncoding,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<usize> {
    let files = convert::workspace_edits(edit)?;
    let files: Vec<_> = files
        .into_iter()
        .filter(|(_, edits)| !edits.is_empty())
        .collect();
    for (path, edits) in &files {
        if let Some(view) = this.view_for_path(path, cx) {
            let view = view.read(cx);
            if !view.is_editable(cx) {
                bail!(
                    "{}",
                    t!(
                        "language-servers.cannot-change",
                        name = view.buffer.read(cx).display_name()
                    )
                );
            }
            convert::text_edits(view.text(cx), edits, encoding)?;
        }
    }
    for (path, edits) in &files {
        match this.view_for_path(path, cx) {
            Some(view) => view.update(cx, |view, cx| apply_edits(view, edits, encoding, cx))?,
            None => {
                this.open_path(path, window, cx);
                let Some(view) = this.view_for_path(path, cx) else {
                    continue;
                };
                let edits = edits.clone();
                when_loaded(view, window, cx, move |view, window, cx| {
                    if let Err(error) = apply_edits(view, &edits, encoding, cx) {
                        report_error(&error, window, cx);
                    }
                });
            }
        }
    }
    Ok(files.len())
}

/// The dialog of Rename Symbol: the new name, the word at the caret to start with.
pub struct RenameDialog {
    workspace: WeakEntity<Workspace>,
    pub input: Entity<InputState>,
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The dialog last opened, for tests to drive.
    pub static RENAME_OPENED: std::cell::RefCell<Option<WeakEntity<RenameDialog>>> =
        std::cell::RefCell::default();
}

impl RenameDialog {
    /// Renames to the name typed, and closes.
    pub fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.input.read(cx).value().to_string();
        let invocation =
            birchpad_commands::Invocation::with_args("lsp.rename", json!({ "name": name }));
        window.close_dialog(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.run(&invocation, window, cx))
            .ok();
    }
}

impl Render for RenameDialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(t!("language-servers.new-name"))
            .child(
                div()
                    .debug_selector(|| "rename-name".into())
                    .child(Input::new(&self.input)),
            )
    }
}

fn ask_new_name(
    this: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    // Asked only of a document whose server renames.
    let target = target(&buffer, cx)?;
    offers(
        &target,
        target.abilities.rename,
        &t!("language-servers.renaming"),
    )?;
    let word = word_at(&view, cx);
    let workspace = cx.entity().downgrade();
    let dialog = cx.new(|cx| {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(word));
        cx.subscribe_in(
            &input,
            window,
            |this: &mut RenameDialog, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.confirm(window, cx);
                }
            },
        )
        .detach();
        RenameDialog { workspace, input }
    });
    #[cfg(feature = "test-support")]
    RENAME_OPENED.with(|opened| *opened.borrow_mut() = Some(dialog.downgrade()));
    let confirm = dialog.downgrade();
    window.open_dialog(cx, move |modal, _, _| {
        let confirm = confirm.clone();
        modal
            .title(t!("language-servers.rename-symbol"))
            .w(px(420.))
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_action(
                        Button::new("rename-confirm")
                            .primary()
                            .label(t!("language-servers.rename"))
                            .on_click(move |_, window, cx| {
                                confirm
                                    .update(cx, |dialog, cx| dialog.confirm(window, cx))
                                    .ok();
                            }),
                    ))
                    .child(crate::workspace::dialog_close("common.cancel")),
            )
    });
    Ok(())
}

/// Selects the next (or previous) diagnostic, around the ends, with its message in the call
/// tip.
fn go_to_diagnostic(
    this: &mut Workspace,
    forward: bool,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (view, buffer) = active(this, cx)?;
    let entries = diagnostics(buffer.entity_id(), cx);
    if entries.is_empty() {
        bail!("{}", t!("language-servers.no-diagnostics"));
    }
    let from = view.read(cx).selection.primary().from();
    let found = if forward {
        entries
            .iter()
            .find(|entry| entry.range.start > from)
            .or(entries.first())
    } else {
        entries
            .iter()
            .rev()
            .find(|entry| entry.range.start < from)
            .or(entries.last())
    };
    let entry = found.expect("not empty").clone();
    view.update(cx, |view, cx| {
        view.select_range(entry.range.clone(), cx);
        view.show_tip(describe(entry.level, &entry.message), cx);
    });
    Ok(())
}

/// Tools > Language Server > List Diagnostics: the document's diagnostics in the search
/// results.
fn list_diagnostics(this: &mut Workspace, cx: &mut Context<Workspace>) -> Result<()> {
    let (_, buffer) = active(this, cx)?;
    let entries = diagnostics(buffer.entity_id(), cx);
    if entries.is_empty() {
        bail!("{}", t!("language-servers.no-diagnostics"));
    }
    let text = buffer.read(cx).doc().text().clone();
    let lines = entries
        .iter()
        .map(|entry| {
            let line = birchpad_core::motion::line_of(&text, entry.range.start);
            let bounds = birchpad_core::motion::line_range(&text, line);
            let start = entry.range.start - bounds.start;
            let end = entry.range.end.min(bounds.end).max(entry.range.start) - bounds.start;
            let first_line = entry.message.lines().next().unwrap_or_default();
            let shown = describe(entry.level, first_line);
            let label = shown.len() - first_line.len();
            birchpad_core::search::LineHit {
                line,
                text: shown,
                highlights: std::iter::once(0..label.saturating_sub(2)).collect(),
                target: start..end,
                matches: 1,
            }
        })
        .collect();
    let name = buffer.read(cx).display_name();
    let file = FileResults::new(
        ResultLocation::Buffer(buffer.downgrade()),
        name.clone(),
        lines,
        entries.len(),
    );
    let run = SearchRun::titled(
        &t!("language-servers.diagnostics-of", name = name),
        vec![file],
        1,
    );
    this.search_results
        .update(cx, |results, cx| results.add(run, cx));
    cx.notify();
    Ok(())
}
