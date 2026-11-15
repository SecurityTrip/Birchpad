//! Tools > Language Server (ADR 0065): a server per language and root folder, started for the
//! documents of a language that has one in `lsp.servers` and stopped with the last of them;
//! the documents kept in step with it; its diagnostics underlined, its completions in the
//! auto-completion list. The commands that ask it are the application's.
//!
//! The servers belong to the application, the documents to the windows: each window tells
//! [`sync_documents`] which documents it has whenever they may have changed (opened, closed,
//! saved, read again, given another language, the settings changed), and [`edited`] after each
//! edit.

#[macro_use]
extern crate birchpad_i18n;

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use birchpad_core::{Assoc, RevisionId, Rope, Transaction};
use birchpad_editor::buffer::{Buffer, ReadOnly};
use birchpad_editor::{DiagnosticLevel, Diagnostics, EditorView};
use birchpad_lsp::convert::{Severity, diagnostic_message};
use birchpad_lsp::lsp_types::{self, Uri};
use birchpad_lsp::position::{byte_range, position};
use birchpad_lsp::{Abilities, Client, CommandLine, RequestError, ServerEvent, Transport};
use futures::StreamExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, EntityId, Global, Task,
    WeakEntity,
};
use serde_json::json;

use birchpad_ui::AppState;

/// How long a document's edits wait before they are sent, so that typing sends the text once
/// a pause, not once a key.
pub const CHANGE_DELAY: Duration = Duration::from_millis(150);
/// How long a server has to answer a request.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What starts a server: the process of its command line, or a scripted one in tests.
pub type Launcher = Rc<dyn Fn(&CommandLine, &Path) -> std::io::Result<Transport>>;

/// A server: the language it serves and the folder it works in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerKey {
    pub language: &'static str,
    pub root: PathBuf,
}

/// Where a server is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    /// Started, not initialized yet.
    Starting,
    Ready(Abilities),
    /// It could not start, did not initialize, or exited: why.
    Failed(String),
}

struct Server {
    /// Tells this server from one started later with the same key.
    id: u64,
    command: String,
    client: Option<Client>,
    state: ServerState,
    /// The window that started it, where its failures are told.
    window: Option<AnyWindowHandle>,
    _tasks: Vec<Task<()>>,
}

/// A document a server knows of, or is to.
struct Doc {
    workspace: EntityId,
    buffer: WeakEntity<Buffer>,
    key: ServerKey,
    path: PathBuf,
    uri: Uri,
    version: i32,
    /// `didOpen` was sent to the server running now.
    opened: bool,
    /// The revision whose text the server has.
    sent: Option<RevisionId>,
    /// The revision last saved, as the server was told.
    saved: Option<RevisionId>,
    /// Edits waiting to be sent.
    flush: Option<Task<()>>,
}

/// A diagnostic of a document: its range moves with the document's edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocDiagnostic {
    pub range: Range<usize>,
    pub level: DiagnosticLevel,
    pub message: String,
}

#[derive(Default)]
struct DocDiagnostics {
    entries: Vec<DocDiagnostic>,
    drawn: Diagnostics,
}

impl DocDiagnostics {
    fn new(mut entries: Vec<DocDiagnostic>) -> Self {
        entries.sort_by_key(|entry| (entry.range.start, entry.range.end));
        let drawn = Rc::new(
            entries
                .iter()
                .map(|entry| (entry.range.clone(), entry.level))
                .collect(),
        );
        Self { entries, drawn }
    }
}

/// The servers, the documents they know of, and their diagnostics.
#[derive(Default)]
pub struct LanguageServers {
    servers: HashMap<ServerKey, Server>,
    docs: HashMap<EntityId, Doc>,
    diagnostics: HashMap<EntityId, DocDiagnostics>,
    launcher: Option<Launcher>,
    next_id: u64,
}

impl Global for LanguageServers {}

fn servers(cx: &mut App) -> &mut LanguageServers {
    if !cx.has_global::<LanguageServers>() {
        cx.set_global(LanguageServers::default());
    }
    cx.global_mut::<LanguageServers>()
}

/// Starts servers with `launcher` instead of as processes (tests).
pub fn set_launcher(launcher: Launcher, cx: &mut App) {
    servers(cx).launcher = Some(launcher);
}

