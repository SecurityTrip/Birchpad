//! Administrator policies.
//!
//! On Windows, policies come from the registry keys that Group Policy and Intune write:
//! `HKCU\Software\Policies\Birchpad`, overridden by `HKLM\Software\Policies\Birchpad`. Elsewhere
//! they come from a TOML file only the administrator can write (see [`crate::ConfigPaths`]).
//!
//! [`POLICIES`] is the single list of supported policies; ADMX templates are generated from it.

/// Registry path of the policy key, relative to `HKLM` or `HKCU`.
pub const REGISTRY_KEY: &str = r"Software\Policies\Birchpad";

/// How a policy is stored in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyKind {
    /// `REG_DWORD`: 0 is false, anything else is true.
    Bool,
    /// `REG_SZ`.
    Text,
    /// `REG_MULTI_SZ`.
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyDef {
    /// Registry value name.
    pub name: &'static str,
    /// Dotted settings key the policy sets.
    pub key: &'static str,
    pub kind: PolicyKind,
}

pub const POLICIES: &[PolicyDef] = &[
    PolicyDef {
        name: "UpdateMode",
        key: "updates.mode",
        kind: PolicyKind::Text,
    },
    PolicyDef {
        name: "UpdateChannel",
        key: "updates.channel",
        kind: PolicyKind::Text,
    },
    PolicyDef {
        name: "UpdateUrl",
        key: "updates.url",
        kind: PolicyKind::Text,
    },
    PolicyDef {
        name: "PluginInstall",
        key: "plugins.install",
        kind: PolicyKind::Text,
    },
    PolicyDef {
        name: "AllowedPlugins",
        key: "plugins.allowed",
        kind: PolicyKind::List,
    },
    PolicyDef {
        name: "BackupUnsaved",
        key: "session.backup-unsaved",
        kind: PolicyKind::Bool,
    },
    PolicyDef {
        name: "OnlineFeatures",
        key: "network.online-features",
        kind: PolicyKind::Bool,
    },
    PolicyDef {
        name: "CrashReports",
        key: "diagnostics.crash-reports",
        kind: PolicyKind::Bool,
    },
];

#[cfg(windows)]
pub(crate) mod registry {
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
    mod tests {
        use super::*;

        #[test]
        fn reads_typed_values() {
            let path = format!(r"Software\Birchpad-tests\policy-{}", std::process::id());
            let key = CURRENT_USER.create(&path).unwrap();
            key.set_string("UpdateMode", "off").unwrap();
            key.set_u32("CrashReports", 0).unwrap();
            key.set_multi_string("AllowedPlugins", &["compare", "json-tools"])
                .unwrap();
            key.set_string("BackupUnsaved", "not a dword").unwrap();

            let mut table = Table::new();
            let mut diagnostics = Vec::new();
            read_key(&key, &mut table, &mut diagnostics);
            CURRENT_USER.remove_tree(&path).unwrap();

            let expected: Table = toml::from_str(
                "updates.mode = 'off'\n\
                 diagnostics.crash-reports = false\n\
                 plugins.allowed = ['compare', 'json-tools']",
            )
            .unwrap();
            assert_eq!(table, expected);
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(
                diagnostics[0].key.as_deref(),
                Some("session.backup-unsaved")
            );
        }
    }
}
