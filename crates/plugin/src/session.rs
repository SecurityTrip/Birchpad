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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn session(text: &str, anchor: usize, head: usize) -> Session {
        Session::new(DocumentState {
            text: Rope::from(text),
            selection: Selection::single(Range::new(anchor, head)),
            path: Some("a.txt".into()),
            language: "text".into(),
        })
    }

    fn result(session: Session, original: &str) -> (String, Option<Selection>) {
        let outcome = session.finish();
        let mut text = Rope::from(original);
        let Some(transaction) = outcome.transaction else {
            return (original.to_owned(), None);
        };
        transaction.changes().apply(&mut text);
        (text.to_string(), transaction.selection().cloned())
    }

    #[test]
    fn edits_compose_into_one_transaction() {
        let mut s = session("hello world", 0, 5);
        assert_eq!(s.selected_text(), "hello");
        s.replace(0, 5, "HELLO").unwrap();
        s.replace(11, 11, "!").unwrap();
        assert_eq!(s.text(), "HELLO world!");
        assert_eq!(s.text_range(6, 11).unwrap(), "world");
        s.insert("Hi").unwrap();
        assert_eq!(s.text(), "Hi world!");
        assert_eq!(s.len(), 9);
        let (text, selection) = result(s, "hello world");
        assert_eq!(text, "Hi world!");
        assert_eq!(selection.unwrap().primary(), Range::point(2));
    }

    #[test]
    fn sets_the_text_and_the_selections() {
        let mut s = session("abc", 0, 0);
        s.set_text("мир").unwrap();
        s.set_selections(&[(6, 2), (0, 0)]).unwrap();
        assert_eq!(s.selections(), [(6, 2), (0, 0)]);
        assert_eq!(s.selected_text(), "ир");
        let (text, selection) = result(s, "abc");
        assert_eq!(text, "мир");
        assert_eq!(selection.unwrap().ranges().len(), 2);
    }

    #[test]
    fn nothing_done_is_no_transaction() {
        let s = session("abc", 1, 1);
        assert_eq!(s.selections(), [(1, 1)]);
        assert_eq!(s.path().as_deref(), Some("a.txt"));
        assert_eq!(s.language(), "text");
        assert_eq!(s.finish(), Outcome::default());
    }

    #[test]
    fn refuses_bad_ranges_and_changes_nothing() {
        let mut s = session("мир", 0, 0);
        assert!(s.replace(2, 1, "").unwrap_err().contains("reversed"));
        assert!(s.replace(0, 7, "").unwrap_err().contains("past the text"));
        assert!(
            s.replace(1, 1, "")
                .unwrap_err()
                .contains("inside a character")
        );
        assert!(s.text_range(0, 1).is_err());
        assert!(s.set_selections(&[]).is_err());
        assert!(s.set_selections(&[(0, 99)]).is_err());
        assert_eq!(s.text_range(0, 0).unwrap(), "");
        assert_eq!(s.text_range(0, 6).unwrap(), "мир");
        assert_eq!(s.finish(), Outcome::default());
    }

    #[test]
    fn without_a_document_reads_nothing_and_edits_nothing() {
        let mut s = Session::empty();
        assert_eq!(s.text(), "");
        assert!(s.is_empty());
        assert!(s.selections().is_empty());
        assert_eq!(s.selected_text(), "");
        assert_eq!(s.path(), None);
        assert_eq!(s.language(), "");
        assert!(s.replace(0, 0, "x").is_err());
        assert!(s.insert("x").is_err());
        assert!(s.set_text("x").is_err());
        assert!(s.set_selections(&[(0, 0)]).is_err());
        assert!(s.text_range(0, 0).is_err());
        s.message("still talks");
        let outcome = s.finish();
        assert_eq!(outcome.transaction, None);
        assert_eq!(outcome.effects, [Effect::Message("still talks".into())]);
    }

    #[test]
    fn gathers_effects_in_order() {
        let mut s = Session::empty();
        s.run_command("edit.select-all", serde_json::Value::Null)
            .unwrap();
        s.run_command_json("edit.insert-text", r#"{"text":"x"}"#)
            .unwrap();
        s.run_command_json("edit.undo", "  ").unwrap();
        assert!(s.run_command("no.such", json!({})).is_err());
        assert!(s.run_command_json("edit.undo", "{oops").is_err());
        s.show_panel("Found", vec!["one".into()]);
        let effects = s.finish().effects;
        assert_eq!(
            effects,
            [
                Effect::RunCommand(Invocation::new("edit.select-all")),
                Effect::RunCommand(Invocation {
                    command: "edit.insert-text".into(),
                    args: json!({"text": "x"})
                }),
                Effect::RunCommand(Invocation::new("edit.undo")),
                Effect::Panel {
                    title: "Found".into(),
                    lines: vec!["one".into()]
                },
            ]
        );
    }

    #[test]
    fn drops_effects_past_the_limit() {
        let mut s = Session::empty();
        for _ in 0..=MAX_EFFECTS {
            s.message("spam");
        }
        assert_eq!(s.finish().effects.len(), MAX_EFFECTS);
    }
}
