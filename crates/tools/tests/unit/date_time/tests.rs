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
    birchpad_i18n::set_language("en");
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
    birchpad_i18n::set_language("en");
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
    birchpad_i18n::set_language("en");
    let at = time(2026, 10, 12, 9, 30, 0);
    assert_eq!(format(&short(false), &at), "09:30 2026-10-12");
    assert_eq!(format(&short(true), &at), "2026-10-12 09:30");
    assert_eq!(format(&long(false), &at), "09:30 Monday, 12 October 2026");
    assert_eq!(format(&long(true), &at), "Monday, 12 October 2026 09:30");
    birchpad_i18n::set_language("ru");
    assert_eq!(
        format(&long(true), &at),
        "понедельник, 12 октября 2026 09:30"
    );
    birchpad_i18n::set_language("en");
}
