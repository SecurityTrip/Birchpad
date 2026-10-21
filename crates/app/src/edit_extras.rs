//! More of Notepad++'s Edit menu: Set Read-Only and Clear Read-Only Flag; Paste Special (copy,
//! cut and paste binary content, which keeps NUL characters, and paste the clipboard's HTML or
//! RTF as text); On Selection (open the file named by the selection, show its folder, or
//! search for it on the Internet with Preferences > Search Engine's engine).

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use gpui_kit::{App, ClipboardItem, Context};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::{EditorView, LastEdit};
use crate::workspace::Workspace;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("edit.toggle-read-only", |this, (), _, cx| {
        let buffer = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?
            .read(cx)
            .buffer
            .clone();
        buffer.update(cx, |buffer, cx| {
            let on = !buffer.requested_read_only();
            buffer.set_requested_read_only(on, cx);
        });
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("edit.clear-read-only-flag", |this, (), _, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        let buffer = view.read(cx).buffer.clone();
        let (path, read_only) = {
            let buffer = buffer.read(cx);
            (
                buffer.path().map(Path::to_owned),
                buffer.read_only() == Some(crate::buffer::ReadOnly::File),
            )
        };
        let Some(path) = path else {
            bail!(
                "{}",
                t!(
                    "file-commands.has-no-file-yet",
                    name = buffer.read(cx).display_name()
                )
            );
        };
        if !read_only {
            bail!(
                "{}",
                t!("edit-extras.is-not-read-only", path = path.display())
            );
        }
        clear_read_only_flag(&path)?;
        buffer.update(cx, |buffer, cx| buffer.file_made_writable(cx));
        this.refresh_menus(cx);
        Ok(())
    });

    registry.editor("edit.copy-binary", |this, (), _, cx| {
        copy_binary(this, cx);
        Ok(())
    });
    registry.editor("edit.cut-binary", |this, (), _, cx| {
        if copy_binary(this, cx) {
            this.insert("", LastEdit::None, cx);
        }
        Ok(())
    });
    registry.editor("edit.paste-binary", |this, (), _, cx| {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| binary_text(&item)) {
            this.insert(&text, LastEdit::None, cx);
        }
        Ok(())
    });
    for (id, format) in [
        ("edit.paste-html", RichFormat::Html),
        ("edit.paste-rtf", RichFormat::Rtf),
    ] {
        registry.editor(id, move |this, (), _, cx| {
            let text = rich_clipboard::read(format)?.ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!("edit-extras.the-clipboard-has-no", format = format.name())
                )
            })?;
            this.insert(&text, LastEdit::None, cx);
            Ok(())
        });
    }

    registry.workspace("edit.open-selected-file", |this, (), window, cx| {
        let path = this.selected_path(cx)?;
        if !path.is_file() {
            bail!(
                "{}",
                t!("edit-extras.there-is-no-file", path = path.display())
            );
        }
        this.open_path(&path, window, cx);
        Ok(())
    });
    registry.workspace("edit.open-selected-folder", |this, (), _, cx| {
        let path = this.selected_path(cx)?;
        if !path.exists() {
            bail!(
                "{}",
                t!("edit-extras.there-is-no-file", path = path.display())
            );
        }
        crate::file_commands::shell(crate::file_commands::Shell::Reveal(path), cx)
    });
    registry.workspace("edit.search-online", |this, (), _, cx| {
        let words = this.selection_text(cx)?;
        let engine = AppState::global(cx).settings.search_engine.clone();
        let url = engine.url(&words).ok_or_else(|| {
            anyhow!(
                "{}",
                t!("edit-extras.the-search-engine-is-not-a-web-address")
            )
        })?;
        crate::file_commands::shell(crate::file_commands::Shell::Browse(url), cx)
    });
}

/// What the clipboard of binary content holds: the text with its NUL characters, which other
/// programs (and the plain text format) would cut short.
#[derive(Serialize, Deserialize)]
struct BinaryClipboard {
    binary: String,
}

/// Paste Special > Copy Binary Content: the selection, NULs included; other programs get it
/// without them.
fn copy_binary(view: &mut EditorView, cx: &mut Context<EditorView>) -> bool {
    let Some(text) = view.selected_text(cx) else {
        return false;
    };
    let plain = text.replace('\0', "");
    cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
        plain,
        BinaryClipboard { binary: text },
    ));
    true
}

/// The text Paste Binary Content pastes: what Copy Binary Content put there, or the plain text.
fn binary_text(item: &ClipboardItem) -> Option<String> {
    let binary = item
        .metadata()
        .and_then(|metadata| serde_json::from_str::<BinaryClipboard>(metadata).ok())
        .map(|clipboard| clipboard.binary);
    binary.or_else(|| item.text())
}

/// Makes a read-only file writable for its owner.
fn clear_read_only_flag(path: &Path) -> Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).map_err(|error| {
        anyhow!(
            "{}",
            t!(
                "edit-extras.cannot-make-writable",
                path = path.display(),
                error = error
            )
        )
    })
}

impl Workspace {
    /// The active document's primary selection, trimmed; an error when nothing is selected.
    fn selection_text(&self, cx: &App) -> Result<String> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("file-ops.no-document")))?;
        let view = view.read(cx);
        let range = view.selection.primary();
        let text = view
            .buffer
            .read(cx)
            .doc()
            .text()
            .slice(range.from()..range.to())
            .to_string();
        let text = text.trim();
        if text.is_empty() {
            bail!("{}", t!("edit-extras.select-something-first"));
        }
        Ok(text.to_owned())
    }

    /// The path the selection names: as written if absolute, else in the folder of the active
    /// document (or the folder dialogs start in).
    fn selected_path(&self, cx: &App) -> Result<PathBuf> {
        let text = self.selection_text(cx)?;
        let text = text.trim_matches(['"', '\'']);
        let path = Path::new(text);
        if path.is_absolute() {
            return Ok(path.to_owned());
        }
        Ok(self.default_directory(cx).join(path))
    }
}

