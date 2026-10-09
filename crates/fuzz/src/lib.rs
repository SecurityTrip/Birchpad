//! What Birchpad's fuzz targets check: one function for each kind of input that comes from
//! outside, which `fuzz/` hands to libFuzzer (`cargo +nightly fuzz run <target>`) and the
//! ordinary tests run over the seeds in `fuzz/seeds` and damaged copies of them.
//!
//! Each function takes any bytes and checks more than "no panic": text that reads cleanly
//! writes back the same, what reads once reads again after a save. Each returns whether the
//! input was accepted, so that the tests can tell that valid seeds still are.

use std::ffi::OsString;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use birchpad_cli::CommandLine;
use birchpad_commands::{Keymap, Keystroke, Layer, Platform};
use birchpad_config::{ProjectWorkspace, Session, Sources, UpdateChannel, UserState, resolve};
use birchpad_core::Encoding;
use birchpad_io::{CHARACTER_SETS, FileInfo, LoadOptions, decode_appended, decode_file, encode};
use birchpad_theme::Theme;
use birchpad_update::SigningKey;
use birchpad_update::manifest::{self, Envelope, Signature};
use ed25519_dalek::Signer as _;

/// A check: takes any bytes, returns whether they were accepted, panics on a bug.
pub type Check = fn(&[u8]) -> bool;

/// The fuzz targets by name, as `fuzz/` and the seed folders name them.
pub const TARGETS: &[(&str, Check)] = &[
    ("open_file", open_file),
    ("settings", settings),
    ("state", state),
    ("session", session),
    ("workspace_file", workspace_file),
    ("theme", theme),
    ("notepad_theme", notepad_theme),
    ("keymap", keymap),
    ("command_line", command_line),
    ("instance_message", instance_message),
    ("manifest", manifest),
];

/// Every encoding a file can be opened in.
fn encodings() -> Vec<Encoding> {
    let mut all = vec![Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be];
    all.extend(
        CHARACTER_SETS
            .iter()
            .flat_map(|(_, sets)| sets.iter())
            .map(|(name, _)| Encoding::Legacy(name)),
    );
    all
}

/// Opening a file. The first byte chooses how: 0 detects the encoding, any other value opens
/// it in one encoding ("Encode in ..."); the rest is the file. A file that decodes without a
/// problem saves back to the same bytes; one with a problem says where.
pub fn open_file(data: &[u8]) -> bool {
    let Some((&choice, bytes)) = data.split_first() else {
        return true;
    };
    let encodings = encodings();
    let encoding = usize::from(choice)
        .checked_sub(1)
        .map(|index| encodings[index % encodings.len()]);
    let info = FileInfo {
        len: bytes.len() as u64,
        read_only: false,
        modified: None,
    };
    let options = LoadOptions {
        encoding,
        ansi: Encoding::Legacy("windows-1252"),
    };
    let file = decode_file(bytes.to_vec(), info, options);
    let format = file.format;
    assert!(file.head.len <= bytes.len());

    // Monitoring (tail -f) decodes what is appended piece by piece; never more than it got.
    if let Some((text, used)) = decode_appended(bytes, format.encoding) {
        assert!(used <= bytes.len());
        assert!(
            text.len() <= 3 * used,
            "{} bytes of text from {used}",
            text.len()
        );
    }

    match file.problem {
        None => {
            let saved = encode(&file.text, format.encoding, format.bom)
                .expect("text that decoded cleanly encodes");
            assert!(
                saved == bytes,
                "{format:?} does not save back to the bytes read"
            );
            assert_eq!(file.changes.count, 0);
            true
        }
        Some(problem) => {
            assert!(
                problem.offset() <= bytes.len(),
                "{problem:?} past {} bytes",
                bytes.len()
            );
            let changes = &file.changes;
            assert!(changes.unwritable <= changes.count);
            assert!(changes.first.len() <= changes.count);
            false
        }
    }
}

