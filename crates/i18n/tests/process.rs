//! The language is the process's: what one thread chooses, the others show. (With the
//! `per-thread` feature, which the application's tests turn on, it is each thread's instead.)
#![cfg(not(feature = "per-thread"))]

use birchpad_i18n::{get, language, set_language, t};

#[test]
fn every_thread_shows_the_language_chosen() {
    assert_eq!(language(), "en", "English until a language is chosen");
    set_language("ru");
    let other =
        std::thread::spawn(|| (language(), get("menu.file"), t!("io.read-only", path = "a")))
            .join()
            .unwrap();
    assert_eq!(
        other,
        (
            "ru".to_owned(),
            "Файл".to_owned(),
            "a только для чтения".to_owned()
        )
    );
    // A language without a locale shows English; the code is kept as chosen.
    std::thread::spawn(|| set_language("xx")).join().unwrap();
    assert_eq!(language(), "xx");
    assert_eq!(get("menu.file"), "File");
    set_language("en");
}
