//! The online catalog (ADR 0051): the plugins Plugins Admin offers to install or update, and
//! the dictionaries the spell checker offers to download, each file with its size and SHA-256.
//! Birchpad reads it only when the user asks, and only while the `OnlineFeatures` policy
//! (`network.online-features`) allows it; what it downloads is checked against the catalog
//! before it is kept.
//!
//! ```json
//! {
//!   "format": 1,
//!   "plugins": [{ "id": "word-count", "name": "Word Count", "version": "1.2.0",
//!                 "description": "...", "author": "...", "api": "0.2",
//!                 "file": { "url": "https://.../word-count-1.2.0.zip", "size": 12345,
//!                           "sha256": "..." } }],
//!   "dictionaries": [{ "language": "en_GB", "name": "English (UK)",
//!                      "aff": { "url": "...", "size": 1, "sha256": "..." },
//!                      "dic": { "url": "...", "size": 1, "sha256": "..." } }]
//! }
//! ```

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::{Transport, hex};

/// The catalog Birchpad reads unless `network.catalog` names another.
pub const DEFAULT_CATALOG: &str =
    "https://raw.githubusercontent.com/SecurityTrip/Birchpad/main/packaging/catalog/catalog.json";
/// The format of catalog this Birchpad reads.
pub const FORMAT: u32 = 1;
/// The largest catalog read, in bytes.
pub const MAX_CATALOG: usize = 4 << 20;
/// The largest file of the catalog downloaded, in bytes.
pub const MAX_FILE: u64 = 64 << 20;

/// A file to download, as the catalog pins it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFile {
    pub url: String,
    pub size: u64,
    /// Lowercase or uppercase hex.
    pub sha256: String,
}

/// A plugin of the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// The plugin API it is built for.
    pub api: String,
    /// A ZIP with its `plugin.toml` at the top.
    pub file: CatalogFile,
}

/// A dictionary of the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDictionary {
    /// `en_GB`: the name its files are kept under.
    pub language: String,
    pub name: String,
    pub aff: CatalogFile,
    pub dic: CatalogFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: u32,
    #[serde(default)]
    pub plugins: Vec<CatalogPlugin>,
    #[serde(default)]
    pub dictionaries: Vec<CatalogDictionary>,
}

/// A catalog that does not read, or does not make sense.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("{}", birchpad_i18n::t!("update.catalog-does-not-read", reason = .0))]
    Syntax(String),
    #[error("{}", birchpad_i18n::t!("update.catalog-newer-format", format = .0, known = FORMAT))]
    Format(u32),
    #[error("{}", birchpad_i18n::t!("update.catalog-invalid", reason = .0))]
    Invalid(String),
}

fn check_file(file: &CatalogFile, what: &str) -> Result<(), CatalogError> {
    let invalid = |reason: String| Err(CatalogError::Invalid(format!("{what}: {reason}")));
    if !file.url.starts_with("https://") {
        return invalid(format!("{} is not an https address", file.url));
    }
    if file.size == 0 || file.size > MAX_FILE {
        return invalid(format!("{} bytes", file.size));
    }
    if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return invalid(format!("{} is not a SHA-256", file.sha256));
    }
    Ok(())
}