/// A settings file, as the user's or the machine's settings and as a policy. Whatever it holds
/// resolves to valid settings; those settings, written out and resolved again, are the same.
pub fn settings(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        return false;
    };
    let user = resolve(Sources {
        user: Some(table.clone()),
        ..Sources::default()
    });
    let policy = resolve(Sources {
        policy: Some(table),
        ..Sources::default()
    });
    assert_eq!(policy.settings, user.settings);
    // The keys listed as set by policy are locked (unknown keys are locked but not listed).
    for (key, _) in policy.policies() {
        assert!(policy.is_locked(&key), "{key}");
    }

    let written = toml::Table::try_from(&user.settings).expect("settings serialize");
    let again = resolve(Sources {
        user: Some(written),
        ..Sources::default()
    });
    assert_eq!(again.settings, user.settings);
    assert!(again.diagnostics.is_empty(), "{:?}", again.diagnostics);

    // Preferences changes one setting in the file and leaves the others as they were.
    let two = toml::Value::Integer(2);
    if let Ok(edited) = birchpad_config::edit_settings(text, "editor.tab-width", Some(&two)) {
        let table = toml::from_str::<toml::Table>(&edited).expect("an edited file reads");
        let edited = resolve(Sources {
            user: Some(table),
            ..Sources::default()
        });
        assert_eq!(
            edited.settings.editor.tab_width, 2,
            "edited:
{edited:?}"
        );
        let mut expected = user.settings.clone();
        expected.editor.tab_width = 2;
        if user.diagnostics.is_empty() {
            assert_eq!(edited.settings, expected);
        }
    }
    user.diagnostics.is_empty()
}

/// The state file: a state that loads saves, and the saved state loads the same.
pub fn state(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Some(state) = UserState::parse(text) else {
        return false;
    };
    let saved = toml::to_string(&state).expect("a state that loads saves");
    let again = UserState::parse(&saved).expect("a saved state loads");
    assert_eq!(toml::to_string(&again).unwrap(), saved);
    true
}

/// A session file: Birchpad's own or Notepad++'s `session.xml`. A session that reads saves in
/// Birchpad's format, which reads the same.
pub fn session(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(session) = Session::from_text(text) else {
        return false;
    };
    let saved = session.to_toml();
    let again = Session::parse(&saved).expect("a saved session reads");
    assert_eq!(again.to_toml(), saved);
    assert!(session.backup_names().count() <= session.documents.len());
    true
}

/// A Project panel's `.workspace` file, in Notepad++'s format: a workspace that reads saves,
/// and the saved file reads the same.
pub fn workspace_file(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let base = Path::new(if cfg!(windows) {
        r"C:\projects\notes"
    } else {
        "/projects/notes"
    });
    let Ok(workspace) = ProjectWorkspace::parse(text, base) else {
        return false;
    };
    let saved = workspace.to_xml(base);
    let again = ProjectWorkspace::parse(&saved, base).expect("a saved workspace reads");
    assert_eq!(again, workspace, "saved as:\n{saved}");
    true
}

/// A theme file: one that reads writes back as a complete file that reads as the same theme.
pub fn theme(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(theme) = Theme::from_toml("Fuzzed", text) else {
        return false;
    };
    check_theme(&theme);
    true
}

/// A Notepad++ XML theme: what it imports saves as a Birchpad theme that reads the same.
pub fn notepad_theme(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(theme) = Theme::from_notepad_xml("Fuzzed", text) else {
        return false;
    };
    check_theme(&theme);
    true
}

fn check_theme(theme: &Theme) {
    let saved = theme.to_toml();
    let again = Theme::from_toml(&theme.name, &saved).expect("a saved theme reads");
    assert_eq!(
        &again, theme,
        "saved as:
{saved}"
    );
    assert_eq!(again.to_toml(), saved);
}

