//! The Tools menu: hashes of the selection or of files (Notepad++'s MD5, SHA-1, SHA-256 and
//! SHA-512), and conversions of the selected text (Base64, URL encoding, ASCII and hexadecimal,
//! as Notepad++'s MIME Tools and Converter plugins do).

use anyhow::{Result, anyhow, bail};
use birchpad_core::hash::{HashKind, hash_hex};
use birchpad_core::ops::{Conversion, encode_selections};
use gpui_kit::{ClipboardItem, Context, Window};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::{Workspace, report_error};

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum HashName {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl From<HashName> for HashKind {
    fn from(name: HashName) -> Self {
        match name {
            HashName::Md5 => Self::Md5,
            HashName::Sha1 => Self::Sha1,
            HashName::Sha256 => Self::Sha256,
            HashName::Sha512 => Self::Sha512,
        }
    }
}

#[derive(Deserialize)]
struct HashArgs {
    hash: HashName,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum ConversionName {
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    AsciiToHex,
    HexToAscii,
}

#[derive(Deserialize)]
struct ConvertArgs {
    to: ConversionName,
}

impl From<ConversionName> for Conversion {
    fn from(name: ConversionName) -> Self {
        match name {
            ConversionName::Base64Encode => Self::Base64Encode,
            ConversionName::Base64Decode => Self::Base64Decode,
            ConversionName::UrlEncode => Self::UrlEncode,
            ConversionName::UrlDecode => Self::UrlDecode,
            ConversionName::AsciiToHex => Self::AsciiToHex,
            ConversionName::HexToAscii => Self::HexToAscii,
        }
    }
}

/// The lines of a hash listing, as `sha256sum` writes them: `<hash>  <path>`.
pub(crate) fn hash_listing(
    kind: HashKind,
    files: &[(std::path::PathBuf, std::io::Result<Vec<u8>>)],
) -> String {
    let mut out = String::new();
    for (path, bytes) in files {
        match bytes {
            Ok(bytes) => out.push_str(&format!("{}  {}\n", hash_hex(kind, bytes), path.display())),
            Err(error) => out.push_str(&format!("# {}: {error}\n", path.display())),
        }
    }
    out
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("tools.hash-selection", |this, args: HashArgs, _, cx| {
        let Some(selected) = this.selected_text(cx) else {
            bail!("{}", t!("tools.select-the-text-to-hash"));
        };
        let hash = hash_hex(args.hash.into(), selected.as_bytes());
        cx.write_to_clipboard(ClipboardItem::new_string(hash));
        Ok(())
    });
    registry.editor("edit.convert", |this, args: ConvertArgs, _, cx| {
        convert(this, args.to.into(), cx)
    });
    registry.workspace("tools.hash-files", |this, args: HashArgs, window, cx| {
        this.hash_files(args.hash.into(), window, cx);
        Ok(())
    });
}

fn convert(
    view: &mut EditorView,
    conversion: Conversion,
    cx: &mut Context<EditorView>,
) -> Result<()> {
    view.try_op_public(cx, |text, selection| {
        encode_selections(text, selection, conversion).map_err(|error| anyhow!(error))
    })
}

impl Workspace {
    /// Asks for files, and opens their hashes in a new document.
    fn hash_files(&mut self, kind: HashKind, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(start, cx).await else {
                return;
            };
            let files: Vec<_> = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| {
                            let bytes = std::fs::read(&path);
                            (path, bytes)
                        })
                        .collect()
                })
                .await;
            let listing = hash_listing(kind, &files);
            this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.open_text_in_new_document(&listing, window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// A new untitled document holding `text`.
    pub(crate) fn open_text_in_new_document(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.new_file(window, cx);
        let invocation = birchpad_commands::Invocation::with_args(
            "edit.insert-text",
            serde_json::json!({ "text": text }),
        );
        self.dispatch(&invocation, window, cx)?;
        let start = birchpad_commands::Invocation::new("cursor.document-start");
        self.dispatch(&start, window, cx)
    }
}

#[cfg(test)]
mod tests;
