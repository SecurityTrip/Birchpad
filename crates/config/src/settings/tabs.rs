//! Preferences > Tab Bar.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > General > Tab Bar, and its Document Switcher (Ctrl+Tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TabSettings {
    /// The tabs in a column on the left of each view ("Vertical").
    pub vertical: bool,
    /// Tabs that do not fit go on further rows instead of scrolling ("Multi-line").
    pub multi_line: bool,
    /// Smaller tabs ("Reduce").
    pub reduce: bool,
    /// Tabs cannot be dragged ("Lock (no drag and drop)").
    pub lock: bool,
    /// A close button on every tab ("Show close button on each tab").
    pub close_button: bool,
    /// A double click on a tab closes it ("Double click to close document").
    pub double_click_close: bool,
    /// Hovering over a tab that is not active shows the start of its text (the document
    /// peeker).
    pub peeker: bool,
    /// Ctrl+Tab shows the documents of the view in a list while Ctrl is held ("Document
    /// Switcher"); off, it goes to the next tab.
    pub switcher: bool,
    /// The switcher lists the documents most recently used first, instead of in tab order
    /// ("Enable MRU behaviour").
    pub switcher_mru: bool,
    /// Closing the last tab quits ("Exit on close the last tab").
    pub exit_on_last_close: bool,
    /// Labels longer than this many characters are cut, `…` at their end; 0 for no limit
    /// ("Max tab label length").
    pub max_label_length: u16,
    /// A check mark, a pencil and a lock for saved, changed and read-only documents in place
    /// of the dot ("Alternate icons").
    pub alternate_icons: bool,
    /// Inactive tabs have a background of their own ("Change inactive tab color").
    pub inactive_color: bool,
    /// A line of the accent color marks the active tab ("Draw a coloured bar on active tab").
    pub active_bar: bool,
    /// Tabs can be pinned ("Enable pin tab feature").
    pub pinning: bool,
    /// Only pinned tabs show the pin; off, every tab has a pin button that pins it ("Show only
    /// pinned button").
    pub only_pinned_button: bool,
    /// Inactive tabs show their close and pin buttons too ("Show buttons on inactive tabs").
    pub inactive_buttons: bool,
}

impl TabSettings {
    /// A tab's label: `name`, cut to the maximum length with `…` at its end.
    pub fn label(&self, name: &str) -> String {
        let max = usize::from(self.max_label_length);
        if max == 0 || name.chars().count() <= max {
            return name.to_owned();
        }
        let mut cut: String = name.chars().take(max.saturating_sub(1).max(1)).collect();
        cut.push('…');
        cut
    }
}

impl Default for TabSettings {
    fn default() -> Self {
        Self {
            vertical: false,
            multi_line: false,
            reduce: false,
            lock: false,
            close_button: true,
            double_click_close: false,
            peeker: true,
            switcher: true,
            switcher_mru: true,
            exit_on_last_close: false,
            max_label_length: 0,
            alternate_icons: false,
            inactive_color: true,
            active_bar: true,
            pinning: true,
            only_pinned_button: true,
            inactive_buttons: true,
        }
    }
}
