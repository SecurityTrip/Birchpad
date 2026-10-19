//! Birchpad as the default editor of text files (issue #32): it registers itself as an editor
//! of the usual text file types, and says whether it opens them now.
//!
//! - **Windows** lets no program make itself the default (since Windows 8): Birchpad registers
//!   a file type (`Birchpad.File`), adds it to the "Open with" list of each extension, and
//!   describes itself to Default apps (`RegisteredApplications`), all for the current user;
//!   then the user picks it in Settings > Default apps, which [`settings_page`] opens.
//! - **Linux**: `xdg-mime default` makes the desktop file the default of the MIME types; the
//!   desktop file has to be installed (README, Installing).
//! - **macOS**: the app bundle declares its document types (`Info.plist`); Finder's Get Info >
//!   Open with > Change All makes it the default. Nothing to register here.

use std::io;
use std::path::Path;

/// The file type Birchpad registers on Windows.
pub const PROG_ID: &str = "Birchpad.File";

/// The name Default apps knows Birchpad by on Windows.
pub const APP_NAME: &str = "Birchpad";

/// The extensions Birchpad registers for, without the dot.
pub const EXTENSIONS: &[&str] = &[
    "txt", "log", "ini", "cfg", "conf", "json", "xml", "md", "csv", "yaml", "yml", "toml",
];

/// The MIME types Birchpad becomes the default of on Linux.
pub const MIME_TYPES: &[&str] = &[
    "text/plain",
    "application/json",
    "application/xml",
    "text/markdown",
    "text/csv",
    "application/x-yaml",
    "application/toml",
];

/// The desktop file of Birchpad on Linux (`packaging/linux`).
pub const DESKTOP_FILE: &str = "io.github.securitytrip.birchpad.desktop";

/// Whether this platform has something to register.
pub fn supported() -> bool {
    cfg!(any(windows, all(unix, not(target_os = "macos"))))
}

/// Where the user chooses the default (Windows' Settings > Default apps, at Birchpad's page);
/// `None` where registering makes Birchpad the default already.
pub fn settings_page() -> Option<&'static str> {
    cfg!(windows).then_some("ms-settings:defaultapps?registeredAppUser=Birchpad")
}

/// Registers `exe` as an editor of [`EXTENSIONS`] for the current user; on Linux, makes it the
/// default of [`MIME_TYPES`].
pub fn register(exe: &Path) -> io::Result<()> {
    imp::register(imp::ROOT, exe)
}

/// Takes the registration away (Windows); elsewhere there is nothing of Birchpad's to take.
pub fn unregister() -> io::Result<()> {
    imp::unregister(imp::ROOT)
}

/// Whether Birchpad opens `.<extension>` files now.
pub fn is_default(extension: &str) -> bool {
    imp::is_default(extension)
}

/// The command Windows runs for a file of the type: the program and the file, both quoted.
pub fn open_command(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// Whether the file type Windows says the user chose is Birchpad's: its own, or the one Windows
/// makes of the program when it is picked in "Open with".
pub fn is_birchpad_choice(prog_id: &str) -> bool {
    prog_id == PROG_ID || prog_id.eq_ignore_ascii_case(r"Applications\birchpad.exe")
}

/// Whether `xdg-mime query default` answered Birchpad's desktop file.
pub fn is_birchpad_desktop(answer: &str) -> bool {
    answer.trim() == DESKTOP_FILE
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::path::Path;

    use windows_registry::CURRENT_USER;

    use super::{APP_NAME, EXTENSIONS, PROG_ID, is_birchpad_choice, open_command};

    /// Under `HKEY_CURRENT_USER`.
    pub(super) const ROOT: &str = "Software";

    fn err(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
        io::Error::other(error)
    }

    pub(super) fn register(root: &str, exe: &Path) -> io::Result<()> {
        let prog = CURRENT_USER
            .create(format!(r"{root}\Classes\{PROG_ID}"))
            .map_err(err)?;
        prog.set_string("", "Text document (Birchpad)")
            .map_err(err)?;
        prog.create("DefaultIcon")
            .and_then(|key| key.set_string("", format!("\"{}\",0", exe.display())))
            .map_err(err)?;
        prog.create(r"shell\open\command")
            .and_then(|key| key.set_string("", open_command(exe)))
            .map_err(err)?;
        let capabilities = CURRENT_USER
            .create(format!(r"{root}\{APP_NAME}\Capabilities"))
            .map_err(err)?;
        capabilities
            .set_string("ApplicationName", APP_NAME)
            .map_err(err)?;
        capabilities
            .set_string(
                "ApplicationDescription",
                "A fast text editor in the spirit of Notepad++",
            )
            .map_err(err)?;
        let associations = capabilities.create("FileAssociations").map_err(err)?;
        for extension in EXTENSIONS {
            associations
                .set_string(format!(".{extension}"), PROG_ID)
                .map_err(err)?;
            CURRENT_USER
                .create(format!(r"{root}\Classes\.{extension}\OpenWithProgids"))
                .and_then(|key| key.set_string(PROG_ID, ""))
                .map_err(err)?;
        }
        CURRENT_USER
            .create(format!(r"{root}\RegisteredApplications"))
            .and_then(|key| key.set_string(APP_NAME, format!(r"Software\{APP_NAME}\Capabilities")))
            .map_err(err)?;
        Ok(())
    }

    pub(super) fn unregister(root: &str) -> io::Result<()> {
        for extension in EXTENSIONS {
            if let Ok(key) =
                CURRENT_USER.open(format!(r"{root}\Classes\.{extension}\OpenWithProgids"))
                && key.get_string(PROG_ID).is_ok()
            {
                let key = CURRENT_USER
                    .create(format!(r"{root}\Classes\.{extension}\OpenWithProgids"))
                    .map_err(err)?;
                key.remove_value(PROG_ID).map_err(err)?;
            }
        }
        if let Ok(key) = CURRENT_USER.create(format!(r"{root}\RegisteredApplications"))
            && key.get_string(APP_NAME).is_ok()
        {
            key.remove_value(APP_NAME).map_err(err)?;
        }
        for path in [
            format!(r"{root}\Classes\{PROG_ID}"),
            format!(r"{root}\{APP_NAME}"),
        ] {
            if CURRENT_USER.open(&path).is_ok() {
                CURRENT_USER.remove_tree(&path).map_err(err)?;
            }
        }
        Ok(())
    }

    pub(super) fn is_default(extension: &str) -> bool {
        CURRENT_USER
            .open(format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.{extension}\UserChoice"
            ))
            .and_then(|key| key.get_string("ProgId"))
            .is_ok_and(|prog_id| is_birchpad_choice(&prog_id))
    }

    /// The tests' own key, not the user's classes.
    #[cfg(test)]
    pub(super) fn is_registered(root: &str) -> bool {
        CURRENT_USER
            .open(format!(r"{root}\Classes\{PROG_ID}\shell\open\command"))
            .is_ok()
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "associations/linux.rs"]
mod imp;

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::Path;

    pub(super) const ROOT: &str = "";

    pub(super) fn register(_: &str, _: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Finder chooses the default: Get Info > Open with > Change All",
        ))
    }

    pub(super) fn unregister(_: &str) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn is_default(_: &str) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "../tests/unit/associations/tests.rs"]
mod tests;
