//! The install statistics update checks send to Birchpad's Update API (ADR 0064): which copy of
//! Birchpad asks (a random id it made for itself), its version, channel and platform, and which
//! PlantUML its previews draw with. Only these fields, named as the API's contract names them;
//! with statistics off, nothing (`{}`).

use uuid::Uuid;

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
