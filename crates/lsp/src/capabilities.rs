//! What Birchpad tells a server it can do, and what it reads of what the server can.

use std::path::Path;

use lsp_types::{InitializeResult, OneOf, ServerCapabilities, TextDocumentSyncCapability};
use serde_json::{Value, json};

use crate::client::{Client, RequestError};
use crate::position::PositionEncoding;
use crate::uri::file_uri_text;

/// The parameters of `initialize` for a server working in `root`: only what Birchpad does
/// (ADR 0065), no snippets, plain text and Markdown, UTF-8 positions preferred.
pub fn initialize_params(root: &Path, process_id: Option<u32>) -> Value {
    let root_uri = file_uri_text(root);
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folders = root_uri
        .as_ref()
        .map(|uri| json!([{ "uri": uri, "name": name }]))
        .unwrap_or(Value::Null);
    let encodings: Vec<String> = PositionEncoding::offered()
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    json!({
        "processId": process_id,
        "clientInfo": { "name": "Birchpad", "version": env!("CARGO_PKG_VERSION") },
        "rootUri": root_uri,
        "rootPath": root.to_str(),
        "workspaceFolders": folders,
        "capabilities": {
            "general": { "positionEncodings": encodings },
            "workspace": {
                "applyEdit": false,
                "workspaceEdit": { "documentChanges": true },
                "configuration": true,
                "workspaceFolders": true,
            },
            "window": { "workDoneProgress": true },
            "textDocument": {
                "synchronization": { "didSave": true, "dynamicRegistration": false },
                "publishDiagnostics": { "relatedInformation": false, "versionSupport": true },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "insertReplaceSupport": true,
                        "documentationFormat": ["plaintext", "markdown"],
                    },
                    "contextSupport": true,
                },
                "hover": { "contentFormat": ["plaintext", "markdown"] },
                "definition": { "linkSupport": true },
                "references": {},
                "rename": { "prepareSupport": false },
                "formatting": {},
            },
        },
    })
}

/// What a server said it can do, read once it initialized.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Abilities {
    pub encoding: PositionEncoding,
    /// Whether it wants documents opened, changed and closed.
    pub sync: bool,
    /// Whether it wants `didSave`, and the text with it.
    pub save: Option<bool>,
    pub completion: bool,
    /// The characters besides word characters after which it offers completions.
    pub trigger_characters: Vec<String>,
    pub hover: bool,
    pub definition: bool,
    pub references: bool,
    pub rename: bool,
    pub formatting: bool,
}

fn offers<T>(capability: &Option<OneOf<bool, T>>) -> bool {
    matches!(capability, Some(OneOf::Left(true) | OneOf::Right(_)))
}

impl Abilities {
    pub fn of(capabilities: &ServerCapabilities) -> Self {
        let (sync, save) = match &capabilities.text_document_sync {
            None => (false, None),
            Some(TextDocumentSyncCapability::Kind(kind)) => {
                (*kind != lsp_types::TextDocumentSyncKind::NONE, None)
            }
            Some(TextDocumentSyncCapability::Options(options)) => {
                let save = match &options.save {
                    Some(lsp_types::TextDocumentSyncSaveOptions::Supported(true)) => Some(false),
                    Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(options)) => {
                        Some(options.include_text.unwrap_or(false))
                    }
                    _ => None,
                };
                let opens = options.open_close.unwrap_or(false);
                let changes = options
                    .change
                    .is_some_and(|kind| kind != lsp_types::TextDocumentSyncKind::NONE);
                (opens || changes, save)
            }
        };
        Self {
            encoding: PositionEncoding::of_server(capabilities.position_encoding.as_ref()),
            sync,
            save,
            completion: capabilities.completion_provider.is_some(),
            trigger_characters: capabilities
                .completion_provider
                .as_ref()
                .and_then(|provider| provider.trigger_characters.clone())
                .unwrap_or_default(),
            hover: matches!(
                capabilities.hover_provider,
                Some(lsp_types::HoverProviderCapability::Simple(true))
                    | Some(lsp_types::HoverProviderCapability::Options(_))
            ),
            definition: offers(&capabilities.definition_provider),
            references: offers(&capabilities.references_provider),
            rename: offers(&capabilities.rename_provider),
            formatting: offers(&capabilities.document_formatting_provider),
        }
    }
}

/// Initializes the server: `initialize` with [`initialize_params`], then `initialized`. The
/// result is what it can do.
pub async fn initialize(
    client: &Client,
    root: &Path,
    process_id: Option<u32>,
) -> Result<Abilities, RequestError> {
    let answer = client
        .request("initialize", initialize_params(root, process_id))
        .await?;
    let result: InitializeResult =
        serde_json::from_value(answer).map_err(|error| RequestError::Invalid(error.to_string()))?;
    client.notify("initialized", json!({}));
    Ok(Abilities::of(&result.capabilities))
}
