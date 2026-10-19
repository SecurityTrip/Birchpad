//! The Encoding menu: "Encode in" (reinterpret the file's bytes) and "Convert to" (change how
//! the document is saved). See ADR 0007.

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_commands::Invocation;
use birchpad_core::motion::{line_of, line_range};
use birchpad_core::{Encoding, Format, LineEnding, Rope};
use birchpad_io::ByteChanges;
use gpui_kit::{Context, Entity, PromptLevel, Window};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::buffer::{Buffer, ReadOnly};
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::{Workspace, report_warning};

#[derive(Deserialize)]
struct EncodingArgs {
    encoding: String,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace(
        "encoding.encode-in",
        |this, args: EncodingArgs, window, cx| encode_in(this, &args.encoding, window, cx),
    );
    registry.workspace(
        "encoding.convert-to",
        |this, args: EncodingArgs, window, cx| convert_to(this, &args.encoding, window, cx),
    );
    registry.editor("encoding.edit-anyway", |this, (), window, cx| {
        edit_anyway(this, window, cx)
    });
}

fn parse(name: &str, cx: &gpui_kit::App) -> Result<(Encoding, bool)> {
    birchpad_io::parse_encoding(name, AppState::global(cx).ansi).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("encoding-ui.unknown-encoding", name = format!("{:?}", name))
        )
    })
}

fn active_buffer(workspace: &Workspace, cx: &gpui_kit::App) -> Result<Entity<Buffer>> {
    let view = workspace.active_view(cx).context("no document is open")?;
    Ok(view.read(cx).buffer.clone())
}

/// Reinterprets the file's bytes in another encoding by reading the file again.
fn encode_in(
    workspace: &mut Workspace,
    name: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (encoding, bom) = parse(name, cx)?;
    let buffer = active_buffer(workspace, cx)?;
    let (path, modified, loading) = {
        let buffer = buffer.read(cx);
        (
            buffer.path().map(ToOwned::to_owned),
            buffer.is_modified(),
            buffer.loading_progress().is_some(),
        )
    };
    if loading {
        bail!("{}", t!("encoding-ui.the-file-is-still-being-read"));
    }
    let Some(path) = path else {
        // An untitled document has no bytes to reinterpret: choose how it will be saved.
        return convert_to(workspace, name, window, cx);
    };
    let options = AppState::global(cx).load_options(Some(encoding));
    let reload = move |buffer: &Entity<Buffer>, cx: &mut gpui_kit::App| {
        buffer.update(cx, |buffer, cx| buffer.load(options, Some(bom), cx));
    };
    if !modified {
        reload(&buffer, cx);
        return Ok(());
    }
    let detail = t!("encoding-ui.reload-detail", path = path.display());
    let answers = [t!("disk.reload"), t!("common.cancel")];
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    let answer = window.prompt(
        PromptLevel::Warning,
        &t!("encoding-ui.discard-changes-and-reload-in-another-encoding"),
        Some(&detail),
        &answers,
        cx,
    );
    cx.spawn(async move |_, cx| {
        if answer.await == Ok(0) {
            cx.update(|cx| reload(&buffer, cx));
        }
    })
    .detach();
    Ok(())
}

/// Changes the encoding the document is saved in. Undoable.
fn convert_to(
    workspace: &mut Workspace,
    name: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    let (encoding, bom) = parse(name, cx)?;
    let view = workspace.active_view(cx).context("no document is open")?;
    let buffer = view.read(cx).buffer.clone();
    if buffer.read(cx).read_only().is_some() {
        bail!("{}", t!("encoding-ui.this-document-is-read-only"));
    }
    let selection = view.read(cx).selection.clone();
    buffer.update(cx, |buffer, cx| {
        buffer.set_format(encoding, bom, &selection, cx)
    });
    let text = buffer.read(cx).doc().text().clone();
    if let Err(problem) = birchpad_io::check_encodable(&text, encoding) {
        report_warning(
            t!(
                "encoding-ui.unencodable",
                problem = problem.to_string(),
                first = first_position(&text, &problem)
            ),
            window,
            cx,
        );
    }
    Ok(())
}

