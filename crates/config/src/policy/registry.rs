use toml::{Table, Value};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};

use super::{POLICIES, PolicyKind, REGISTRY_KEY};
use crate::layers::{Diagnostic, Layer, set_path};

/// Reads machine and user policies; machine policies win.
pub(crate) fn read() -> (Table, Vec<Diagnostic>) {
    let mut table = Table::new();
    let mut diagnostics = Vec::new();
    for root in [CURRENT_USER, LOCAL_MACHINE] {
        if let Ok(key) = root.open(REGISTRY_KEY) {
            read_key(&key, &mut table, &mut diagnostics);
        }
    }
    (table, diagnostics)
}

pub(crate) fn read_key(key: &Key, table: &mut Table, diagnostics: &mut Vec<Diagnostic>) {
    for policy in POLICIES {
        // A missing value means the policy is not configured.
        if key.get_type(policy.name).is_err() {
            continue;
        }
        let value = match policy.kind {
            PolicyKind::Bool => key.get_u32(policy.name).map(|v| Value::Boolean(v != 0)),
            PolicyKind::Text => key.get_string(policy.name).map(Value::String),
            // REG_MULTI_SZ ends with an empty string terminator; empty entries mean nothing.
            PolicyKind::List => key.get_multi_string(policy.name).map(|items| {
                let items = items.into_iter().filter(|item| !item.is_empty());
                Value::Array(items.map(Value::String).collect())
            }),
        };
        match value {
            Ok(value) => set_path(table, policy.key, value),
            Err(error) => diagnostics.push(Diagnostic {
                layer: Layer::Policy,
                key: Some(policy.key.to_owned()),
                message: format!("cannot read registry value {}: {error}", policy.name),
            }),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/policy/registry/tests.rs"]
mod tests;
