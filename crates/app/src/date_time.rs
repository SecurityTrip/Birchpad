//! Edit > Insert > Date Time (short, long, customized), as Notepad++'s: the current local time
//! as text at every caret, in the formats of `birchpad_tools::date_time`. The customized format
//! and the order of the short and long ones are Preferences > Multi-Instance & Date
//! (`date.custom-format`, `date.reverse`).

use birchpad_tools::date_time::{format, long, short};
use serde::Deserialize;

use crate::commands::{CommandRegistry, Register as _};

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.editor("edit.insert-date-time", |this, args: DateArgs, _, cx| {
        let settings = crate::app_state::AppState::global(cx).settings.date.clone();
        let now = chrono::Local::now().naive_local();
        let text = match args.format {
            Kind::Short => format(&short(settings.reverse), &now),
            Kind::Long => format(&long(settings.reverse), &now),
            Kind::Custom => format(&settings.custom_format, &now),
        };
        this.insert(&text, crate::editor::LastEdit::None, cx);
        Ok(())
    });
}

#[derive(Deserialize)]
struct DateArgs {
    format: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    Short,
    Long,
    Custom,
}
