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
mod imp {
    use std::io;
    use std::path::Path;
    use std::process::Command;

    use super::{DESKTOP_FILE, MIME_TYPES, is_birchpad_desktop};

    pub(super) const ROOT: &str = "";

    pub(super) fn register(_: &str, _: &Path) -> io::Result<()> {
        run("xdg-mime", &register_args())
    }

    pub(super) fn unregister(_: &str) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn is_default(_: &str) -> bool {
        answers_birchpad("xdg-mime", &["query", "default", "text/plain"])
    }

    /// `xdg-mime default`'s arguments: the desktop file, then the MIME types.
    fn register_args() -> Vec<&'static str> {
        let mut args = vec!["default", DESKTOP_FILE];
        args.extend(MIME_TYPES);
        args
    }

    /// Runs `program`; an error if it cannot start or does not succeed.
    fn run(program: &str, args: &[&str]) -> io::Result<()> {
        let status = Command::new(program).args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!("{program}: {status}")))
        }
    }

    /// Whether `program` answers Birchpad's desktop file; not if it cannot start.
    fn answers_birchpad(program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .output()
            .is_ok_and(|output| is_birchpad_desktop(&String::from_utf8_lossy(&output.stdout)))
    }

    /// With programs of every system instead of `xdg-mime`, which would change the user's
    /// defaults.
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_desktop_file_for_every_type() {
            let args = register_args();
            assert_eq!(&args[..2], ["default", DESKTOP_FILE]);
            assert_eq!(&args[2..], MIME_TYPES);
        }

        #[test]
        fn what_the_programs_say() {
            run("true", &[]).unwrap();
            assert!(run("false", &[]).is_err(), "did not succeed");
            assert!(
                run("birchpad-no-such-program", &[]).is_err(),
                "cannot start"
            );
            assert!(answers_birchpad("echo", &[DESKTOP_FILE]));
            assert!(!answers_birchpad("echo", &["org.gnome.TextEditor.desktop"]));
            assert!(!answers_birchpad("echo", &[]), "nothing");
            assert!(!answers_birchpad("birchpad-no-such-program", &[]));
            // What only reads: nothing of Birchpad's to take away, and asking changes nothing.
            super::super::unregister().unwrap();
            let _ = super::super::is_default("txt");
        }
    }
}

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
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn what_counts_as_birchpad() {
        assert!(is_birchpad_choice("Birchpad.File"));
        assert!(is_birchpad_choice(r"Applications\Birchpad.exe"));
        for other in ["", "txtfile", "Applications\\notepad.exe", "Birchpad.File2"] {
            assert!(!is_birchpad_choice(other), "{other}");
        }
        assert!(is_birchpad_desktop(
            "io.github.securitytrip.birchpad.desktop\n"
        ));
        assert!(!is_birchpad_desktop(""));
        assert!(!is_birchpad_desktop("org.gnome.TextEditor.desktop"));
        assert_eq!(
            open_command(&PathBuf::from(r"C:\Apps\Birchpad\birchpad.exe")),
            r#""C:\Apps\Birchpad\birchpad.exe" "%1""#
        );
        // The two lists name text files only, once each.
        let mut seen = std::collections::HashSet::new();
        assert!(EXTENSIONS.iter().all(|extension| seen.insert(*extension)));
        assert!(EXTENSIONS.contains(&"txt") && EXTENSIONS.contains(&"json"));
        assert!(MIME_TYPES.contains(&"text/plain"));
        assert_eq!(settings_page().is_some(), cfg!(windows));
        assert_eq!(supported(), !cfg!(target_os = "macos"));
    }

    /// In a key of the tests' own, not the user's classes.
    #[cfg(windows)]
    #[test]
    fn registered_and_taken_away() {
        use windows_registry::CURRENT_USER;
        let root = format!(
            r"Software\Birchpad-tests\associations-{}",
            std::process::id()
        );
        assert!(!imp::is_registered(&root));
        imp::unregister(&root).unwrap();

        let exe = PathBuf::from(r"C:\Apps\Birchpad\birchpad.exe");
        imp::register(&root, &exe).unwrap();
        imp::register(&root, &exe).unwrap();
        assert!(imp::is_registered(&root));
        let command = CURRENT_USER
            .open(format!(r"{root}\Classes\{PROG_ID}\shell\open\command"))
            .unwrap();
        assert_eq!(command.get_string("").unwrap(), open_command(&exe));
        let txt = CURRENT_USER
            .open(format!(r"{root}\Classes\.txt\OpenWithProgids"))
            .unwrap();
        assert_eq!(txt.get_string(PROG_ID).unwrap(), "");
        let apps = CURRENT_USER
            .open(format!(r"{root}\RegisteredApplications"))
            .unwrap();
        assert_eq!(
            apps.get_string(APP_NAME).unwrap(),
            r"Software\Birchpad\Capabilities"
        );

        imp::unregister(&root).unwrap();
        assert!(!imp::is_registered(&root));
        let txt = CURRENT_USER
            .open(format!(r"{root}\Classes\.txt\OpenWithProgids"))
            .unwrap();
        assert!(
            txt.get_string(PROG_ID).is_err(),
            "the extension's own key stays"
        );
        imp::unregister(&root).unwrap();
        CURRENT_USER.remove_tree(&root).unwrap();
        // An extension no one chose Birchpad for.
        assert!(!is_default("birchpad-test-nothing"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finder_chooses_on_macos() {
        assert!(register(Path::new("/Applications/Birchpad.app")).is_err());
        unregister().unwrap();
        assert!(!is_default("txt"));
    }
}
