//! The editor's commands: their registration and the caret motions they name.

use anyhow::Result;
use birchpad_commands::Scope;
use birchpad_core::LineEnding;
use birchpad_core::motion::{self};
use birchpad_ui::{AppState, CommandRegistry, CommandTarget};
use gpui_kit::{Context, Window};
use serde::Deserialize;

use crate::{completion, folding, marks, multi, operations};

use super::{EditorView, LastEdit};

impl CommandTarget for EditorView {
    const SCOPE: Scope = Scope::Editor;
}

/// Registering the handlers of editor commands.
pub trait EditorCommands {
    /// Registers an editor command whose arguments decode to `A` (`()` for none).
    fn editor<A: serde::de::DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    );
}

impl EditorCommands for CommandRegistry {
    fn editor<A: serde::de::DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }
}

/// Caret motions; each is registered as `cursor.<name>` and `select.<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

pub(crate) const MOTIONS: [(&str, &str, Motion); 12] = [
    ("cursor.left", "select.left", Motion::Left),
    ("cursor.right", "select.right", Motion::Right),
    ("cursor.up", "select.up", Motion::Up),
    ("cursor.down", "select.down", Motion::Down),
    ("cursor.word-left", "select.word-left", Motion::WordLeft),
    ("cursor.word-right", "select.word-right", Motion::WordRight),
    ("cursor.home", "select.home", Motion::Home),
    ("cursor.end", "select.end", Motion::End),
    ("cursor.page-up", "select.page-up", Motion::PageUp),
    ("cursor.page-down", "select.page-down", Motion::PageDown),
    (
        "cursor.document-start",
        "select.document-start",
        Motion::DocumentStart,
    ),
    (
        "cursor.document-end",
        "select.document-end",
        Motion::DocumentEnd,
    ),
];

#[derive(Deserialize)]
pub(crate) struct InsertTextArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EolName {
    Crlf,
    Lf,
    Cr,
}

#[derive(Deserialize)]
pub(crate) struct ConvertEolArgs {
    eol: EolName,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    completion::register_commands(registry);
    folding::register_commands(registry);
    marks::register_commands(registry);
    multi::register_commands(registry);
    operations::register_commands(registry);
    for (cursor, select, motion) in MOTIONS {
        registry.editor(cursor, move |this, (), _, cx| {
            this.move_carets(motion, false, cx);
            Ok(())
        });
        registry.editor(select, move |this, (), _, cx| {
            this.move_carets(motion, true, cx);
            Ok(())
        });
    }
    registry.editor("edit.undo", |this, (), _, cx| {
        this.undo(cx);
        Ok(())
    });
    registry.editor("edit.redo", |this, (), _, cx| {
        this.redo(cx);
        Ok(())
    });
    registry.editor("edit.copy", |this, (), _, cx| {
        let _ = this.copy(cx);
        Ok(())
    });
    registry.editor("edit.cut", |this, (), _, cx| {
        this.cut(cx);
        Ok(())
    });
    registry.editor("edit.paste", |this, (), _, cx| {
        this.paste(cx);
        Ok(())
    });
    registry.editor("edit.delete", |this, (), _, cx| {
        this.delete_with(cx, motion::next_boundary);
        Ok(())
    });
    registry.editor("edit.backspace", |this, (), _, cx| {
        // Preferences > Auto-Completion > Backspace unindents: in the indentation, the
        // line goes out a level instead of losing a space.
        if this.in_indentation(cx) && AppState::global(cx).settings.editor.backspace_unindents {
            this.indent(false, cx);
            return Ok(());
        }
        if !this.backspace_pair(cx) {
            this.delete_with(cx, motion::prev_boundary);
        }
        let head = this.selection.primary().head;
        if this.call_tip.as_ref().is_some_and(|tip| head < tip.open) {
            this.call_tip = None;
        }
        if this.completion.is_some() {
            this.update_completion(false, cx);
        }
        Ok(())
    });
    registry.editor("edit.delete-word-left", |this, (), _, cx| {
        this.delete_with(cx, motion::word_left);
        Ok(())
    });
    registry.editor("edit.delete-word-right", |this, (), _, cx| {
        this.delete_with(cx, motion::word_right);
        Ok(())
    });
    registry.editor("edit.expand-selection", |this, (), _, cx| {
        this.expand_selection(cx);
        Ok(())
    });
    registry.editor("edit.shrink-selection", |this, (), _, cx| {
        this.shrink_selection(cx);
        Ok(())
    });
    registry.editor("edit.select-all", |this, (), _, cx| {
        this.select_all(cx);
        Ok(())
    });
    registry.editor("edit.newline", |this, (), _, cx| {
        this.newline(cx);
        Ok(())
    });
    registry.editor("search.go-to-matching-brace", |this, (), _, cx| {
        this.go_to_matching_brace(false, cx);
        Ok(())
    });
    registry.editor("search.select-to-matching-brace", |this, (), _, cx| {
        this.go_to_matching_brace(true, cx);
        Ok(())
    });
    registry.editor("edit.tab", |this, (), _, cx| {
        // A selection over several lines indents them, as in Notepad++.
        let text = this.text(cx).clone();
        let several_lines = this.selection.iter().any(|range| {
            !range.is_empty() && birchpad_core::ops::lines_of(&text, *range).len() > 1
        });
        if several_lines {
            this.indent(true, cx);
        } else {
            this.insert_tab(cx);
        }
        Ok(())
    });
    registry.editor("edit.indent", |this, (), _, cx| {
        this.indent(true, cx);
        Ok(())
    });
    registry.editor("edit.outdent", |this, (), _, cx| {
        this.indent(false, cx);
        Ok(())
    });
    registry.editor("edit.insert-text", |this, args: InsertTextArgs, _, cx| {
        this.insert(&args.text, LastEdit::None, cx);
        Ok(())
    });
    registry.editor("edit.type", |this, args: InsertTextArgs, _, cx| {
        this.type_text(&args.text, cx);
        Ok(())
    });
    registry.editor("edit.text-direction-rtl", |this, (), _, cx| {
        this.rtl = true;
        this.settings_changed(cx);
        Ok(())
    });
    registry.editor("edit.text-direction-ltr", |this, (), _, cx| {
        this.rtl = false;
        this.settings_changed(cx);
        Ok(())
    });
    registry.editor("edit.toggle-overwrite", |this, (), _, cx| {
        this.overwrite = !this.overwrite;
        this.pause_blink(cx);
        cx.notify();
        Ok(())
    });
    registry.editor("edit.convert-eol", |this, args: ConvertEolArgs, _, cx| {
        let target = match args.eol {
            EolName::Crlf => LineEnding::CrLf,
            EolName::Lf => LineEnding::Lf,
            EolName::Cr => LineEnding::Cr,
        };
        let transaction = this.buffer.read(cx).doc().convert_line_endings(target);
        this.apply(transaction, LastEdit::None, cx);
        Ok(())
    });
}
