//! Edit menu operations: Line Operations, Blank Operations, Convert Case to, Comment/Uncomment.
//! The work is done by `birchpad_core::ops`; this module turns commands into calls.

use anyhow::Result;
use birchpad_core::ops::{self, Case, CommentTokens, SortKey, Trim};
use birchpad_core::{LineEnding, Rope, Selection, Transaction};
use gpui_kit::Context;
use serde::Deserialize;

use super::{EditorView, LastEdit};
use birchpad_ui::CommandRegistry;

use crate::EditorCommands as _;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SortBy {
    Lexicographic,
    IgnoreCase,
    Integer,
    DecimalComma,
    DecimalDot,
    Length,
}

#[derive(Deserialize)]
struct SortArgs {
    by: SortBy,
    #[serde(default)]
    descending: bool,
}

#[derive(Deserialize, Default)]
struct ConsecutiveArgs {
    #[serde(default)]
    consecutive: bool,
}

#[derive(Deserialize, Default)]
struct BlankArgs {
    #[serde(default)]
    blank: bool,
}

#[derive(Deserialize, Default)]
struct TrimArgs {
    #[serde(default)]
    trim: bool,
}

#[derive(Deserialize, Default)]
struct LeadingArgs {
    #[serde(default)]
    leading: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TrimWhich {
    Trailing,
    Leading,
    Both,
}

#[derive(Deserialize)]
struct TrimWhichArgs {
    which: TrimWhich,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CaseName {
    Upper,
    Lower,
    Proper,
    ProperBlend,
    Sentence,
    SentenceBlend,
    Invert,
    Random,
}

#[derive(Deserialize)]
struct CaseArgs {
    to: CaseName,
}

/// An operation on the lines or selections of a document.
type Op = fn(&Rope, &Selection, LineEnding) -> Option<Transaction>;
/// A comment operation, given the language's comment tokens.
type CommentOp = fn(&Rope, &Selection, CommentTokens) -> Option<Transaction>;

pub fn register_commands(registry: &mut CommandRegistry) {
    let simple: [(&'static str, Op); 6] = [
        ("edit.duplicate-line", ops::duplicate),
        ("edit.delete-line", |text, selection, _| {
            ops::delete_lines(text, selection)
        }),
        ("edit.move-line-up", |text, selection, eol| {
            ops::move_lines(text, selection, true, eol)
        }),
        ("edit.move-line-down", |text, selection, eol| {
            ops::move_lines(text, selection, false, eol)
        }),
        ("edit.insert-line-above", |text, selection, eol| {
            ops::insert_blank_line(text, selection, true, eol)
        }),
        ("edit.insert-line-below", |text, selection, eol| {
            ops::insert_blank_line(text, selection, false, eol)
        }),
    ];
    for (id, op) in simple {
        registry.editor(id, move |this, (), _, cx| {
            this.run_op(cx, op);
            Ok(())
        });
    }
    registry.editor("edit.join-lines", |this, (), _, cx| {
        this.run_op(cx, ops::join_lines);
        Ok(())
    });
    registry.editor("edit.split-lines", |this, (), _, cx| {
        // At the edge while it is shown, else at the width of the view, as in Notepad++.
        let width = this
            .view_settings(cx)
            .split_column()
            .or_else(|| this.visible_columns())
            .unwrap_or(80);
        this.run_op(cx, |text, selection, eol| {
            ops::split_lines(text, selection, width, eol)
        });
        Ok(())
    });
    registry.editor("edit.sort-lines", |this, args: SortArgs, _, cx| {
        let key = match args.by {
            SortBy::Lexicographic => SortKey::Lexicographic,
            SortBy::IgnoreCase => SortKey::IgnoreCase,
            SortBy::Integer => SortKey::Integer,
            SortBy::DecimalComma => SortKey::DecimalComma,
            SortBy::DecimalDot => SortKey::DecimalDot,
            SortBy::Length => SortKey::Length,
        };
        // With a rectangle, lines sort by the text in its columns.
        if this.sort_by_block(key, args.descending, cx)? {
            return Ok(());
        }
        this.try_op(cx, |text, selection, eol| {
            Ok(ops::sort_lines(text, selection, key, args.descending, eol)?)
        })
    });
    registry.editor("edit.reverse-lines", |this, (), _, cx| {
        this.run_op(cx, ops::reverse_lines);
        Ok(())
    });
    registry.editor("edit.shuffle-lines", |this, (), _, cx| {
        let seed = random_seed();
        this.run_op(cx, |text, selection, eol| {
            ops::shuffle_lines(text, selection, seed, eol)
        });
        Ok(())
    });
    registry.editor(
        "edit.remove-duplicate-lines",
        |this, args: Option<ConsecutiveArgs>, _, cx| {
            let consecutive = args.unwrap_or_default().consecutive;
            this.run_op(cx, |text, selection, eol| {
                ops::remove_duplicate_lines(text, selection, consecutive, eol)
            });
            Ok(())
        },
    );
    registry.editor(
        "edit.remove-empty-lines",
        |this, args: Option<BlankArgs>, _, cx| {
            let blank = args.unwrap_or_default().blank;
            this.run_op(cx, |text, selection, eol| {
                ops::remove_empty_lines(text, selection, blank, eol)
            });
            Ok(())
        },
    );
    registry.editor("edit.trim", |this, args: TrimWhichArgs, _, cx| {
        let which = match args.which {
            TrimWhich::Trailing => Trim::Trailing,
            TrimWhich::Leading => Trim::Leading,
            TrimWhich::Both => Trim::Both,
        };
        this.run_op(cx, |text, selection, eol| {
            ops::trim(text, selection, which, eol)
        });
        Ok(())
    });
    registry.editor(
        "edit.eol-to-space",
        |this, args: Option<TrimArgs>, _, cx| {
            let trim = args.unwrap_or_default().trim;
            this.run_op(cx, |text, selection, eol| {
                ops::eol_to_space(text, selection, trim, eol)
            });
            Ok(())
        },
    );
    registry.editor("edit.tabs-to-spaces", |this, (), _, cx| {
        let tab_width = this.view_settings(cx).tab_width;
        this.run_op(cx, |text, selection, eol| {
            ops::tabs_to_spaces(text, selection, tab_width, eol)
        });
        Ok(())
    });
    registry.editor(
        "edit.spaces-to-tabs",
        |this, args: Option<LeadingArgs>, _, cx| {
            let leading = args.unwrap_or_default().leading;
            let tab_width = this.view_settings(cx).tab_width;
            this.run_op(cx, |text, selection, eol| {
                ops::spaces_to_tabs(text, selection, tab_width, leading, eol)
            });
            Ok(())
        },
    );
    registry.editor("edit.convert-case", |this, args: CaseArgs, _, cx| {
        let case = match args.to {
            CaseName::Upper => Case::Upper,
            CaseName::Lower => Case::Lower,
            CaseName::Proper => Case::Proper,
            CaseName::ProperBlend => Case::ProperBlend,
            CaseName::Sentence => Case::Sentence,
            CaseName::SentenceBlend => Case::SentenceBlend,
            CaseName::Invert => Case::Invert,
            CaseName::Random => Case::Random(random_seed()),
        };
        this.run_op(cx, |text, selection, _| {
            ops::convert_case(text, selection, case)
        });
        Ok(())
    });
    let comments: [(&'static str, CommentOp); 4] = [
        ("edit.toggle-comment", ops::toggle_comment),
        ("edit.comment-lines", ops::comment_lines),
        ("edit.uncomment-lines", ops::uncomment_lines),
        ("edit.block-comment", ops::block_comment),
    ];
    for (id, op) in comments {
        registry.editor(id, move |this, (), _, cx| {
            let Some(language) = this.buffer.read(cx).language() else {
                return Ok(());
            };
            let tokens = CommentTokens {
                line: language.line_comment,
                block: language.block_comment,
            };
            this.run_op(cx, |text, selection, _| op(text, selection, tokens));
            Ok(())
        });
    }
}

fn random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_nanos() as u64)
}

impl EditorView {
    /// Runs an operation on the text and the selection as one undo step.
    pub fn run_op(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&Rope, &Selection, LineEnding) -> Option<Transaction>,
    ) {
        let _ = self.try_op(cx, |text, selection, eol| Ok(op(text, selection, eol)));
    }

    /// [`Self::try_op`] for operations elsewhere that need neither the line ending nor more.
    pub fn try_op_public(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&Rope, &Selection) -> Result<Option<Transaction>>,
    ) -> Result<()> {
        self.try_op(cx, |text, selection, _| op(text, selection))
    }

    /// Like `run_op`, for operations that can refuse (Sort as Integers on a line without a
    /// number); the error reaches the user as a notification.
    fn try_op(
        &mut self,
        cx: &mut Context<Self>,
        op: impl FnOnce(&Rope, &Selection, LineEnding) -> Result<Option<Transaction>>,
    ) -> Result<()> {
        if !self.is_editable(cx) {
            return Ok(());
        }
        let doc = self.buffer.read(cx).doc();
        let (text, eol) = (doc.text().clone(), doc.line_ending());
        if let Some(transaction) = op(&text, &self.selection, eol)? {
            self.apply(transaction, LastEdit::None, cx);
        }
        Ok(())
    }
}