/// Whether `id` is a plugin id: lower-case letters, digits and dashes.
fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Whether `language` can name a dictionary's files: letters, digits, `_` and `-`.
fn is_language(language: &str) -> bool {
    !language.is_empty()
        && language.len() <= 32
        && language
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Reads and checks a catalog.
pub fn parse(bytes: &[u8]) -> Result<Catalog, CatalogError> {
    if bytes.len() > MAX_CATALOG {
        return Err(CatalogError::Invalid(format!("{} bytes", bytes.len())));
    }
    // The format first, so that a newer catalog says so rather than failing to read.
    #[derive(Deserialize)]
    struct Header {
        format: u32,
    }
    let header: Header =
        serde_json::from_slice(bytes).map_err(|error| CatalogError::Syntax(error.to_string()))?;
    if header.format != FORMAT {
        return Err(CatalogError::Format(header.format));
    }
    let catalog: Catalog =
        serde_json::from_slice(bytes).map_err(|error| CatalogError::Syntax(error.to_string()))?;
    for (index, plugin) in catalog.plugins.iter().enumerate() {
        if !is_id(&plugin.id) {
            return Err(CatalogError::Invalid(format!(
                "{:?} is not a plugin id",
                plugin.id
            )));
        }
        if catalog.plugins[..index]
            .iter()
            .any(|other| other.id == plugin.id)
        {
            return Err(CatalogError::Invalid(format!("{} comes twice", plugin.id)));
        }
        if plugin.name.trim().is_empty() || plugin.version.trim().is_empty() {
            return Err(CatalogError::Invalid(format!(
                "{} needs a name and a version",
                plugin.id
            )));
        }
        check_file(&plugin.file, &plugin.id)?;
    }
    for (index, dictionary) in catalog.dictionaries.iter().enumerate() {
        if !is_language(&dictionary.language) {
            return Err(CatalogError::Invalid(format!(
                "{:?} is not a language",
                dictionary.language
            )));
        }
        if catalog.dictionaries[..index]
            .iter()
            .any(|other| other.language == dictionary.language)
        {
            return Err(CatalogError::Invalid(format!(
                "{} comes twice",
                dictionary.language
            )));
        }
        check_file(&dictionary.aff, &dictionary.language)?;
        check_file(&dictionary.dic, &dictionary.language)?;
    }
    Ok(catalog)
}

/// The catalog at `url`.
pub fn fetch(transport: &dyn Transport, url: &str) -> Result<Catalog, String> {
    let body = transport.get(url).map_err(|reason| {
        birchpad_i18n::t!("update.cannot-download", url = url, reason = reason)
    })?;
    parse(&body).map_err(|error| error.to_string())
}

/// Whether the catalog's `offered` version is newer than the `installed` one: by semantic
/// versions where both are, otherwise when they differ.
pub fn is_newer(offered: &str, installed: &str) -> bool {
    match (
        semver::Version::parse(offered.trim()),
        semver::Version::parse(installed.trim()),
    ) {
        (Ok(offered), Ok(installed)) => offered > installed,
        _ => offered.trim() != installed.trim(),
    }
}

/// Checks that the file at `path` is what `file` pins.
fn verify(path: &Path, file: &CatalogFile) -> Result<(), String> {
    let mut opened = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0;
    loop {
        let read = opened
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    if size != file.size || !hex(&hasher.finalize()).eq_ignore_ascii_case(&file.sha256) {
        return Err(birchpad_i18n::t!("update.not-as-listed", url = file.url));
    }
    Ok(())
}

/// Downloads `file` to `to` and checks it; nothing is left at `to` if it fails.
pub fn download(
    transport: &dyn Transport,
    file: &CatalogFile,
    to: &Path,
    progress: &dyn Fn(u64),
) -> Result<(), String> {
    let result = transport
        .download(&file.url, to, file.size, progress)
        .map_err(|reason| {
            birchpad_i18n::t!("update.cannot-download", url = file.url, reason = reason)
        })
        .and_then(|()| verify(to, file));
    if result.is_err() {
        fs::remove_file(to).ok();
    }
    result
}

/// Downloads `plugin`'s ZIP into `work` (a folder of its own, emptied first), checks it and
/// unpacks it: the folder that holds its `plugin.toml`, to install from.
pub fn fetch_plugin(
    transport: &dyn Transport,
    plugin: &CatalogPlugin,
    work: &Path,
    progress: &dyn Fn(u64),
) -> Result<PathBuf, String> {
    fs::remove_dir_all(work).ok();
    fs::create_dir_all(work).map_err(|error| error.to_string())?;
    let archive = work.join("plugin.zip");
    let unpacked = work.join("plugin");
    download(transport, &plugin.file, &archive, progress)?;
    crate::component::unpack(&archive, &unpacked)?;
    fs::remove_file(&archive).ok();
    let manifest = unpacked.join("plugin.toml");
    if !manifest.is_file() {
        return Err(birchpad_i18n::t!(
            "update.no-plugin-toml",
            url = plugin.file.url
        ));
    }
    Ok(manifest)
}

/// Downloads `dictionary`'s `.aff` and `.dic` into `folder`, as `<language>.aff` and `.dic`,
/// replacing those there only once both have arrived and are as the catalog pins them.
pub fn fetch_dictionary(
    transport: &dyn Transport,
    dictionary: &CatalogDictionary,
    folder: &Path,
    progress: &dyn Fn(u64),
) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let language = &dictionary.language;
    let parts = [
        (&dictionary.aff, folder.join(format!("{language}.aff.part"))),
        (&dictionary.dic, folder.join(format!("{language}.dic.part"))),
    ];
    for (file, part) in &parts {
        if let Err(error) = download(transport, file, part, progress) {
            for (_, part) in &parts {
                fs::remove_file(part).ok();
            }
            return Err(error);
        }
    }
    let aff = folder.join(format!("{language}.aff"));
    let dic = folder.join(format!("{language}.dic"));
    for ((_, part), target) in parts.iter().zip([&aff, &dic]) {
        fs::rename(part, target).map_err(|error| error.to_string())?;
    }
    Ok((aff, dic))
}
