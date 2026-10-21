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
use birchpad_io::{
    CHARACTER_SETS, FileInfo, LoadOptions, decode_appended, decode_file, encode_document,
};
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
    ("macros", macros),
    ("run_commands", run_commands),
    ("api_file", api_file),
    ("udl", udl),
    ("keymap", keymap),
    ("context_menu", context_menu),
    ("command_line", command_line),
    ("instance_message", instance_message),
    ("manifest", manifest),
    ("plugin_manifest", plugin_manifest),
    ("catalog", catalog),
    ("json", json),
    ("json_tree", json_tree),
    ("xml", xml),
    ("xpath", xpath),
    ("xml_schema", xml_schema),
    ("hex_dump", hex_dump),
    ("spell_dictionary", spell_dictionary),
    ("search_query", search_query),
    ("plugin_calls", plugin_calls),
    ("rhai_script", rhai_script),
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
            // As Birchpad saves it: an EBCDIC file read with NEL gets NEL back.
            let saved =
                encode_document(&file.text, format).expect("text that decoded cleanly encodes");
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
        assert_eq!(edited.settings.editor.tab_width, 2, "edited:\n{edited:?}");
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

/// `macros.toml`: macros that read write back as a file that reads the same.
pub fn macros(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(macros) = birchpad_commands::parse_macros(text) else {
        return false;
    };
    let written = birchpad_commands::write_macros(&macros);
    let again = birchpad_commands::parse_macros(&written).expect("written macros read");
    assert_eq!(again, macros, "written:\n{written}");
    true
}

/// `context-menu.toml`: a menu that reads writes back the same, and holds only commands the
/// catalog knows, without separators at its ends or twice in a row.
pub fn context_menu(data: &[u8]) -> bool {
    use birchpad_commands::context_menu::{Node, parse, write};
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(nodes) = parse(text) else {
        return false;
    };
    fn check(nodes: &[Node]) {
        assert_ne!(nodes.first(), Some(&Node::Separator));
        assert_ne!(nodes.last(), Some(&Node::Separator));
        for pair in nodes.windows(2) {
            assert!(pair != [Node::Separator, Node::Separator]);
        }
        for node in nodes {
            match node {
                Node::Command { invocation, .. } => {
                    assert!(birchpad_commands::find(&invocation.command).is_some());
                }
                Node::Folder { items, .. } => {
                    assert!(!items.is_empty());
                    check(items);
                }
                Node::Separator => {}
            }
        }
    }
    check(&nodes);
    let written = write(&nodes);
    let again = parse(&written).expect("a written menu reads");
    assert_eq!(
        again, nodes,
        "written:
{written}"
    );
    true
}

/// `run.toml`: commands that read write back the same, and any line expands without a panic.
pub fn run_commands(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let vars = birchpad_commands::RunVariables {
        file_name: text.lines().next().unwrap_or_default().to_owned(),
        current_word: "word".into(),
        ..Default::default()
    };
    let expanded = birchpad_commands::expand_variables(text, &vars, |name| Some(name.to_owned()));
    if !text.contains("$(") {
        assert_eq!(expanded, text);
    }
    let Ok(commands) = birchpad_commands::parse_run_commands(text) else {
        return false;
    };
    let written = birchpad_commands::write_run_commands(&commands);
    assert_eq!(
        birchpad_commands::parse_run_commands(&written).expect("written commands read"),
        commands
    );
    true
}

/// A Notepad++ API file: what reads has sorted, distinct, named keywords, and every function
/// completes and finds itself.
pub fn api_file(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(api) = birchpad_config::ApiFile::parse(text) else {
        return false;
    };
    let names: Vec<&str> = api
        .keywords
        .iter()
        .map(|keyword| keyword.name.as_str())
        .collect();
    assert!(
        names.is_sorted() && names.windows(2).all(|pair| pair[0] != pair[1]),
        "{names:?}"
    );
    for keyword in &api.keywords {
        assert!(!keyword.name.trim().is_empty());
        if !keyword.overloads.is_empty() {
            assert!(api.function(&keyword.name).is_some(), "{}", keyword.name);
        }
        let prefix: String = keyword.name.chars().take(1).collect();
        if prefix.len() < keyword.name.len() {
            assert!(
                api.completions(&prefix)
                    .iter()
                    .any(|found| found.name == keyword.name),
                "{}",
                keyword.name
            );
        }
    }
    true
}

