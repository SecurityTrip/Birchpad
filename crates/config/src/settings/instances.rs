//! Preferences > Multi-Instance & Date.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > Multi-Instance & Date.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InstanceSettings {
    pub mode: InstanceMode,
    /// The panels open in a run without a session (`-nosession`) are remembered too.
    pub nosession_panels: bool,
}

/// What a launch does while Birchpad runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstanceMode {
    /// It hands its files to the running Birchpad ("Default (mono-instance)").
    #[default]
    Single,
    /// As `single`, but a session given with `-openSession` opens in a Birchpad of its own
    /// ("Open session in a new instance").
    Session,
    /// Every launch is a Birchpad of its own ("Always in multi-instance mode").
    Multi,
}

impl InstanceSettings {
    /// Whether a launch with `open_session` (`-openSession`) starts a Birchpad of its own.
    pub fn new_instance(&self, open_session: bool) -> bool {
        match self.mode {
            InstanceMode::Single => false,
            InstanceMode::Session => open_session,
            InstanceMode::Multi => true,
        }
    }
}

/// Edit > Insert > Date Time: Notepad++'s "Customize insert Date Time".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DateSettings {
    /// Date Time (customized), in Notepad++'s letters: `yyyy-MM-dd HH:mm:ss`.
    pub custom_format: String,
    /// The date before the time in the short and long formats ("Reverse default date time
    /// order").
    pub reverse: bool,
}

impl Default for DateSettings {
    fn default() -> Self {
        Self {
            custom_format: "yyyy-MM-dd HH:mm:ss".to_owned(),
            reverse: false,
        }
    }
}