/// The state of the server of `key`, if one was started.
pub fn server_state(key: &ServerKey, cx: &App) -> Option<ServerState> {
    cx.try_global::<LanguageServers>()?
        .servers
        .get(key)
        .map(|server| server.state.clone())
}

/// The servers started, by key.
pub fn server_keys(cx: &App) -> Vec<ServerKey> {
    let mut keys: Vec<ServerKey> = cx
        .try_global::<LanguageServers>()
        .map(|servers| servers.servers.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort_by(|a, b| (a.language, &a.root).cmp(&(b.language, &b.root)));
    keys
}

/// The server a buffer should have: none while it is read, for an untitled document, one
/// without a language, a user defined language or a large file, or a language without a
/// server in `lsp.servers`.
pub fn wanted(buffer: &Buffer, cx: &App) -> Option<(ServerKey, String)> {
    if buffer.read_only() == Some(ReadOnly::Loading)
        || buffer.user_language().is_some()
        || buffer.is_large(cx)
    {
        return None;
    }
    let path = buffer.path()?;
    let language = buffer.language()?;
    let command = AppState::global(cx).settings.lsp.server(language.id)?;
    let root = birchpad_lsp::languages::root_folder(path)?;
    Some((
        ServerKey {
            language: language.id,
            root,
        },
        command.to_owned(),
    ))
}

/// Brings the servers in step with the documents of a window, `documents`: new ones opened
/// (and their servers started), closed ones closed, saves told, servers whose command changed
/// restarted, and servers left without documents stopped. `owner` is the window's workspace.
pub fn sync_documents(
    owner: EntityId,
    documents: impl IntoIterator<Item = Entity<Buffer>>,
    window: AnyWindowHandle,
    cx: &mut App,
) {
    let mut wanted_docs: Vec<(Entity<Buffer>, ServerKey, String)> = Vec::new();
    for buffer in documents {
        if wanted_docs
            .iter()
            .any(|(known, _, _)| known.entity_id() == buffer.entity_id())
        {
            continue;
        }
        if let Some((key, command)) = wanted(buffer.read(cx), cx) {
            wanted_docs.push((buffer, key, command));
        }
    }
    reconcile(owner, wanted_docs, Some(window), cx);
}

/// A window closed: its documents are closed, and servers left without any stop.
pub fn forget_workspace(workspace: EntityId, cx: &mut App) {
    reconcile(workspace, Vec::new(), None, cx);
}

fn reconcile(
    workspace: EntityId,
    wanted_docs: Vec<(Entity<Buffer>, ServerKey, String)>,
    window: Option<AnyWindowHandle>,
    cx: &mut App,
) {
    if !cx.has_global::<LanguageServers>() && wanted_docs.is_empty() {
        return;
    }
    // Documents gone, or now of another server.
    let stale: Vec<EntityId> = servers(cx)
        .docs
        .iter()
        .filter(|(id, doc)| {
            doc.workspace == workspace
                && !wanted_docs
                    .iter()
                    .any(|(buffer, key, _)| buffer.entity_id() == **id && *key == doc.key)
        })
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        close_doc(id, cx);
    }
    // Servers whose command line changed, or whose language no longer has one.
    let settings = AppState::global(cx).settings.lsp.clone();
    let changed: Vec<ServerKey> = servers(cx)
        .servers
        .iter()
        .filter(|(key, server)| settings.server(key.language) != Some(server.command.as_str()))
        .map(|(key, _)| key.clone())
        .collect();
    for key in changed {
        stop_server(&key, cx);
    }
    for (buffer, key, command) in wanted_docs {
        let id = buffer.entity_id();
        if !servers(cx).docs.contains_key(&id) {
            let Some(path) = buffer.read(cx).path().map(Path::to_owned) else {
                continue;
            };
            let Some(uri) = birchpad_lsp::uri::file_uri(&path) else {
                continue;
            };
            servers(cx).docs.insert(
                id,
                Doc {
                    workspace,
                    buffer: buffer.downgrade(),
                    key: key.clone(),
                    path,
                    uri,
                    version: 0,
                    opened: false,
                    sent: None,
                    saved: None,
                    flush: None,
                },
            );
        }
        ensure_server(&key, &command, window, cx);
        refresh_doc(id, cx);
    }
    stop_unused(cx);
}