/// A Notepad++ user defined language file: every language it holds colors its own text, and
/// the file itself, with pieces in order, apart, and on character boundaries.
pub fn udl(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(languages) = birchpad_syntax::parse_udl(text) else {
        return false;
    };
    for language in &languages {
        assert!(!language.name.trim().is_empty());
        let pieces = language.highlight(text, 0);
        let mut end = 0;
        for (range, _) in &pieces {
            assert!(range.start >= end && range.start < range.end && range.end <= text.len());
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            end = range.end;
        }
        let from = text.len() / 2;
        let later = language.highlight(text, text.floor_char_boundary(from));
        assert!(later.iter().all(|piece| pieces.contains(piece)));
    }
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
    assert_eq!(&again, theme, "saved as:\n{saved}");
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
        assert_eq!(found, Some(&save), "bound:\n{bound}");
        let unbound = birchpad_commands::unbind(&bound, platform, "ctrl-alt-shift-f12", None)
            .expect("what was bound unbinds");
        assert!(
            effective(&unbound).resolve(&keys, &[]).is_none(),
            "unbound:\n{unbound}"
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
        // Each default key of the command is bound again: to it, or to what the file binds.
        for default in keys(&defaults) {
            let parsed = Keystroke::parse_sequence(&default, platform).unwrap();
            let bound = keymap
                .bindings()
                .iter()
                .any(|binding| binding.keys == parsed);
            assert!(bound, "{default} after reset:\n{reset}");
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
    // Wildcards would be matched on the fuzzing machine's disk: `-r /*` walks all of it.
    let mut line = CommandLine::parse_unexpanded(args, cwd);
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

/// A plugin's `plugin.toml`: one that reads writes back as TOML that reads as the same.
pub fn plugin_manifest(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let Ok(manifest) = birchpad_plugin::Manifest::parse(text) else {
        return false;
    };
    assert!(birchpad_plugin::manifest::is_valid_id(&manifest.id));
    assert_eq!(
        birchpad_plugin::Manifest::parse(&manifest.to_toml()).as_ref(),
        Ok(&manifest)
    );
    true
}

/// A document checked, formatted and minified as JSON: what is JSON formats and minifies to
/// JSON with the same tokens, whatever the layout; what is not is refused by all three.
pub fn json(data: &[u8]) -> bool {
    use birchpad_tools::json;
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let layout = birchpad_tools::Layout::default();
    if let Err(error) = json::validate(text) {
        assert!(error.offset <= text.len());
        assert!(json::format(text, layout).is_err() && json::minify(text).is_err());
        return false;
    }
    let small = json::minify(text).unwrap();
    let pretty = json::format(text, layout).unwrap();
    assert_eq!(json::minify(&pretty).unwrap(), small);
    assert_eq!(json::minify(&small).unwrap(), small);
    assert_eq!(json::format(&pretty, layout).unwrap(), pretty);
    true
}

/// The online catalog of plugins and dictionaries: one that reads names only https files of a
/// size Birchpad downloads, with a SHA-256 each, and plugins and languages once.
pub fn catalog(data: &[u8]) -> bool {
    use birchpad_update::catalog::{MAX_FILE, parse};
    let Ok(catalog) = parse(data) else {
        return false;
    };
    let files = catalog.plugins.iter().map(|plugin| &plugin.file).chain(
        catalog
            .dictionaries
            .iter()
            .flat_map(|dictionary| [&dictionary.aff, &dictionary.dic]),
    );
    for file in files {
        assert!(file.url.starts_with("https://"));
        assert!(file.size > 0 && file.size <= MAX_FILE);
        assert_eq!(file.sha256.len(), 64);
    }
    let mut ids: Vec<&str> = catalog
        .plugins
        .iter()
        .map(|plugin| plugin.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), catalog.plugins.len());
    true
}

/// JSON read as JSON Viewer's tree: a text is a tree exactly when it is JSON; every value lies
/// within its parent; the path of a value finds a value of that path; and the text with its
/// keys sorted is JSON with the same values, which sorting again leaves as it is.
pub fn json_tree(data: &[u8]) -> bool {
    use birchpad_tools::json;
    use birchpad_tools::json_tree::{self, JsonTree};
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let layout = birchpad_tools::Layout::default();
    let Ok(tree) = JsonTree::parse(text) else {
        assert!(json::validate(text).is_err());
        assert!(json_tree::sort_keys(text, layout).is_err());
        return false;
    };
    assert!(json::validate(text).is_ok());
    for (index, node) in tree.nodes.iter().enumerate() {
        assert!(node.range.start < node.range.end && node.range.end <= text.len());
        if let Some(parent) = node.parent {
            let outer = &tree.nodes[parent].range;
            assert!(outer.start < node.range.start && node.range.end < outer.end);
            assert!(tree.nodes[parent].children.contains(&index));
        }
        if index < 200 {
            let path = tree.path(index);
            let found = tree.find(&path).unwrap();
            assert_eq!(tree.path(found), path);
        }
    }
    let sorted = json_tree::sort_keys(text, layout).unwrap();
    let again = JsonTree::parse(&sorted).unwrap();
    assert_eq!(again.nodes.len(), tree.nodes.len());
    assert_eq!(json_tree::sort_keys(&sorted, layout).unwrap(), sorted);
    true
}

/// An XPath expression over a document: the bytes up to the first NUL are the document, the
/// rest the expression. It is evaluated or refused; the nodes it gives are in the document,
/// in order.
pub fn xpath(data: &[u8]) -> bool {
    use birchpad_tools::xpath::{XPathResult, evaluate};
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let (document, expression) = text.split_once('\0').unwrap_or((text, "/*"));
    match evaluate(document, expression) {
        Ok(XPathResult::Nodes(nodes)) => {
            for node in &nodes {
                assert!(node.range.start <= node.range.end && node.range.end <= document.len());
                assert!(document.is_char_boundary(node.range.start));
                assert!(document.is_char_boundary(node.range.end));
            }
            true
        }
        Ok(XPathResult::Value(_)) => true,
        Err(_) => false,
    }
}

/// A document validated against a DTD or an XML Schema: the bytes up to the first NUL are the
/// document, the rest the external DTD, or the schema when it is one. Validation ends with a
/// list of problems in the document, or refuses what is not XML.
pub fn xml_schema(data: &[u8]) -> bool {
    use birchpad_tools::xml_schema::{doctype, schema_location, validate_dtd, validate_xsd};
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let (document, other) = text.split_once('\0').unwrap_or((text, ""));
    let _ = schema_location(document);
    let result = if other.contains("schema") {
        validate_xsd(document, other)
    } else if doctype(document).is_some() {
        validate_dtd(document, (!other.is_empty()).then_some(other))
    } else {
        return false;
    };
    match result {
        Ok(problems) => {
            assert!(problems.len() <= birchpad_tools::xml_schema::MAX_PROBLEMS);
            for problem in &problems {
                assert!(problem.offset <= document.len());
            }
            true
        }
        Err(_) => false,
    }
}

/// A document checked, pretty printed and linearized as XML: what is well formed stays well
/// formed both ways, and pretty printing twice changes nothing more.
pub fn xml(data: &[u8]) -> bool {
    use birchpad_tools::xml;
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let layout = birchpad_tools::Layout::default();
    if let Err(error) = xml::check(text) {
        assert!(error.offset <= text.len());
        assert!(xml::pretty_print(text, layout).is_err() && xml::linearize(text).is_err());
        return false;
    }
    let pretty = xml::pretty_print(text, layout).unwrap();
    let line = xml::linearize(text).unwrap();
    assert!(xml::check(&pretty).is_ok(), "{pretty}");
    assert!(xml::check(&line).is_ok(), "{line}");
    assert_eq!(xml::pretty_print(&pretty, layout).unwrap(), pretty);
    assert_eq!(xml::linearize(&line).unwrap(), line);
    true
}

/// A hex dump edited by hand: what reads dumps back to rows that read as the same bytes.
pub fn hex_dump(data: &[u8]) -> bool {
    use birchpad_tools::hex;
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    // The same text as a pattern of View in HEX's Find: two digits a byte.
    if let Ok(pattern) = hex::parse_pattern(text) {
        let digits = text.chars().filter(|c| !c.is_whitespace()).count();
        assert_eq!(pattern.len() * 2, digits);
    }
    let Ok(bytes) = hex::parse(text) else {
        return false;
    };
    assert_eq!(
        hex::parse(&hex::dump(
            &bytes, "
"
        ))
        .unwrap(),
        bytes
    );
    true
}

/// A Hunspell dictionary: the `.aff` file, a NUL byte, then the `.dic` file. A dictionary that
/// reads checks and suggests words, and finds the misspelled ones in a text, without a panic.
pub fn spell_dictionary(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    let (aff, dic) = text.split_once('\0').unwrap_or((&text, ""));
    let Ok(mut speller) = birchpad_tools::spell::Speller::new(aff, dic) else {
        return false;
    };
    let words: Vec<&str> = dic
        .lines()
        .skip(1)
        .take(8)
        .filter_map(|line| line.split(['/', '\t', ' ']).next())
        .chain(["", "a", "Word", "naïve", "x-y", "o'clock"])
        .collect();
    for word in &words {
        speller.check(word);
        // Suggestions are slow by design: a few short words.
        if word.chars().count() <= 12 {
            speller.suggest(word);
        }
    }
    let sample: String = dic.chars().take(500).collect();
    for range in speller.misspelled(&sample) {
        assert!(sample.is_char_boundary(range.start) && sample.is_char_boundary(range.end));
    }
    speller.add("zzyzx");
    assert!(speller.check("zzyzx"));
    true
}

/// A search: the first byte chooses the mode and options; then the pattern, a NUL, the text, a
/// NUL and the replacement. A query that reads finds matches in order, inside the text and on
/// character boundaries, and replaces them; the pattern read as Find in Files' filters takes
/// or leaves out any name.
pub fn search_query(data: &[u8]) -> bool {
    use birchpad_core::Rope;
    use birchpad_core::search::{Query, SearchMode, Searcher};

    let Some((&flags, rest)) = data.split_first() else {
        return false;
    };
    let text = String::from_utf8_lossy(rest);
    let mut parts = text.splitn(3, '\0');
    let (pattern, haystack, template) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    if let Ok(filters) = birchpad_io::Filters::parse(pattern) {
        for name in haystack.split(['/', '\\', '\n']).take(16) {
            filters.takes_file(name);
            filters.leaves_out_folder(name, flags & 0x20 != 0);
        }
    }
    let query = Query {
        pattern: pattern.to_owned(),
        match_case: flags & 0x04 != 0,
        whole_word: flags & 0x08 != 0,
        mode: match flags % 3 {
            0 => SearchMode::Normal,
            1 => SearchMode::Extended,
            _ => SearchMode::Regex,
        },
        dot_matches_newline: flags & 0x10 != 0,
    };
    let Ok(searcher) = Searcher::new(&query) else {
        return false;
    };
    let rope = Rope::from_str(haystack);
    let on_boundaries = |range: &std::ops::Range<usize>| {
        range.start <= range.end
            && range.end <= haystack.len()
            && haystack.is_char_boundary(range.start)
            && haystack.is_char_boundary(range.end)
    };
    let found = searcher.find_all(&rope);
    for pair in found.windows(2) {
        assert!(pair[0].start <= pair[1].start, "in order: {found:?}");
    }
    for range in &found {
        assert!(
            on_boundaries(range),
            "{range:?} in {} bytes",
            haystack.len()
        );
        assert!(
            searcher.is_match(&rope, range.clone()),
            "{range:?} is a match"
        );
    }
    for (range, _) in searcher.replacements(&rope, 0..rope.len(), template) {
        assert!(on_boundaries(&range), "{range:?}");
    }
    true
}

/// The calls a plugin makes into the host, with any arguments: the text up to the first NUL
/// byte is the document, each byte after it chooses a call and the bytes after it its
/// arguments. Every call is done or refused; the document stays text; what the calls did,
/// applied to the document as it was, gives the text they left; a setting set reads back;
/// files stay out of reach of a plugin the user did not allow them.
pub fn plugin_calls(data: &[u8]) -> bool {
    use birchpad_core::{Rope, Selection};
    use birchpad_plugin::{Control, DocumentState, Session};

    let (document, calls) = match data.iter().position(|&byte| byte == 0) {
        Some(at) => (&data[..at], &data[at + 1..]),
        None => (data, &[][..]),
    };
    let original = Rope::from_str(&String::from_utf8_lossy(document));
    let mut session = Session::new(DocumentState {
        text: original.clone(),
        selection: Selection::point(0),
        path: None,
        language: "text".into(),
    });
    let mut bytes = calls.iter().copied();
    let number = |bytes: &mut dyn Iterator<Item = u8>| {
        let high = usize::from(bytes.next().unwrap_or(0));
        let low = usize::from(bytes.next().unwrap_or(0));
        // Past the end now and then.
        (high << 8 | low) % 600
    };
    let word = |bytes: &mut dyn Iterator<Item = u8>| {
        let len = usize::from(bytes.next().unwrap_or(0) % 12);
        let raw: Vec<u8> = bytes.take(len).collect();
        String::from_utf8_lossy(&raw).into_owned()
    };
    let mut done = 0;
    while let Some(call) = bytes.next() {
        let accepted = match call % 13 {
            0 => {
                let (start, end) = (number(&mut bytes), number(&mut bytes));
                session.text_range(start, end).is_ok()
            }
            1 => {
                let (start, end) = (number(&mut bytes), number(&mut bytes));
                session.replace(start, end, &word(&mut bytes)).is_ok()
            }
            2 => session.set_text(&word(&mut bytes)).is_ok(),
            3 => session.insert(&word(&mut bytes)).is_ok(),
            4 => {
                let ranges = [
                    (number(&mut bytes), number(&mut bytes)),
                    (number(&mut bytes), number(&mut bytes)),
                ];
                let count = 1 + usize::from(bytes.next().unwrap_or(0) % 2);
                session.set_selections(&ranges[..count]).is_ok()
            }
            5 => {
                let id = match bytes.next().unwrap_or(0) % 3 {
                    0 => "edit.select-all".to_owned(),
                    1 => "view.word-wrap".to_owned(),
                    _ => word(&mut bytes),
                };
                session.run_command_json(&id, &word(&mut bytes)).is_ok()
            }
            6 => {
                session.message(&word(&mut bytes));
                true
            }
            7 => {
                let lines = vec![word(&mut bytes), word(&mut bytes)];
                session.show_panel(&word(&mut bytes), lines);
                true
            }
            8 => {
                let (key, value) = (word(&mut bytes), word(&mut bytes));
                let set = session.set_setting(&key, &value).is_ok();
                if set {
                    assert_eq!(session.setting(&key).as_deref(), Some(value.as_str()));
                }
                set
            }
            9 => {
                let controls = vec![
                    Control::Button {
                        id: word(&mut bytes),
                        title: word(&mut bytes),
                    },
                    Control::TextField {
                        id: word(&mut bytes),
                        value: word(&mut bytes),
                    },
                    Control::Label(word(&mut bytes)),
                ];
                session.show_controls(&word(&mut bytes), controls).is_ok()
            }
            10 => session
                .start_task(&word(&mut bytes), &word(&mut bytes))
                .is_ok(),
            11 => {
                let path = word(&mut bytes);
                assert!(session.read_file(&path).is_err(), "{path:?}");
                assert!(session.write_file(&path, "x").is_err(), "{path:?}");
                assert!(session.open_file(&path).is_err(), "{path:?}");
                true
            }
            _ => {
                let text = session.text();
                assert_eq!(text.len(), session.len());
                let selected = session.selected_text();
                assert!(selected.len() <= text.len() * 2 + 1);
                true
            }
        };
        done += usize::from(accepted);
    }
    let text = session.text();
    let len = text.len();
    for (anchor, head) in session.selections() {
        assert!(anchor <= len && head <= len, "{anchor}, {head} in {len}");
    }
    let outcome = session.finish();
    let mut applied = original;
    if let Some(transaction) = &outcome.transaction {
        transaction.changes().apply(&mut applied);
    }
    assert_eq!(applied.to_string(), text);
    done > 0 || calls.is_empty()
}

/// A Rhai script, run over a small document within the limits on operations, strings, arrays,
/// maps, calls and expressions: it finishes or fails, never hangs or crashes Birchpad, and what
/// it did applies to the document.
pub fn rhai_script(data: &[u8]) -> bool {
    use birchpad_core::{Rope, Selection};
    use birchpad_plugin::{DocumentState, Session};

    let source = String::from_utf8_lossy(data);
    let original = Rope::from_str("one two\nthree\n");
    let session = Session::new(DocumentState {
        text: original.clone(),
        selection: Selection::point(4),
        path: Some("notes.txt".into()),
        language: "text".into(),
    });
    match birchpad_plugin::script::run_limited("fuzz", &source, session, 200_000) {
        Ok(outcome) => {
            let mut text = original;
            if let Some(transaction) = &outcome.transaction {
                transaction.changes().apply(&mut text);
            }
            true
        }
        Err(_) => false,
    }
}