/// The user's `keymap.toml`, on each platform: the defaults stay whatever it holds, and every
/// binding's keys, as handed to GPUI, read back as the same keys.
pub fn keymap(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let mut accepted = true;
    for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
        let mut keymap = Keymap::with_defaults(platform);
        assert!(keymap.diagnostics().is_empty());
        keymap.add_layer(Layer::User, text);
        accepted &= keymap.diagnostics().is_empty();
        for binding in keymap.bindings() {
            let keys = binding.keys_string();
            assert_eq!(
                Keystroke::parse_sequence(&keys, platform).as_ref(),
                Ok(&binding.keys),
                "{keys}"
            );
            if let Some(shifted) = binding.shifted_symbol_keys() {
                assert!(
                    Keystroke::parse_sequence(&shifted, platform).is_ok(),
                    "{shifted}"
                );
            }
        }
    }
    // The Shortcut Mapper's changes to a keymap that reads cleanly: what it binds is bound,
    // what it unbinds is not, and resetting gives a command its default keys.
    if !accepted {
        return false;
    }
    let platform = Platform::Windows;
    let save = birchpad_commands::Invocation::new("file.save");
    let effective = |text: &str| {
        let mut keymap = Keymap::with_defaults(platform);
        keymap.add_layer(Layer::User, text);
        keymap
    };
    if let Ok(bound) = birchpad_commands::bind(text, platform, "ctrl-alt-shift-f12", None, &save) {
        let keymap = effective(&bound);
        let keys = Keystroke::parse_sequence("ctrl-alt-shift-f12", platform).unwrap();
        let found = keymap
            .resolve(&keys, &[])
            .map(|binding| &binding.invocation);
        assert_eq!(
            found,
            Some(&save),
            "bound:
{bound}"
        );
        let unbound = birchpad_commands::unbind(&bound, platform, "ctrl-alt-shift-f12", None)
            .expect("what was bound unbinds");
        assert!(
            effective(&unbound).resolve(&keys, &[]).is_none(),
            "unbound:
{unbound}"
        );
    }
    if let Ok(reset) = birchpad_commands::reset(text, platform, &save) {
        let defaults = Keymap::with_defaults(platform);
        let keys = |keymap: &Keymap| -> Vec<String> {
            keymap
                .bindings_for(&save)
                .map(|binding| binding.keys_string())
                .collect()
        };
        let keymap = effective(&reset);
        for default in keys(&defaults) {
            let parsed = Keystroke::parse_sequence(&default, platform).unwrap();
            // Unless the file binds those keys to something else.
            if keymap
                .resolve(&parsed, &[])
                .is_some_and(|binding| binding.invocation == save)
            {
                continue;
            }
            assert!(
                keymap
                    .bindings()
                    .iter()
                    .any(|binding| binding.keys == parsed),
                "{default} after reset:
{reset}"
            );
        }
    }
    accepted
}

/// The command line, split at NUL bytes: any arguments parse, and the result reaches a running
/// instance unchanged.
pub fn command_line(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    let args = text.split('\0').map(OsString::from);
    let cwd = Path::new(if cfg!(windows) {
        r"C:\Users\me"
    } else {
        "/home/me"
    });
    let mut line = CommandLine::parse(args, cwd);
    let _ = line.caret_target();
    // Warnings are shown by the instance that parsed the arguments, not sent.
    line.warnings.clear();
    let message = serde_json::to_string(&line).expect("command lines serialize");
    let received: CommandLine = serde_json::from_str(&message).expect("a sent command line reads");
    assert_eq!(received, line);
    !line.files.is_empty()
}

/// A message to a running instance on its socket, where any program can write.
pub fn instance_message(data: &[u8]) -> bool {
    serde_json::from_slice::<CommandLine>(data).is_ok()
}

const NOW: u64 = 1_790_000_000;

/// The update manifest. As served: any bytes are refused or read, never a crash. As signed:
/// the bytes are the manifest text, signed with a trusted key so that the manifest itself is
/// read, and a manifest that reads lists its releases.
pub fn manifest(data: &[u8]) -> bool {
    let key = SigningKey::from_bytes(&[7; 32]);
    let keys = [key.verifying_key()];
    let served = manifest::verify(data, &keys, NOW, 0).is_ok();

    let Ok(text) = std::str::from_utf8(data) else {
        return served;
    };
    let envelope = Envelope {
        manifest: text.to_owned(),
        signatures: vec![Signature {
            key: manifest::public_key_text(&keys[0]),
            signature: BASE64.encode(key.sign(text.as_bytes()).to_bytes()),
        }],
    };
    let bytes = serde_json::to_vec(&envelope).expect("an envelope serializes");
    // Never expired, never older than one seen before: only the format can refuse it.
    match manifest::verify(&bytes, &keys, 0, 0) {
        Ok(manifest) => {
            for channel in [
                UpdateChannel::Stable,
                UpdateChannel::Beta,
                UpdateChannel::Nightly,
            ] {
                let _ = manifest.newest(channel);
            }
            for release in &manifest.releases {
                let _ = release.package("windows-x64");
            }
            true
        }
        Err(_) => served,
    }
}