/// Starts the server of `key` unless one runs (or failed: it is not started again by itself).
fn ensure_server(key: &ServerKey, command: &str, window: Option<AnyWindowHandle>, cx: &mut App) {
    if servers(cx).servers.contains_key(key) {
        return;
    }
    let state = servers(cx);
    state.next_id += 1;
    let id = state.next_id;
    let launcher = state.launcher.clone();
    let started = CommandLine::parse(command)
        .map_err(|error| error.to_string())
        .and_then(|line| {
            let result = match &launcher {
                Some(launch) => launch(&line, &key.root),
                None => Transport::spawn(&line, &key.root),
            };
            result.map_err(|error| {
                t!(
                    "language-servers.cannot-start",
                    program = line.program,
                    reason = error
                )
            })
        });
    let server = match started {
        Err(reason) => Server {
            id,
            command: command.to_owned(),
            client: None,
            state: ServerState::Failed(reason.clone()),
            window,
            _tasks: Vec::new(),
        },
        Ok(transport) => {
            let (client, mut events) = Client::start(transport);
            let initializing = {
                let client = client.clone();
                let key = key.clone();
                cx.spawn(async move |cx| {
                    let root = key.root.clone();
                    let result = timeout(
                        birchpad_lsp::capabilities::initialize(
                            &client,
                            &root,
                            Some(std::process::id()),
                        ),
                        cx,
                    )
                    .await;
                    cx.update(|cx| initialized(&key, id, result, cx));
                })
            };
            let listening = {
                let key = key.clone();
                cx.spawn(async move |cx| {
                    while let Some(event) = events.next().await {
                        cx.update(|cx| on_event(&key, id, event, cx));
                    }
                })
            };
            Server {
                id,
                command: command.to_owned(),
                client: Some(client),
                state: ServerState::Starting,
                window,
                _tasks: vec![initializing, listening],
            }
        }
    };
    let failed = match &server.state {
        ServerState::Failed(reason) => Some(reason.clone()),
        _ => None,
    };
    servers(cx).servers.insert(key.clone(), server);
    if let Some(reason) = failed {
        report_failure(key, &reason, cx);
    }
}

/// `future`, or a timeout after [`REQUEST_TIMEOUT`].
pub async fn timeout<T>(
    future: impl Future<Output = Result<T, RequestError>>,
    cx: &AsyncApp,
) -> Result<T, String> {
    let timer = cx.background_executor().timer(REQUEST_TIMEOUT);
    match futures::future::select(Box::pin(future), timer).await {
        futures::future::Either::Left((result, _)) => result.map_err(|error| error.to_string()),
        futures::future::Either::Right(_) => Err(t!(
            "language-servers.no-answer",
            seconds = REQUEST_TIMEOUT.as_secs()
        )),
    }
}

fn initialized(key: &ServerKey, id: u64, result: Result<Abilities, String>, cx: &mut App) {
    let Some(server) = servers(cx)
        .servers
        .get_mut(key)
        .filter(|server| server.id == id)
    else {
        return;
    };
    match result {
        Ok(abilities) => {
            server.state = ServerState::Ready(abilities);
            let docs: Vec<EntityId> = servers(cx)
                .docs
                .iter()
                .filter(|(_, doc)| doc.key == *key)
                .map(|(id, _)| *id)
                .collect();
            for doc in docs {
                refresh_doc(doc, cx);
            }
        }
        Err(reason) => {
            server.state = ServerState::Failed(reason.clone());
            if let Some(client) = server.client.take() {
                client.stop();
            }
            report_failure(key, &reason, cx);
        }
    }
    cx.refresh_windows();
}

