//! Edit > Insert > Date Time (short, long, customized), as Notepad++'s: the current local time
//! as text at every caret. The customized format and the order of the short and long ones are
//! Preferences > Multi-Instance & Date (`date.custom-format`, `date.reverse`).
//!
//! Formats use Notepad++'s (Windows') letters: `yyyy` `yy` the year, `MMMM` `MMM` `MM` `M` the
//! month, `dddd` `ddd` `dd` `d` the weekday or the day, `HH` `H` `hh` `h` the hour (24 or 12),
//! `mm` `m` minutes, `ss` `s` seconds, `tt` `t` AM/PM; text in single quotes stays as written.

use chrono::{Datelike, NaiveDateTime, Timelike};
use serde::Deserialize;

use crate::commands::CommandRegistry;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
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

/// The short format: time and date in numbers (date first with `reverse`).
pub(crate) fn short(reverse: bool) -> String {
    let (time, date) = ("HH:mm", "yyyy-MM-dd");
    if reverse {
        format!("{date} {time}")
    } else {
        format!("{time} {date}")
    }
}

/// The long format: with the weekday and the month's name.
pub(crate) fn long(reverse: bool) -> String {
    let (time, date) = ("HH:mm", "dddd, d MMMM yyyy");
    if reverse {
        format!("{date} {time}")
    } else {
        format!("{time} {date}")
    }
}

/// The name of month `month` (1 to 12) or weekday `weekday` (0 Monday to 6 Sunday), from the
/// interface's language.
fn month_name(month: u32) -> String {
    t!(&format!("date-time.month-{month}"))
}

fn weekday_name(weekday: u32) -> String {
    t!(&format!("date-time.weekday-{weekday}"))
}

/// The first three characters of a name.
fn abbreviated(name: String) -> String {
    name.chars().take(3).collect()
}

/// `time` written in `pattern`.
pub(crate) fn format(pattern: &str, time: &NaiveDateTime) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let letter = chars[at];
        if letter == '\'' {
            // Quoted text, up to the closing quote; '' is a quote.
            let mut end = at + 1;
            while end < chars.len() && chars[end] != '\'' {
                out.push(chars[end]);
                end += 1;
            }
            if end == at + 1 && end < chars.len() {
                out.push('\'');
            }
            at = end + 1;
            continue;
        }
        let run = chars[at..].iter().take_while(|&&c| c == letter).count();
        let hour12 = match time.hour() % 12 {
            0 => 12,
            hour => hour,
        };
        let am = time.hour() < 12;
        let written = match (letter, run) {
            ('y', 1 | 2) => Some(format!("{:02}", time.year().rem_euclid(100))),
            ('y', _) => Some(format!("{:04}", time.year())),
            ('M', 1) => Some(time.month().to_string()),
            ('M', 2) => Some(format!("{:02}", time.month())),
            ('M', 3) => Some(abbreviated(month_name(time.month()))),
            ('M', _) => Some(month_name(time.month())),
            ('d', 1) => Some(time.day().to_string()),
            ('d', 2) => Some(format!("{:02}", time.day())),
            ('d', 3) => Some(abbreviated(weekday_name(
                time.weekday().num_days_from_monday(),
            ))),
            ('d', _) => Some(weekday_name(time.weekday().num_days_from_monday())),
            ('H', 1) => Some(time.hour().to_string()),
            ('H', _) => Some(format!("{:02}", time.hour())),
            ('h', 1) => Some(hour12.to_string()),
            ('h', _) => Some(format!("{hour12:02}")),
            ('m', 1) => Some(time.minute().to_string()),
            ('m', _) => Some(format!("{:02}", time.minute())),
            ('s', 1) => Some(time.second().to_string()),
            ('s', _) => Some(format!("{:02}", time.second())),
            ('t', 1) => Some(if am { "A" } else { "P" }.to_owned()),
            ('t', _) => Some(if am { "AM" } else { "PM" }.to_owned()),
            _ => None,
        };
        match written {
            Some(written) => {
                out.push_str(&written);
                at += run;
            }
            None => {
                out.push(letter);
                at += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn time(year: i32, month: u32, day: u32, h: u32, m: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(h, m, s)
            .unwrap()
    }

    #[test]
    fn notepad_plus_plus_letters() {
        crate::i18n::set_language("en");
        let at = time(2026, 3, 7, 14, 5, 9);
        assert_eq!(format("yyyy-MM-dd HH:mm:ss", &at), "2026-03-07 14:05:09");
        assert_eq!(format("yy/M/d H:m:s", &at), "26/3/7 14:5:9");
        assert_eq!(format("hh:mm tt", &at), "02:05 PM");
        assert_eq!(format("h t", &at), "2 P");
        assert_eq!(format("dddd, MMMM d", &at), "Saturday, March 7");
        assert_eq!(format("ddd MMM", &at), "Sat Mar");
        // Quoted text and other characters stay; '' is a quote.
        assert_eq!(format("'Today is' dd.MM", &at), "Today is 07.03");
        assert_eq!(format("HH'h'mm ''x''", &at), "14h05 'x'");
        assert_eq!(format("é 日 #", &at), "é 日 #");
    }

    #[test]
    fn edges_of_the_clock_and_the_calendar() {
        crate::i18n::set_language("en");
        let midnight = time(2000, 1, 1, 0, 0, 0);
        assert_eq!(format("hh tt H", &midnight), "12 AM 0");
        let noon = time(1999, 12, 31, 12, 59, 59);
        assert_eq!(format("h tt yy", &noon), "12 PM 99");
        assert_eq!(format("yyyyy", &time(7, 1, 1, 0, 0, 0)), "0007");
        assert_eq!(format("", &noon), "");
        // An unclosed quote takes the rest.
        assert_eq!(format("HH 'open", &noon), "12 open");
        assert_eq!(format("'", &noon), "");
    }

    #[test]
    fn short_and_long_in_either_order() {
        crate::i18n::set_language("en");
        let at = time(2026, 10, 12, 9, 30, 0);
        assert_eq!(format(&short(false), &at), "09:30 2026-10-12");
        assert_eq!(format(&short(true), &at), "2026-10-12 09:30");
        assert_eq!(format(&long(false), &at), "09:30 Monday, 12 October 2026");
        assert_eq!(format(&long(true), &at), "Monday, 12 October 2026 09:30");
        crate::i18n::set_language("ru");
        assert_eq!(
            format(&long(true), &at),
            "понедельник, 12 октября 2026 09:30"
        );
        crate::i18n::set_language("en");
    }
}
