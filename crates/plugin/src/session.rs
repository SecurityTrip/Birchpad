//! What a plugin or a script sees of Birchpad during one call: a copy of the document it works
//! on, its edits gathered into one transaction, and what it asks Birchpad to do afterwards.
//!
//! The copy is cheap (a rope clone), and nothing a call does reaches the window until it has
//! returned, so a plugin that fails half way changes nothing but what it already asked for.

use birchpad_commands::Invocation;
use birchpad_core::{ChangeSet, Edit, Range, Rope, Selection, Transaction};
use birchpad_i18n::t;

/// The most effects one call may ask for; more are dropped, so a runaway plugin cannot flood
/// the window with messages.
pub const MAX_EFFECTS: usize = 1_000;

/// Something a call asks Birchpad to do once it returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Run a command, as from the menus.
    RunCommand(Invocation),
    /// Tell the user something.
    Message(String),
    /// Show the caller's panel with these lines.
    Panel { title: String, lines: Vec<String> },
}

/// What a call did: the edit to apply to the document (one undo step), and the effects.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub transaction: Option<Transaction>,
    pub effects: Vec<Effect>,
}

/// The document a call works on.
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub text: Rope,
    pub selection: Selection,
    pub path: Option<String>,
    pub language: String,
}

#[derive(Debug)]
struct Working {
    text: Rope,
    selection: Selection,
    /// Every edit so far, composed.
    changes: ChangeSet,
    selection_set: bool,
    path: Option<String>,
    language: String,
}

/// One call's view of Birchpad.
#[derive(Debug, Default)]
pub struct Session {
    document: Option<Working>,
    effects: Vec<Effect>,
}

/// Why a call cannot do what it asked.
pub type Refusal = String;

impl Session {
    /// A session over `document`.
    pub fn new(document: DocumentState) -> Self {
        let changes = ChangeSet::identity(document.text.len());
        Self {
            document: Some(Working {
                text: document.text,
                selection: document.selection,
                changes,
                selection_set: false,
                path: document.path,
                language: document.language,
            }),
            effects: Vec::new(),
        }
    }

    /// A session without a document: reading gives nothing, editing is refused.
    pub fn empty() -> Self {
        Self::default()
    }

    fn working(&mut self) -> Result<&mut Working, Refusal> {
        self.document
            .as_mut()
            .ok_or_else(|| t!("plugin.no-document"))
    }

    pub fn text(&self) -> String {
        self.document
            .as_ref()
            .map(|doc| doc.text.to_string())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.document.as_ref().map_or(0, |doc| doc.text.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn check_range(text: &Rope, start: usize, end: usize) -> Result<(), Refusal> {
        if start > end {
            return Err(t!("plugin.range-reversed", start = start, end = end));
        }
        if end > text.len() {
            return Err(format!(
                "the range {start}..{end} ends past the text ({} bytes)",
                text.len()
            ));
        }
        for pos in [start, end] {
            if !text.is_char_boundary(pos) {
                return Err(t!("plugin.inside-a-character", position = pos));
            }
        }
        Ok(())
    }

    /// The text from `start` to `end`.
    pub fn text_range(&self, start: usize, end: usize) -> Result<String, Refusal> {
        let Some(doc) = &self.document else {
            return Err(t!("plugin.no-document"));
        };
        Self::check_range(&doc.text, start, end)?;
        Ok(doc.text.slice(start..end).to_string())
    }

    fn apply(doc: &mut Working, changes: ChangeSet, selection: Option<Selection>) {
        doc.selection = selection.unwrap_or_else(|| doc.selection.map(&changes));
        changes.apply(&mut doc.text);
        let total = std::mem::replace(&mut doc.changes, ChangeSet::identity(0));
        doc.changes = total.compose(changes);
    }

    /// Replaces the text from `start` to `end` with `text`.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) -> Result<(), Refusal> {
        let doc = self.working()?;
        Self::check_range(&doc.text, start, end)?;
        let changes = ChangeSet::from_edits(&doc.text, [Edit::replace(start..end, text)])
            .map_err(|error| error.to_string())?;
        Self::apply(doc, changes, None);
        Ok(())
    }

    /// Replaces the whole text.
    pub fn set_text(&mut self, text: &str) -> Result<(), Refusal> {
        let len = self.len();
        self.replace(0, len, text)
    }

    /// Replaces every selection with `text`, leaving a caret after each.
    pub fn insert(&mut self, text: &str) -> Result<(), Refusal> {
        let doc = self.working()?;
        let transaction = Transaction::replace_selections(&doc.text, &doc.selection, text)
            .map_err(|error| error.to_string())?;
        let selection = transaction.selection().cloned();
        Self::apply(doc, transaction.changes().clone(), selection);
        Ok(())
    }

    /// The selections as `(anchor, head)`, the primary one first.
    pub fn selections(&self) -> Vec<(usize, usize)> {
        let Some(doc) = &self.document else {
            return Vec::new();
        };
        let primary = doc.selection.primary();
        std::iter::once(primary)
            .chain(
                doc.selection
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != doc.selection.primary_index())
                    .map(|(_, range)| *range),
            )
            .map(|range| (range.anchor, range.head))
            .collect()
    }