fn on_event(key: &ServerKey, id: u64, event: ServerEvent, cx: &mut App) {
    if !servers(cx)
        .servers
        .get(key)
        .is_some_and(|server| server.id == id)
    {
        return;
    }
    match event {
        ServerEvent::Notification { method, params } => match method.as_str() {
            "textDocument/publishDiagnostics" => {
                if let Some(published) =
                    birchpad_lsp::client::params::<lsp_types::PublishDiagnosticsParams>(&params)
                {
                    publish(key, published, cx);
                }
            }
            "window/showMessage" => {
                if let Some(shown) =
                    birchpad_lsp::client::params::<lsp_types::ShowMessageParams>(&params)
                {
                    let serious = shown.typ == lsp_types::MessageType::ERROR
                        || shown.typ == lsp_types::MessageType::WARNING;
                    if serious {
                        let message = format!("{}: {}", key.language, shown.message);
                        tell(key, message, cx);
                    } else {
                        eprintln!("{} language server: {}", key.language, shown.message);
                    }
                }
            }
            "window/logMessage" => {
                if let Some(logged) =
                    birchpad_lsp::client::params::<lsp_types::LogMessageParams>(&params)
                {
                    eprintln!("{} language server: {}", key.language, logged.message);
                }
            }
            _ => {}
        },
        ServerEvent::Exited { reason } => {
            let state = servers(cx);
            if let Some(server) = state.servers.get_mut(key) {
                server.state = ServerState::Failed(reason.clone());
                server.client = None;
            }
            let docs: Vec<EntityId> = state
                .docs
                .iter_mut()
                .filter(|(_, doc)| doc.key == *key)
                .map(|(id, doc)| {
                    doc.opened = false;
                    doc.flush = None;
                    *id
                })
                .collect();
            for doc in docs {
                state.diagnostics.remove(&doc);
            }
            report_failure(key, &reason, cx);
            cx.refresh_windows();
        }
    }
}

/// Tells the window that started the server of `key` why it is not running.
fn report_failure(key: &ServerKey, reason: &str, cx: &mut App) {
    let message = t!(
        "language-servers.server-failed",
        language = key.language,
        reason = reason
    );
    tell(key, message, cx);
}

fn tell(key: &ServerKey, message: String, cx: &mut App) {
    let window = servers(cx)
        .servers
        .get(key)
        .and_then(|server| server.window);
    match window {
        // Once the window is no longer being updated: a server may fail while it syncs.
        Some(window) => cx.defer(move |cx| {
            let _ = cx.update_window(window, |_, window, cx| {
                birchpad_ui::report_warning(message, window, cx);
            });
        }),
        None => eprintln!("{message}"),
    }
}

/// The server's diagnostics of a document, as byte ranges of its text now.
fn publish(key: &ServerKey, published: lsp_types::PublishDiagnosticsParams, cx: &mut App) {
    let Some(path) = birchpad_lsp::uri::file_path(&published.uri) else {
        return;
    };
    let encoding = match server_state(key, cx) {
        Some(ServerState::Ready(abilities)) => abilities.encoding,
        _ => return,
    };
    let found = servers(cx)
        .docs
        .iter()
        .find(|(_, doc)| doc.key == *key && same_path(&doc.path, &path))
        .and_then(|(id, doc)| Some((*id, doc.buffer.upgrade()?)));
    let Some((id, buffer)) = found else {
        return;
    };
    let text = buffer.read(cx).doc().text().clone();
    let entries = published
        .diagnostics
        .iter()
        .map(|diagnostic| DocDiagnostic {
            range: shown_range(&text, byte_range(&text, diagnostic.range, encoding)),
            level: level(Severity::of(diagnostic.severity)),
            message: diagnostic_message(diagnostic),
        })
        .collect();
    servers(cx)
        .diagnostics
        .insert(id, DocDiagnostics::new(entries));
    cx.refresh_windows();
}

/// A range wide enough to underline: an empty one takes the character after it, or before it
/// at the end of a line.
fn shown_range(text: &Rope, range: Range<usize>) -> Range<usize> {
    if !range.is_empty() {
        return range;
    }
    let at = range.start;
    let line = birchpad_core::motion::line_range(text, birchpad_core::motion::line_of(text, at));
    if at < line.end {
        at..birchpad_core::motion::next_boundary(text, at)
    } else if at > line.start {
        birchpad_core::motion::prev_boundary(text, at)..at
    } else {
        range
    }
}

fn level(severity: Severity) -> DiagnosticLevel {
    match severity {
        Severity::Error => DiagnosticLevel::Error,
        Severity::Warning => DiagnosticLevel::Warning,
        Severity::Information => DiagnosticLevel::Information,
        Severity::Hint => DiagnosticLevel::Hint,
    }
}

/// Whether two paths name the same file: on Windows without regard to case.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
    } else {
        a == b
    }
}

