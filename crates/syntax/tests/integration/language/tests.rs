use birchpad_syntax::*;
use std::path::Path;

fn detected(path: &str, first_line: &str) -> Option<&'static str> {
    detect(Some(Path::new(path)), first_line).map(|language| language.id)
}

#[test]
fn languages_are_sorted_and_unique() {
    let names: Vec<&str> = LANGUAGES.iter().map(|language| language.name).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|name| name.to_lowercase());
    assert_eq!(names, sorted, "LANGUAGES must stay sorted by name");
    for (i, language) in LANGUAGES.iter().enumerate() {
        assert!(
            LANGUAGES[i + 1..]
                .iter()
                .all(|other| other.id != language.id),
            "duplicate id {}",
            language.id
        );
        for extension in language.extensions {
            assert_eq!(*extension, extension.to_ascii_lowercase());
            let owners: Vec<&str> = LANGUAGES
                .iter()
                .filter(|other| other.extensions.contains(extension))
                .map(|other| other.id)
                .collect();
            assert_eq!(owners.len(), 1, ".{extension} belongs to {owners:?}");
        }
    }
}

#[test]
fn detects_by_name_extension_and_first_line() {
    assert_eq!(detected("src/main.RS", ""), Some("rust"));
    assert_eq!(detected("include/api.h", ""), Some("cpp"));
    assert_eq!(detected("Makefile", ""), Some("makefile"));
    assert_eq!(detected("Cargo.lock", ""), Some("toml"));
    assert_eq!(detected("notes.txt", ""), None);
    assert_eq!(
        detected("run", "#!/usr/bin/env -S python3 -u"),
        Some("python")
    );
    assert_eq!(detected("run", "#!/bin/bash"), Some("bash"));
    assert_eq!(
        detected("data", "\u{feff}<?xml version=\"1.0\"?>"),
        Some("xml")
    );
    assert_eq!(detected("page", "<!DOCTYPE html>"), Some("html"));
    assert_eq!(detected("index", "<?php echo 1;"), Some("php"));
    // The name wins over the first line.
    assert_eq!(detected("build.py", "#!/bin/sh"), Some("python"));
    assert_eq!(by_id("CPP").map(|language| language.name), Some("C++"));
    assert_eq!(detected("app/build.gradle.kts", ""), Some("kotlin"));
    assert_eq!(detected("build.gradle", ""), Some("groovy"));
    assert_eq!(detected("Dockerfile", ""), Some("dockerfile"));
    assert_eq!(detected("CMakeLists.txt", ""), Some("cmake"));
    assert_eq!(detected("main.tf", ""), Some("hcl"));
    // Embedded languages are not files' languages.
    assert_eq!(by_id("markdown_inline"), None);
}

#[test]
fn embedded_blocks_name_languages_many_ways() {
    let named = |name| injected(name).map(|language| language.id);
    assert_eq!(named("rust"), Some("rust"));
    assert_eq!(named("JS"), Some("javascript"));
    assert_eq!(named("c++"), Some("cpp"));
    assert_eq!(named("C#"), Some("cs"));
    assert_eq!(named("shell"), Some("bash"));
    assert_eq!(named("{.python}"), Some("python"));
    assert_eq!(named("rust,ignore"), Some("rust"));
    assert_eq!(named("kt"), Some("kotlin"));
    assert_eq!(named("Objective-C"), Some("objc"));
    assert_eq!(named("SQL"), Some("sql"));
    assert_eq!(named("markdown_inline"), Some("markdown_inline"));
    assert_eq!(named("div"), None);
    assert_eq!(named(""), None);
}