    /// The text of the primary selection.
    pub fn selected_text(&self) -> String {
        let Some(doc) = &self.document else {
            return String::new();
        };
        let range = doc.selection.primary();
        doc.text.slice(range.from()..range.to()).to_string()
    }

    /// Sets the selections from `(anchor, head)` pairs, the primary one first.
    pub fn set_selections(&mut self, ranges: &[(usize, usize)]) -> Result<(), Refusal> {
        let doc = self.working()?;
        if ranges.is_empty() {
            return Err(t!("plugin.selection-needs-a-range"));
        }
        for &(anchor, head) in ranges {
            Self::check_range(&doc.text, anchor.min(head), anchor.max(head))?;
        }
        doc.selection = Selection::new(
            ranges
                .iter()
                .map(|&(anchor, head)| Range::new(anchor, head)),
            0,
        );
        doc.selection_set = true;
        Ok(())
    }

    pub fn path(&self) -> Option<String> {
        self.document.as_ref().and_then(|doc| doc.path.clone())
    }

    pub fn language(&self) -> String {
        self.document
            .as_ref()
            .map(|doc| doc.language.clone())
            .unwrap_or_default()
    }

    fn push(&mut self, effect: Effect) {
        if self.effects.len() < MAX_EFFECTS {
            self.effects.push(effect);
        }
    }

    /// Asks to run the command `id` with `args` once the call returns. Unknown commands are
    /// refused at once.
    pub fn run_command(&mut self, id: &str, args: serde_json::Value) -> Result<(), Refusal> {
        if birchpad_commands::find(id).is_none() {
            return Err(t!("plugin.no-command", id = id));
        }
        self.push(Effect::RunCommand(Invocation {
            command: id.to_owned(),
            args,
        }));
        Ok(())
    }

    /// Like [`Self::run_command`], with the arguments in JSON (empty for none).
    pub fn run_command_json(&mut self, id: &str, args: &str) -> Result<(), Refusal> {
        let args = if args.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(args)
                .map_err(|error| t!("plugin.arguments-not-json", id = id, reason = error))?
        };
        self.run_command(id, args)
    }

    pub fn message(&mut self, text: &str) {
        self.push(Effect::Message(text.to_owned()));
    }

    pub fn show_panel(&mut self, title: &str, lines: Vec<String>) {
        self.push(Effect::Panel {
            title: title.to_owned(),
            lines,
        });
    }

    /// What the call did.
    pub fn finish(self) -> Outcome {
        let transaction = self.document.and_then(|doc| {
            if doc.changes.is_identity() && !doc.selection_set {
                return None;
            }
            Some(Transaction::new(doc.changes).with_selection(doc.selection))
        });
        Outcome {
            transaction,
            effects: self.effects,
        }
    }
}