/// Sends what the server should know of a document: `didOpen` once the server is ready, its
/// edits not sent yet, and `didSave` after it was saved.
fn refresh_doc(id: EntityId, cx: &mut App) {
    let Some(doc) = servers(cx).docs.get(&id) else {
        return;
    };
    let Some(buffer) = doc.buffer.upgrade() else {
        close_doc(id, cx);
        return;
    };
    let key = doc.key.clone();
    let (client, abilities) = match servers(cx).servers.get(&key) {
        Some(Server {
            client: Some(client),
            state: ServerState::Ready(abilities),
            ..
        }) => (client.clone(), abilities.clone()),
        _ => return,
    };
    if !abilities.sync {
        return;
    }
    let (revision, modified, text) = {
        let buffer = buffer.read(cx);
        (
            buffer.doc().revision(),
            buffer.is_modified(),
            buffer.doc().text().clone(),
        )
    };
    let doc = servers(cx).docs.get_mut(&id).expect("looked up above");
    if !doc.opened {
        doc.version += 1;
        doc.opened = true;
        doc.sent = Some(revision);
        doc.saved = (!modified).then_some(revision);
        client.notify(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": doc.uri.as_str(),
                "languageId": birchpad_lsp::languages::protocol_language_id(key.language),
                "version": doc.version,
                "text": text.to_string(),
            }}),
        );
        return;
    }
    // Edits wait for their pause (`edited`); a save or a reading from disk is told at once,
    // the text first.
    if modified {
        return;
    }
    send_changes(id, cx);
    let doc = servers(cx).docs.get_mut(&id).expect("looked up above");
    if doc.saved != Some(revision) {
        doc.saved = Some(revision);
        if let Some(include_text) = abilities.save {
            let mut params = json!({ "textDocument": { "uri": doc.uri.as_str() } });
            if include_text {
                params["text"] = json!(text.to_string());
            }
            client.notify("textDocument/didSave", params);
        }
    }
}

/// Sends a document's text if the server does not have it yet.
fn send_changes(id: EntityId, cx: &mut App) {
    let Some((buffer, key)) = servers(cx)
        .docs
        .get(&id)
        .map(|doc| (doc.buffer.clone(), doc.key.clone()))
    else {
        return;
    };
    let Some(buffer) = buffer.upgrade() else {
        return;
    };
    let Some(client) = servers(cx)
        .servers
        .get(&key)
        .and_then(|server| server.client.clone())
    else {
        return;
    };
    let (revision, text) = {
        let buffer = buffer.read(cx);
        (buffer.doc().revision(), buffer.doc().text().clone())
    };
    let doc = servers(cx).docs.get_mut(&id).expect("looked up above");
    doc.flush = None;
    if !doc.opened || doc.sent == Some(revision) {
        return;
    }
    doc.version += 1;
    doc.sent = Some(revision);
    client.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": doc.uri.as_str(), "version": doc.version },
            "contentChanges": [{ "text": text.to_string() }],
        }),
    );
}

/// A document was edited: its diagnostics move with the edit, and the server gets the text
/// after [`CHANGE_DELAY`] without further edits.
pub fn edited(buffer: &Entity<Buffer>, transaction: &Transaction, cx: &mut App) {
    if !cx.has_global::<LanguageServers>() {
        return;
    }
    let id = buffer.entity_id();
    if let Some(diagnostics) = servers(cx).diagnostics.get_mut(&id) {
        let entries = map_diagnostics(std::mem::take(&mut diagnostics.entries), transaction);
        *diagnostics = DocDiagnostics::new(entries);
    }
    let Some(doc) = servers(cx).docs.get(&id) else {
        return;
    };
    if !doc.opened {
        return;
    }
    let flush = cx.spawn(async move |cx| {
        cx.background_executor().timer(CHANGE_DELAY).await;
        cx.update(|cx| send_changes(id, cx));
    });
    if let Some(doc) = servers(cx).docs.get_mut(&id) {
        doc.flush = Some(flush);
    }
}

/// Diagnostics through an edit: text inserted inside one extends it, at its edges does not,
/// and one whose text was deleted goes.
fn map_diagnostics(
    mut entries: Vec<DocDiagnostic>,
    transaction: &Transaction,
) -> Vec<DocDiagnostic> {
    let changes = transaction.changes();
    if changes.is_identity() || entries.is_empty() {
        return entries;
    }
    entries.sort_by_key(|entry| entry.range.start);
    let mut by_end: Vec<usize> = (0..entries.len()).collect();
    by_end.sort_by_key(|&index| entries[index].range.end);
    let mut ends = changes.mapper(Assoc::Before);
    let mapped_ends: Vec<(usize, usize)> = by_end
        .into_iter()
        .map(|index| (index, ends.map(entries[index].range.end)))
        .collect();
    let mut starts = changes.mapper(Assoc::After);
    let was_empty: Vec<bool> = entries.iter().map(|entry| entry.range.is_empty()).collect();
    for entry in entries.iter_mut() {
        entry.range.start = starts.map(entry.range.start);
    }
    for (index, end) in mapped_ends {
        entries[index].range.end = end.max(entries[index].range.start);
    }
    entries
        .into_iter()
        .zip(was_empty)
        .filter(|(entry, empty)| *empty || !entry.range.is_empty())
        .map(|(entry, _)| entry)
        .collect()
}

