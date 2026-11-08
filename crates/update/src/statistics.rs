//! The install statistics update checks send to Birchpad's Update API (ADR 0064): which copy of
//! Birchpad asks (a random id it made for itself), its version, channel and platform, and which
//! PlantUML its previews draw with. Only these fields, named as the API's contract names them;
//! with statistics off, nothing (`{}`).

use birchpad_config::UpdateChannel;
use serde::Serialize;
use uuid::Uuid;

/// What a copy of Birchpad tells the Update API about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    /// Its install id ([`new_install_id`]), kept in `state.toml`.
    pub install_id: String,
    /// The running version (`0.8.0`, `0.8.0-nightly.20261110.5`).
    pub version: String,
    /// `updates.channel`.
    pub channel: UpdateChannel,
    /// `windows-x64`, `windows-arm64`, `linux-x64`, `macos-arm64` or `macos-x64`; `None`
    /// elsewhere.
    pub platform: Option<String>,
    pub plantuml: PlantUml,
}

/// Which PlantUML the previews draw with, as they choose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlantUml {
    /// The one Birchpad installed (ADR 0045), of this version and revision (`1.2026.9-3`).
    Component { version: String },
    /// Another one: the `preview.plantuml` setting, or a `plantuml` on `PATH`.
    Custom,
    /// None found.
    None,
}

/// The fields of the API's contract, camelCase; those that are empty are left out.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Fields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    install_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
    channel: UpdateChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<&'a str>,
    plantuml: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    plantuml_version: Option<&'a str>,
}

impl Serialize for Statistics {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (plantuml, plantuml_version) = match &self.plantuml {
            PlantUml::Component { version } => ("component", filled(version)),
            PlantUml::Custom => ("custom", None),
            PlantUml::None => ("none", None),
        };
        Fields {
            install_id: filled(&self.install_id),
            version: filled(&self.version),
            channel: self.channel,
            platform: self.platform.as_deref().and_then(filled),
            plantuml,
            plantuml_version,
        }
        .serialize(serializer)
    }
}

/// `text`, unless it is empty.
fn filled(text: &str) -> Option<&str> {
    Some(text).filter(|text| !text.trim().is_empty())
}

/// The body of a check's request to the Update API: the fields of `statistics`, or `{}` without
/// them (statistics off), which says nothing a download of the manifest does not.
pub fn request_body(statistics: Option<&Statistics>) -> Vec<u8> {
    match statistics {
        Some(statistics) => serde_json::to_vec(statistics).expect("plain fields serialize"),
        None => b"{}".to_vec(),
    }
}

/// A new install id: a random UUID, version 4, derived from nothing on the computer.
pub fn new_install_id() -> String {
    Uuid::new_v4().to_string()
}

/// Whether `id` is an install id: a UUID of version 4 (`0b3c6f1e-6a1f-4c4e-9a59-3c1f7f0d2a11`).
pub fn is_install_id(id: &str) -> bool {
    Uuid::try_parse(id).is_ok_and(|uuid| uuid.get_version_num() == 4)
}

/// Keeps the install id `id` as statistics need it: with `statistics` on, the one there is, or
/// a new one when there is none (or what is there is no install id); off, none. Returns whether
/// it changed, and so is to be saved.
pub fn keep_install_id(id: &mut Option<String>, statistics: bool) -> bool {
    let kept = match id.as_deref() {
        _ if !statistics => None,
        Some(id) if is_install_id(id) => Some(id.to_owned()),
        _ => Some(new_install_id()),
    };
    let changed = kept != *id;
    *id = kept;
    changed
}
