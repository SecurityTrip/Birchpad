//! The date and time formats of Edit > Insert > Date Time, as Notepad++'s.
//!
//! Formats use Notepad++'s (Windows') letters: `yyyy` `yy` the year, `MMMM` `MMM` `MM` `M` the
//! month, `dddd` `ddd` `dd` `d` the weekday or the day, `HH` `H` `hh` `h` the hour (24 or 12),
//! `mm` `m` minutes, `ss` `s` seconds, `tt` `t` AM/PM; text in single quotes stays as written.
//! The names of months and weekdays are in the interface's language.

use birchpad_i18n::t;
use chrono::{Datelike, NaiveDateTime, Timelike};

/// The short format: time and date in numbers (date first with `reverse`).
pub fn short(reverse: bool) -> String {
    let (time, date) = ("HH:mm", "yyyy-MM-dd");
    if reverse {
        format!("{date} {time}")
    } else {
        format!("{time} {date}")
    }
}

/// The long format: with the weekday and the month's name.
pub fn long(reverse: bool) -> String {
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
    t!(&format!("tools.date-time.month-{month}"))
}

fn weekday_name(weekday: u32) -> String {
    t!(&format!("tools.date-time.weekday-{weekday}"))
}

/// The first three characters of a name.
fn abbreviated(name: String) -> String {
    name.chars().take(3).collect()
}

/// `time` written in `pattern`.
pub fn format(pattern: &str, time: &NaiveDateTime) -> String {
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
