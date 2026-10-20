//! The Tools menu: hashes of the selection or of files (Notepad++'s MD5, SHA-1, SHA-256 and
//! SHA-512), and conversions of the selected text (Base64, URL encoding, ASCII and hexadecimal,
//! as Notepad++'s MIME Tools and Converter plugins do). The hashes of files show in a window,
//! as Notepad++'s, with Copy, and open in a new document from there.

use anyhow::{Result, anyhow, bail};
use birchpad_core::hash::{HashKind, hash_hex};
use birchpad_core::ops::{Conversion, encode_selections};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, WeakEntity, Window, div, prelude::*, px,
};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::theme::{paint, ui};
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
pub fn hash_listing(
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

pub fn register_commands(registry: &mut CommandRegistry) {
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

#[cfg(feature = "test-support")]
thread_local! {
    /// The window of hashes opened last, for tests to drive.
    pub static OPENED: std::cell::RefCell<Option<WeakEntity<HashesDialog>>> =
        std::cell::RefCell::default();
}

/// Tools > MD5 (SHA-1, ...) > Generate from Files...: the hashes of the files chosen, as
/// `sha256sum` writes them, to copy or to open in a new document.
pub struct HashesDialog {
    workspace: WeakEntity<Workspace>,
    pub kind: HashKind,
    pub listing: String,
    /// Copy was clicked: the button says so.
    pub copied: bool,
}

/// Shows `listing`, the `kind` hashes of files.
pub fn open_hashes(
    workspace: WeakEntity<Workspace>,
    kind: HashKind,
    listing: String,
    window: &mut Window,
    cx: &mut App,
) {
    let dialog = cx.new(|_| HashesDialog {
        workspace,
        kind,
        listing,
        copied: false,
    });
    #[cfg(feature = "test-support")]
    OPENED.with(|opened| *opened.borrow_mut() = Some(dialog.downgrade()));
    let title = t!("tools.hashes-of-files", hash = kind.name());
    window.open_dialog(cx, move |modal, _, _| {
        modal
            .title(title.clone())
            .w(px(720.))
            .child(dialog.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

impl HashesDialog {
    /// Copy: the listing goes to the clipboard.
    pub fn copy(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.listing.clone()));
        self.copied = true;
        cx.notify();
    }

    /// Open in New Document: the listing in a new untitled document, and the window closes.
    pub fn open_in_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let listing = self.listing.clone();
        let opened = self.workspace.update(cx, |workspace, cx| {
            workspace.open_text_in_new_document(&listing, window, cx)
        });
        match opened {
            Ok(Ok(())) => window.close_dialog(cx),
            Ok(Err(error)) => report_error(&error, window, cx),
            Err(_) => window.close_dialog(cx),
        }
    }
}

impl Render for HashesDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines: Vec<String> = self.listing.lines().map(str::to_owned).collect();
        let empty = lines.is_empty();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("hashes-listing")
                    .h(px(240.))
                    .overflow_scroll()
                    .p_2()
                    .border_1()
                    .border_color(paint(ui().border))
                    .font_family(crate::MONOSPACE)
                    .text_sm()
                    .children(lines.into_iter().map(|line| {
                        let error = line.starts_with("# ");
                        div()
                            .whitespace_nowrap()
                            .when(error, |line| line.text_color(paint(ui().error)))
                            .child(line)
                    }))
                    .when(empty, |listing| listing.child(t!("tools.no-files"))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new("hashes-copy")
                            .primary()
                            .label(if self.copied {
                                t!("tools.copied")
                            } else {
                                t!("tools.copy")
                            })
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(
                        Button::new("hashes-document")
                            .label(t!("tools.open-in-new-document"))
                            .disabled(empty)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.open_in_document(window, cx)
                                }),
                            ),
                    ),
            )
    }
}

impl Workspace {
    /// Asks for files, and shows their hashes.
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
            this.update_in(cx, |_, window, cx| {
                open_hashes(cx.entity().downgrade(), kind, listing, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// A new untitled document holding `text`.
    pub fn open_text_in_new_document(
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
