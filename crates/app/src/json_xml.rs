//! Plugins > JSON Viewer and XML Tools (ADR 0039): checking, formatting and putting on one line
//! the selection, or the whole text when nothing is selected.

use anyhow::{Result, anyhow, bail};
use birchpad_core::{Edit, Range, Selection, Transaction};
use birchpad_tools::{Indent, Layout, SyntaxError, json, xml};
use gpui_kit::{Context, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;

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