/// Closes a document: `didClose` if the server knew of it, its diagnostics forgotten.
fn close_doc(id: EntityId, cx: &mut App) {
    let state = servers(cx);
    state.diagnostics.remove(&id);
    let Some(doc) = state.docs.remove(&id) else {
        return;
    };
    if doc.opened
        && let Some(client) = state
            .servers
            .get(&doc.key)
            .and_then(|server| server.client.as_ref())
    {
        client.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": doc.uri.as_str() } }),
        );
    }
    cx.refresh_windows();
}

/// Stops the server of `key`; its documents are to be opened again in the next one.
fn stop_server(key: &ServerKey, cx: &mut App) {
    let state = servers(cx);
    if let Some(server) = state.servers.remove(key)
        && let Some(client) = &server.client
    {
        client.stop();
    }
    let docs: Vec<EntityId> = state
        .docs
        .iter_mut()
        .filter(|(_, doc)| doc.key == *key)
        .map(|(id, doc)| {
            doc.opened = false;
            doc.sent = None;
            doc.flush = None;
            *id
        })
        .collect();
    for doc in docs {
        state.diagnostics.remove(&doc);
    }
    cx.refresh_windows();
}

fn stop_unused(cx: &mut App) {
    let state = servers(cx);
    let unused: Vec<ServerKey> = state
        .servers
        .keys()
        .filter(|key| !state.docs.values().any(|doc| doc.key == **key))
        .cloned()
        .collect();
    for key in unused {
        stop_server(&key, cx);
    }
}

/// Tools > Language Server > Restart: the server of `buffer` stops, and the next sync starts
/// it again.
pub fn restart(buffer: &Entity<Buffer>, cx: &mut App) -> Result<()> {
    let key = servers(cx)
        .docs
        .get(&buffer.entity_id())
        .map(|doc| doc.key.clone())
        .ok_or_else(|| anyhow!("{}", no_server_message(buffer, cx)))?;
    stop_server(&key, cx);
    Ok(())
}

/// Stops every server (the application quits).
pub fn stop_all(cx: &mut App) {
    let keys: Vec<ServerKey> = servers(cx).servers.keys().cloned().collect();
    for key in keys {
        stop_server(&key, cx);
    }
    servers(cx).docs.clear();
}

/// Why `buffer` has no server to ask.
fn no_server_message(buffer: &Entity<Buffer>, cx: &App) -> String {
    let language = buffer
        .read(cx)
        .language()
        .map_or_else(|| "text".to_owned(), |language| language.name.to_owned());
    if !AppState::global(cx).settings.lsp.enabled {
        return t!("language-servers.turned-off");
    }
    t!("language-servers.no-server", language = language)
}

/// What a request about a document needs: the server's client and abilities, and the
/// document's URI and text, its edits sent first.
pub struct Target {
    pub client: Client,
    pub abilities: Abilities,
    pub uri: Uri,
    pub text: Rope,
    pub key: ServerKey,
}

impl Target {
    /// The position of byte `offset` of the document, as the server counts.
    pub fn position(&self, offset: usize) -> lsp_types::Position {
        position(&self.text, offset, self.abilities.encoding)
    }

    pub fn document_position(&self, offset: usize) -> serde_json::Value {
        json!({
            "textDocument": { "uri": self.uri.as_str() },
            "position": self.position(offset),
        })
    }
}

