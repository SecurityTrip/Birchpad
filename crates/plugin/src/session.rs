//! What a plugin or a script sees of Birchpad during one call: a copy of the document it works
//! on, its edits gathered into one transaction, and what it asks Birchpad to do afterwards.
//!
//! The copy is cheap (a rope clone), and nothing a call does reaches the window until it has
//! returned, so a plugin that fails half way changes nothing but what it already asked for.
//! Beyond the document a call sees what its [`Grants`] give it: the plugin's settings, and
//! files if the user allowed them (ADR 0046).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use birchpad_commands::Invocation;
use birchpad_core::{ChangeSet, Edit, Range, Rope, Selection, Transaction};
use birchpad_i18n::t;

/// The most effects one call may ask for; more are dropped, so a runaway plugin cannot flood
/// the window with messages.
pub const MAX_EFFECTS: usize = 1_000;
/// The most settings a plugin may keep.
pub const MAX_SETTINGS: usize = 256;
/// The longest key of a setting, in bytes.
pub const MAX_SETTING_KEY: usize = 64;
/// The longest value of a setting, in bytes.
pub const MAX_SETTING_VALUE: usize = 64 << 10;
/// The largest file a plugin may read, in bytes.
pub const MAX_FILE_BYTES: u64 = 64 << 20;
/// The most controls a plugin's panel may have.
pub const MAX_CONTROLS: usize = 200;
/// The most tasks one call may start.
pub const MAX_TASKS: usize = 16;

/// A control of a plugin's panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Label(String),
    /// A click tells the plugin, with an empty value.
    Button {
        id: String,
        title: String,
    },
    /// Enter tells the plugin, with the field's text.
    TextField {
        id: String,
        value: String,
    },
    /// A click tells the plugin, with `true` or `false`.
    Checkbox {
        id: String,
        title: String,
        checked: bool,
    },
    Separator,
}

impl Control {
    /// The id the plugin hears about, for the controls that have one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Button { id, .. } | Self::TextField { id, .. } | Self::Checkbox { id, .. } => {
                Some(id)
            }
            Self::Label(_) | Self::Separator => None,
        }
    }
}

/// What a plugin may use beyond the document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    /// The plugin's settings by key, with the defaults of its manifest for those not set.
    pub settings: BTreeMap<String, String>,
    /// Whether the user let the plugin read and write files.
    pub files: bool,
}

/// Whether `key` may name a setting: lower-case letters, digits, dashes, dots and
/// underscores, at most [`MAX_SETTING_KEY`] bytes.
pub fn is_setting_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_SETTING_KEY
        && key.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_')
        })
}