/// A format of the clipboard besides plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichFormat {
    Html,
    Rtf,
}

impl RichFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Rtf => "RTF",
        }
    }
}

/// Reading the clipboard's HTML and RTF, which GPUI's clipboard does not offer.
pub mod rich_clipboard {
    use super::RichFormat;

    #[cfg(feature = "test-support")]
    thread_local! {
        /// The clipboard's HTML and RTF as tests set them.
        pub static FAKE: std::cell::RefCell<Vec<(RichFormat, String)>> =
            std::cell::RefCell::default();
    }

    /// The clipboard's content in `format`, if it has some.
    pub fn read(format: RichFormat) -> anyhow::Result<Option<String>> {
        #[cfg(feature = "test-support")]
        {
            Ok(FAKE.with(|fake| {
                fake.borrow()
                    .iter()
                    .find(|(known, _)| *known == format)
                    .map(|(_, text)| text.clone())
            }))
        }
        #[cfg(not(feature = "test-support"))]
        {
            native::read(format)
        }
    }

    /// The HTML of Windows' "HTML Format": the document after its description, which says
    /// where it starts (`StartHTML:`); all of it if that is missing.
    #[cfg(any(feature = "test-support", windows))]
    pub fn html_of_windows_format(data: &str) -> String {
        let start = data.lines().find_map(|line| {
            line.strip_prefix("StartHTML:")
                .and_then(|offset| offset.trim().parse::<usize>().ok())
        });
        match start {
            Some(start) if start <= data.len() && data.is_char_boundary(start) => {
                data[start..].to_owned()
            }
            _ => data.to_owned(),
        }
    }

    #[cfg(all(not(feature = "test-support"), windows))]
    mod native {
        use windows_sys::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
        };
        use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

        use super::RichFormat;

        pub(super) fn read(format: RichFormat) -> anyhow::Result<Option<String>> {
            let name = match format {
                RichFormat::Html => "HTML Format",
                RichFormat::Rtf => "Rich Text Format",
            };
            let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
            // SAFETY: the clipboard is opened and closed here, on this thread; the data handle
            // is only read between GlobalLock and GlobalUnlock, within GlobalSize bytes, and
            // copied out before the clipboard closes.
            #[allow(unsafe_code)]
            let bytes = unsafe {
                let id = RegisterClipboardFormatW(wide.as_ptr());
                if id == 0 || OpenClipboard(std::ptr::null_mut()) == 0 {
                    anyhow::bail!("{}", std::io::Error::last_os_error());
                }
                let handle = GetClipboardData(id);
                let bytes = if handle.is_null() {
                    None
                } else {
                    let size = GlobalSize(handle);
                    let data = GlobalLock(handle) as *const u8;
                    let bytes =
                        (!data.is_null()).then(|| std::slice::from_raw_parts(data, size).to_vec());
                    GlobalUnlock(handle);
                    bytes
                };
                CloseClipboard();
                bytes
            };
            Ok(bytes.map(|mut bytes| {
                // Both formats end with a NUL.
                if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
                    bytes.truncate(end);
                }
                let text = String::from_utf8_lossy(&bytes).into_owned();
                match format {
                    RichFormat::Html => super::html_of_windows_format(&text),
                    RichFormat::Rtf => text,
                }
            }))
        }
    }

    #[cfg(all(not(feature = "test-support"), target_os = "macos"))]
    mod native {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF};

        use super::RichFormat;

        pub(super) fn read(format: RichFormat) -> anyhow::Result<Option<String>> {
            // SAFETY: the pasteboard type constants are immutable NSString statics of AppKit.
            #[allow(unsafe_code)]
            let kind = unsafe {
                match format {
                    RichFormat::Html => NSPasteboardTypeHTML,
                    RichFormat::Rtf => NSPasteboardTypeRTF,
                }
            };
            let pasteboard = NSPasteboard::generalPasteboard();
            Ok(pasteboard.stringForType(kind).map(|text| text.to_string()))
        }
    }

    #[cfg(all(not(feature = "test-support"), unix, not(target_os = "macos")))]
    mod native {
        use super::RichFormat;

        /// Through `wl-paste` under Wayland and `xclip` under X11, when they are installed.
        pub(super) fn read(format: RichFormat) -> anyhow::Result<Option<String>> {
            let mime = match format {
                RichFormat::Html => "text/html",
                RichFormat::Rtf => "text/rtf",
            };
            let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
            let output = if wayland {
                std::process::Command::new("wl-paste")
                    .args(["--no-newline", "--type", mime])
                    .output()
            } else {
                std::process::Command::new("xclip")
                    .args(["-selection", "clipboard", "-target", mime, "-out"])
                    .output()
            }
            .map_err(|error| {
                anyhow::anyhow!(
                    "{}",
                    t!(
                        "edit-extras.cannot-read-the-clipboard",
                        program = if wayland { "wl-paste" } else { "xclip" },
                        error = error
                    )
                )
            })?;
            Ok(output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
                .filter(|text| !text.is_empty()))
        }
    }
}