/// The server of `buffer`, ready to be asked, or why there is none.
pub fn target(buffer: &Entity<Buffer>, cx: &mut App) -> Result<Target> {
    let id = buffer.entity_id();
    let Some(key) = servers(cx).docs.get(&id).map(|doc| doc.key.clone()) else {
        return Err(anyhow!("{}", no_server_message(buffer, cx)));
    };
    send_changes(id, cx);
    let missing = no_server_message(buffer, cx);
    let server = servers(cx)
        .servers
        .get(&key)
        .ok_or_else(|| anyhow!("{missing}"))?;
    let abilities = match &server.state {
        ServerState::Ready(abilities) => abilities.clone(),
        ServerState::Starting => {
            return Err(anyhow!(
                "{}",
                t!("language-servers.starting", language = key.language)
            ));
        }
        ServerState::Failed(reason) => {
            return Err(anyhow!(
                "{}",
                t!(
                    "language-servers.server-failed",
                    language = key.language,
                    reason = reason
                )
            ));
        }
    };
    let client = server
        .client
        .clone()
        .ok_or_else(|| anyhow!("{}", t!("lsp.server-not-running")))?;
    let doc = servers(cx).docs.get(&id).expect("looked up above");
    Ok(Target {
        client,
        abilities,
        uri: doc.uri.clone(),
        text: buffer.read(cx).doc().text().clone(),
        key,
    })
}

/// The diagnostics of a buffer, for the editor to underline.
pub fn underlined(buffer: EntityId, cx: &App) -> Diagnostics {
    cx.try_global::<LanguageServers>()
        .and_then(|servers| servers.diagnostics.get(&buffer))
        .map(|diagnostics| diagnostics.drawn.clone())
        .unwrap_or_default()
}

/// The diagnostics of a buffer, in the order of the text.
pub fn diagnostics(buffer: EntityId, cx: &App) -> Vec<DocDiagnostic> {
    cx.try_global::<LanguageServers>()
        .and_then(|servers| servers.diagnostics.get(&buffer))
        .map(|diagnostics| diagnostics.entries.clone())
        .unwrap_or_default()
}

/// The editor's hook after typing, and Function Completion: asks the server for entries after
/// a word character or one of its trigger characters, or when asked, and offers them in the
/// list. While the list has its entries for the word being typed, typing on filters them.
pub fn server_completion(view: &mut EditorView, typed: Option<&str>, cx: &mut Context<EditorView>) {
    let buffer = view.buffer.clone();
    if !cx.has_global::<LanguageServers>()
        || !cx
            .global::<LanguageServers>()
            .docs
            .contains_key(&buffer.entity_id())
    {
        return;
    }
    let Ok(target) = target(&buffer, cx) else {
        return;
    };
    if !target.abilities.completion {
        return;
    }
    let head = view.selection.primary().head;
    let start = birchpad_core::completion::word_start(&target.text, head);
    let context = match typed {
        None => json!({ "triggerKind": 1 }),
        Some(typed) => {
            let trigger = target
                .abilities
                .trigger_characters
                .iter()
                .find(|trigger| !trigger.is_empty() && typed.ends_with(trigger.as_str()));
            match trigger {
                Some(trigger) => json!({ "triggerKind": 2, "triggerCharacter": trigger }),
                None => {
                    let word = typed
                        .chars()
                        .last()
                        .is_some_and(birchpad_core::completion::is_word_char);
                    let listed = view
                        .server_items
                        .as_ref()
                        .is_some_and(|items| items.start == start);
                    if !word || listed {
                        return;
                    }
                    json!({ "triggerKind": 1 })
                }
            }
        }
    };
    let mut params = target.document_position(head);
    params["context"] = context;
    let answer = target.client.request("textDocument/completion", params);
    cx.spawn(async move |view, cx| {
        let Ok(value) = timeout(answer, cx).await else {
            return;
        };
        let response: Option<lsp_types::CompletionResponse> =
            serde_json::from_value(value).ok().flatten();
        let items = birchpad_lsp::convert::completions(response)
            .into_iter()
            .map(|entry| birchpad_editor::completion::ServerItem {
                label: entry.label,
                detail: entry.detail,
                insert: entry.insert,
                filter: entry.filter,
            })
            .collect();
        view.update(cx, |view, cx| view.offer_server_items(start, items, cx))
            .ok();
    })
    .detach();
}

/// Stops every server and waits for them, a few seconds at most (Birchpad quits).
pub fn stop_all_now(cx: &mut App) {
    let Some(state) = cx.try_global::<LanguageServers>() else {
        return;
    };
    let clients: Vec<Client> = state
        .servers
        .values()
        .filter_map(|server| server.client.clone())
        .collect();
    let stopping: Vec<_> = clients
        .into_iter()
        .map(|client| std::thread::spawn(move || client.stop_now()))
        .collect();
    for thread in stopping {
        let _ = thread.join();
    }
}