/// Makes a document that did not decode exactly editable, once the user accepts where saving
/// will change the file's bytes.
fn edit_anyway(
    view: &mut EditorView,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    let buffer = view.buffer.clone();
    let (name, encoding, changes) = {
        let buffer = buffer.read(cx);
        if !matches!(buffer.read_only(), Some(ReadOnly::Decoding(_))) {
            bail!(
                "{}",
                t!(
                    "encoding-ui.was-decoded-exactly-it-can-be-edited",
                    buffer = buffer.display_name()
                )
            );
        }
        let encoding = birchpad_io::display_name(buffer.doc().format().encoding, false);
        (
            buffer.display_name(),
            encoding,
            buffer.decode_changes().clone(),
        )
    };
    let answers = [t!("encoding-ui.edit-anyway"), t!("common.cancel")];
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    let answer = window.prompt(
        PromptLevel::Warning,
        &t!("encoding-ui.edit-question", name = name),
        Some(&describe_changes(&changes, &encoding)),
        &answers,
        cx,
    );
    cx.spawn(async move |_, cx| {
        if answer.await == Ok(0) {
            buffer.update(cx, |buffer, cx| buffer.edit_anyway(cx));
        }
    })
    .detach();
    Ok(())
}

/// Where saving a document that did not decode exactly will change the file, for the user to
/// accept before editing it.
pub(crate) fn describe_changes(changes: &ByteChanges, encoding: &str) -> String {
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let places = crate::i18n::count(changes.count, "place", "places");
    let mut text = t!("encoding-ui.changes", encoding = encoding, places = places);
    for change in &changes.first {
        let (offset, before) = (change.offset, hex(&change.before));
        let read_as = if change.text.is_empty() {
            String::new()
        } else {
            format!(" ({})", change.text)
        };
        let line = match &change.after {
            Some(after) if after.is_empty() => {
                t!(
                    "encoding-ui.change-left-out",
                    offset = offset,
                    before = before
                )
            }
            Some(after) => t!(
                "encoding-ui.change-becomes",
                offset = offset,
                before = before,
                read_as = read_as,
                after = hex(after)
            ),
            None => t!(
                "encoding-ui.change-unwritable",
                offset = offset,
                before = before,
                read_as = read_as,
                encoding = encoding
            ),
        };
        text.push_str("\n• ");
        text.push_str(&line);
    }
    let unlisted = changes.count - changes.first.len();
    if unlisted > 0 {
        text.push_str(&format!("\n• {}", t!("encoding-ui.more", count = unlisted)));
    }
    if changes.unwritable > 0 {
        text.push_str("\n\n");
        text.push_str(&t!("encoding-ui.refused", encoding = encoding));
    }
    text.push_str("\n\n");
    text.push_str(&t!("encoding-ui.rest-kept"));
    text
}

/// "first at Ln 3, Col 7" for an encoding problem.
pub(crate) fn first_position(text: &Rope, problem: &birchpad_io::Unencodable) -> String {
    let Some(&(offset, _)) = problem.samples.first() else {
        return String::new();
    };
    let line = line_of(text, offset);
    let column = text
        .slice(line_range(text, line).start..offset)
        .chars()
        .count();
    t!("encoding-ui.first-at", line = line + 1, column = column + 1)
}

/// Whether a menu item describes the document's current format (for check marks).
pub(crate) fn is_current(invocation: &Invocation, format: Format, ansi: Encoding) -> bool {
    match invocation.command.as_str() {
        "encoding.encode-in" => invocation
            .args
            .get("encoding")
            .and_then(|name| name.as_str())
            .and_then(|name| birchpad_io::parse_encoding(name, ansi))
            .is_some_and(|(encoding, bom)| {
                encoding == format.encoding && (bom == format.bom || !encoding.is_unicode())
            }),
        "edit.convert-eol" => {
            let current = match format.line_ending {
                LineEnding::CrLf => "crlf",
                LineEnding::Lf => "lf",
                LineEnding::Cr => "cr",
                LineEnding::Nel => "nel",
            };
            invocation.args.get("eol").and_then(|eol| eol.as_str()) == Some(current)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../tests/unit/encoding_ui/tests.rs"]
mod tests;
