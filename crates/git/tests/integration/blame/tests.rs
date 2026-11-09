use birchpad_git::Blame;

const PORCELAIN: &str = "\
3f961065654fe4ef798e8831abc55de6f2599f96 12 12 1
author Ada Lovelace
author-mail <ada@example.com>
author-time 1762631476
author-tz +0400
committer Ada Lovelace
committer-mail <ada@example.com>
committer-time 1762631476
committer-tz +0400
summary Version 0.8.0
previous 869edda0000000000000000000000000000000000 Cargo.toml
filename Cargo.toml
\tversion = \"0.8.0\"
";

#[test]
fn reads_a_lines_commit() {
    let blame = Blame::parse(PORCELAIN).unwrap();
    assert_eq!(blame.commit, "3f961065654fe4ef798e8831abc55de6f2599f96");
    assert_eq!(blame.author, "Ada Lovelace");
    assert_eq!(blame.time, 1_762_631_476);
    assert_eq!(blame.zone, "+0400");
    assert_eq!(blame.summary, "Version 0.8.0");
    assert!(!blame.is_uncommitted());
    assert_eq!(blame.short_id(), "3f961065");
    // 2025-11-08 19:51:16 UTC is 23:51 in +0400, the same day.
    assert_eq!(blame.date(), "2025-11-08");
    assert_eq!(
        blame.describe(),
        "3f961065 Ada Lovelace, 2025-11-08\nVersion 0.8.0"
    );
}

#[test]
fn a_line_not_committed_says_so() {
    let porcelain = "0000000000000000000000000000000000000000 1 1 1\n\
        author Not Committed Yet\nauthor-time 1762631476\nauthor-tz +0000\n\
        summary Version of Cargo.toml from Cargo.toml\n\tline\n";
    let blame = Blame::parse(porcelain).unwrap();
    assert!(blame.is_uncommitted());
    assert_eq!(blame.describe(), "Not committed yet");
}

#[test]
fn a_commit_seen_before_has_no_fields_again() {
    // The second line of a range: the header and the line only.
    let blame = Blame::parse("3f961065654fe4ef798e8831abc55de6f2599f96 13 13\n\tline\n").unwrap();
    assert_eq!(blame.author, "");
    assert_eq!(blame.time, 0);
    assert_eq!(blame.date(), "1970-01-01");
    // Upper-case hex is read as the id.
    let blame = Blame::parse("3F961065654FE4EF798E8831ABC55DE6F2599F96 1 1 1\n").unwrap();
    assert_eq!(blame.short_id(), "3f961065");
}

#[test]
fn output_that_is_no_blame_is_none() {
    for text in [
        "",
        "\n",
        "not a commit 1 1 1",
        // 39 and 41 hex digits.
        "3f961065654fe4ef798e8831abc55de6f2599f9 1 1 1",
        "3f961065654fe4ef798e8831abc55de6f2599f966 1 1 1",
        "3f961065654fe4ef798e8831abc55de6f2599fzz 1 1 1",
        // No line numbers, or not numbers.
        "3f961065654fe4ef798e8831abc55de6f2599f96",
        "3f961065654fe4ef798e8831abc55de6f2599f96 1",
        "3f961065654fe4ef798e8831abc55de6f2599f96 a b",
        "fatal: no such path 'x' in HEAD",
    ] {
        assert_eq!(Blame::parse(text), None, "{text:?}");
    }
}

#[test]
fn dates_in_the_authors_time_zone() {
    let at = |time: i64, zone: &str| {
        Blame {
            commit: "1".repeat(40),
            author: String::new(),
            time,
            zone: zone.into(),
            summary: String::new(),
        }
        .date()
    };
    assert_eq!(at(0, "+0000"), "1970-01-01");
    // One second before midnight UTC is the next day in +0100, the same in -0100.
    assert_eq!(at(86_399, "+0100"), "1970-01-02");
    assert_eq!(at(86_399, "-0100"), "1970-01-01");
    assert_eq!(at(-1, "+0000"), "1969-12-31");
    // A leap day, and the year 2000.
    assert_eq!(at(951_782_400, "+0000"), "2000-02-29");
    assert_eq!(at(1_709_164_800, "+0000"), "2024-02-29");
    // Zones that are not ones count as UTC.
    for zone in ["", "0400", "+04", "+04:00", "*0400", "+04a0"] {
        assert_eq!(at(86_399, zone), "1970-01-01", "{zone:?}");
    }
}

#[test]
fn times_at_the_ends_of_the_range_do_not_overflow() {
    for time in [i64::MAX, i64::MIN] {
        let blame = Blame {
            commit: "1".repeat(40),
            author: String::new(),
            time,
            zone: "+1400".into(),
            summary: String::new(),
        };
        assert!(!blame.date().is_empty());
    }
}