/// Something a call asks Birchpad to do once it returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Run a command, as from the menus.
    RunCommand(Invocation),
    /// Tell the user something.
    Message(String),
    /// Show the caller's panel with these lines.
    Panel { title: String, lines: Vec<String> },
    /// Show the caller's panel with these controls.
    Controls {
        title: String,
        controls: Vec<Control>,
    },
    /// Keep the caller's setting.
    SetSetting { key: String, value: String },
    /// Open the file in a tab.
    OpenFile(PathBuf),
    /// Run the caller's task in the background.
    StartTask { task: String, input: String },
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
    grants: Grants,
    /// The tasks started so far.
    tasks: usize,
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
            grants: Grants::default(),
            tasks: 0,
        }
    }

    /// A session without a document: reading gives nothing, editing is refused.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The session, with what the plugin may use beyond the document.
    pub fn with_grants(mut self, grants: Grants) -> Self {
        self.grants = grants;
        self
    }

    /// Whether the session has a document.
    pub fn has_document(&self) -> bool {
        self.document.is_some()
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

    /// Shows the caller's panel with `controls`. More than [`MAX_CONTROLS`], or two that share
    /// an id, are refused.
    pub fn show_controls(&mut self, title: &str, controls: Vec<Control>) -> Result<(), Refusal> {
        if controls.len() > MAX_CONTROLS {
            return Err(t!(
                "plugin.too-many-controls",
                count = controls.len(),
                limit = MAX_CONTROLS
            ));
        }
        let mut ids: Vec<&str> = controls.iter().filter_map(Control::id).collect();
        ids.sort_unstable();
        if let Some(pair) = ids.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(t!("plugin.control-twice", id = pair[0]));
        }
        self.push(Effect::Controls {
            title: title.to_owned(),
            controls,
        });
        Ok(())
    }

    /// The caller's setting `key`.
    pub fn setting(&self, key: &str) -> Option<String> {
        self.grants.settings.get(key).cloned()
    }

    /// Sets the caller's setting `key`, for this call and the ones after it.
    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<(), Refusal> {
        if !is_setting_key(key) {
            return Err(t!(
                "plugin.not-a-setting-key",
                key = format!("{key:?}"),
                limit = MAX_SETTING_KEY
            ));
        }
        if value.len() > MAX_SETTING_VALUE {
            return Err(t!(
                "plugin.setting-too-long",
                key = key,
                limit = MAX_SETTING_VALUE
            ));
        }
        if !self.grants.settings.contains_key(key) && self.grants.settings.len() >= MAX_SETTINGS {
            return Err(t!("plugin.too-many-settings", limit = MAX_SETTINGS));
        }
        self.grants
            .settings
            .insert(key.to_owned(), value.to_owned());
        self.push(Effect::SetSetting {
            key: key.to_owned(),
            value: value.to_owned(),
        });
        Ok(())
    }

    /// `path`, if the caller may use files and it is absolute.
    fn file_path(&self, path: &str) -> Result<PathBuf, Refusal> {
        if !self.grants.files {
            return Err(t!("plugin.files-not-allowed"));
        }
        let path = Path::new(path);
        if !path.is_absolute() {
            return Err(t!("plugin.path-not-absolute", path = path.display()));
        }
        Ok(path.to_owned())
    }

    /// The text of the file at `path`, which must be UTF-8.
    pub fn read_file(&self, path: &str) -> Result<String, Refusal> {
        let path = self.file_path(path)?;
        let failed = |error: std::io::Error| {
            t!(
                "plugin.cannot-read-file",
                path = path.display(),
                reason = error
            )
        };
        let size = std::fs::metadata(&path).map_err(failed)?.len();
        if size > MAX_FILE_BYTES {
            return Err(t!(
                "plugin.file-too-large",
                path = path.display(),
                limit = MAX_FILE_BYTES
            ));
        }
        let bytes = std::fs::read(&path).map_err(failed)?;
        String::from_utf8(bytes).map_err(|_| t!("plugin.file-not-utf8", path = path.display()))
    }

    /// Writes `text` into the file at `path`, replacing it.
    pub fn write_file(&self, path: &str, text: &str) -> Result<(), Refusal> {
        let path = self.file_path(path)?;
        std::fs::write(&path, text).map_err(|error| {
            t!(
                "plugin.cannot-write-file",
                path = path.display(),
                reason = error
            )
        })
    }

    /// Opens the file at `path` in a tab once the call returns.
    pub fn open_file(&mut self, path: &str) -> Result<(), Refusal> {
        let path = self.file_path(path)?;
        self.push(Effect::OpenFile(path));
        Ok(())
    }

    /// Starts the caller's task `task` in the background once the call returns.
    pub fn start_task(&mut self, task: &str, input: &str) -> Result<(), Refusal> {
        if !crate::manifest::is_valid_id(task) {
            return Err(t!("plugin.not-a-task-id", id = format!("{task:?}")));
        }
        if self.tasks >= MAX_TASKS {
            return Err(t!("plugin.too-many-tasks", limit = MAX_TASKS));
        }
        self.tasks += 1;
        self.push(Effect::StartTask {
            task: task.to_owned(),
            input: input.to_owned(),
        });
        Ok(())
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
