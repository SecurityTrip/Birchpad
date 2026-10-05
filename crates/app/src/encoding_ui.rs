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
use crate::commands::CommandRegistry;
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
    birchpad_io::parse_encoding(name, AppState::global(cx).ansi)
        .ok_or_else(|| anyhow!("unknown encoding {name:?}"))
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
        bail!("the file is still being read");
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
    let detail = format!(
        "Reinterpreting reads {} again from disk. Your unsaved changes will be lost.",
        path.display()
    );
    let answer = window.prompt(
        PromptLevel::Warning,
        "Discard changes and reload in another encoding?",
        Some(&detail),
        &["Reload", "Cancel"],
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
        bail!("this document is read-only");
    }
    let selection = view.read(cx).selection.clone();
    buffer.update(cx, |buffer, cx| {
        buffer.set_format(encoding, bom, &selection, cx)
    });
    let text = buffer.read(cx).doc().text().clone();
    if let Err(problem) = birchpad_io::check_encodable(&text, encoding) {
        report_warning(
            format!(
                "{problem}. {}. Saving is blocked until they are removed.",
                first_position(&text, &problem)
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
                "{} was decoded exactly: it can be edited",
                buffer.display_name()
            );
        }
        let encoding = birchpad_io::display_name(buffer.doc().format().encoding, false);
        (
            buffer.display_name(),
            encoding,
            buffer.decode_changes().clone(),
        )
    };
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Edit {name} anyway?"),
        Some(&describe_changes(&changes, &encoding)),
        &["Edit Anyway", "Cancel"],
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
    let places = if changes.count == 1 {
        "1 place".to_owned()
    } else {
        format!("{} places", changes.count)
    };
    let mut text =
        format!("Saving writes the text in {encoding} again, which changes {places} in the file:");
    for change in &changes.first {
        let (offset, before) = (change.offset, hex(&change.before));
        let read_as = if change.text.is_empty() {
            String::new()
        } else {
            format!(" ({})", change.text)
        };
        let line = match &change.after {
            Some(after) if after.is_empty() => format!("offset {offset}: {before} is left out"),
            Some(after) => format!("offset {offset}: {before}{read_as} becomes {}", hex(after)),
            None => format!("offset {offset}: {before}{read_as} cannot be written in {encoding}"),
        };
        text.push_str("\n• ");
        text.push_str(&line);
    }
    let unlisted = changes.count - changes.first.len();
    if unlisted > 0 {
        text.push_str(&format!("\n• and {unlisted} more"));
    }
    if changes.unwritable > 0 {
        text.push_str(&format!(
            "\n\nSaving is refused while the text has characters {encoding} cannot write: \
             replace them, or convert the document to UTF-8."
        ));
    }
    text.push_str("\n\nEverything else is saved as it was, apart from your edits.");
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
    format!("The first one is at Ln {}, Col {}", line + 1, column + 1)
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
            };
            invocation.args.get("eol").and_then(|eol| eol.as_str()) == Some(current)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use birchpad_core::{Encoding, LineEnding};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use serde_json::json;

    use super::*;
    use crate::workspace::tests::{active_text, open_workspace, secondary};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../io/tests/fixtures")
            .join(name)
    }

    fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx).unwrap();
        });
        cx.run_until_parked();
    }

    fn active_format(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Format {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).doc().format()
        })
    }

    fn open(workspace: &Entity<Workspace>, name: &str, cx: &mut VisualTestContext) {
        let path = fixture(name);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(&path, window, cx)
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn opens_files_in_the_background_with_their_format(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, "koi8-r-crlf.txt", cx);
        let format = active_format(&workspace, cx);
        assert_eq!(format.encoding, Encoding::Legacy("KOI8-R"));
        assert_eq!(format.line_ending, LineEnding::CrLf);
        assert!(active_text(&workspace, cx).starts_with("Съешь"));
        // The untouched "new 1" was replaced; opening the file again switches to its tab.
        open(&workspace, "koi8-r-crlf.txt", cx);
        assert_eq!(
            crate::workspace::tests::tab_names(&workspace, cx),
            ["koi8-r-crlf.txt"]
        );
    }

    #[gpui_kit::test]
    fn convert_to_is_undoable(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, "utf-8-lf.txt", cx);
        run(
            &workspace,
            Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16le-bom" })),
            cx,
        );
        let format = active_format(&workspace, cx);
        assert_eq!((format.encoding, format.bom), (Encoding::Utf16Le, true));

        cx.simulate_keystrokes(&secondary("z"));
        let format = active_format(&workspace, cx);
        assert_eq!((format.encoding, format.bom), (Encoding::Utf8, false));
        let modified = workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).is_modified()
        });
        assert!(
            !modified,
            "undoing the conversion returns to the saved state"
        );
    }

    #[gpui_kit::test]
    fn encode_in_rereads_the_bytes_and_bad_bytes_make_it_read_only(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, "invalid-utf8.txt", cx);
        assert!(
            !active_format(&workspace, cx).encoding.is_unicode(),
            "detected as ANSI"
        );

        run(
            &workspace,
            Invocation::with_args("encoding.encode-in", json!({ "encoding": "utf-8" })),
            cx,
        );
        assert_eq!(active_format(&workspace, cx).encoding, Encoding::Utf8);
        let before = active_text(&workspace, cx);
        assert!(before.contains('\u{FFFD}'));
        cx.simulate_input("typed");
        cx.simulate_keystrokes("backspace");
        assert_eq!(active_text(&workspace, cx), before, "read-only");

        let convert = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(
                &Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-8-bom" })),
                window,
                cx,
            )
        });
        assert!(convert.is_err());
    }

    #[gpui_kit::test]
    fn files_that_do_not_decode_can_be_edited_anyway(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.txt");
        std::fs::write(&path, b"ok \xC3( caf\xE9\n").unwrap();
        let (workspace, cx) = open_workspace(cx);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(&path, window, cx)
        });
        cx.run_until_parked();
        run(
            &workspace,
            Invocation::with_args("encoding.encode-in", json!({ "encoding": "utf-8" })),
            cx,
        );
        let changes = workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx).buffer.read(cx).decode_changes().clone()
        });
        assert_eq!(
            changes.first.iter().map(|c| c.offset).collect::<Vec<_>>(),
            [3, 9]
        );

        let edit_anyway = Invocation::new("encoding.edit-anyway");
        run(&workspace, edit_anyway.clone(), cx);
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        cx.simulate_input("typed");
        assert_eq!(active_text(&workspace, cx), "ok \u{FFFD}( caf\u{FFFD}\n");

        run(&workspace, edit_anyway.clone(), cx);
        cx.simulate_prompt_answer("Edit Anyway");
        cx.run_until_parked();
        cx.simulate_input("X");
        cx.simulate_keystrokes(&secondary("s"));
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"Xok \xEF\xBF\xBD( caf\xEF\xBF\xBD\n",
            "only the bytes that did not decode changed, besides the edit"
        );
        let decoded = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&edit_anyway, window, cx)
        });
        assert!(decoded.is_err(), "nothing to accept any more");
    }

    #[test]
    fn the_warning_lists_the_bytes_that_change() {
        let changes =
            birchpad_io::byte_changes(b"a\xFA\x5Bb\x82", 0, Encoding::Legacy("Shift_JIS"));
        assert_eq!(
            describe_changes(&changes, "Shift_JIS"),
            "Saving writes the text in Shift_JIS again, which changes 2 places in the file:\n\
             • offset 1: FA 5B (∵) becomes 81 E6\n\
             • offset 4: 82 (\u{FFFD}) cannot be written in Shift_JIS\n\
             \n\
             Saving is refused while the text has characters Shift_JIS cannot write: replace \
             them, or convert the document to UTF-8.\n\
             \n\
             Everything else is saved as it was, apart from your edits."
        );
    }

    #[gpui_kit::test]
    fn eol_conversion_from_the_menu(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        open(&workspace, "mixed-eol.txt", cx);
        run(
            &workspace,
            Invocation::with_args("edit.convert-eol", json!({ "eol": "lf" })),
            cx,
        );
        assert_eq!(active_text(&workspace, cx), "crlf\nlf\ncr\nlast");
        assert_eq!(active_format(&workspace, cx).line_ending, LineEnding::Lf);
        let checked = is_current(
            &Invocation::with_args("edit.convert-eol", json!({ "eol": "lf" })),
            active_format(&workspace, cx),
            Encoding::Legacy("windows-1252"),
        );
        assert!(checked);
    }
}
