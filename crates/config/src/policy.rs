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
        name: "UserScripts",
        key: "plugins.scripts",
        kind: PolicyKind::Bool,
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
pub(crate) mod registry;
