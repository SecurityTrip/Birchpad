use birchpad_config::*;

#[test]
fn schemes_and_delimiters() {
    let links = LinkSettings::default();
    assert_eq!(
        links.extra_schemes(),
        ["svn://", "git://", "ssh://", "sftp://"]
    );
    let none = LinkSettings {
        schemes: "  ".into(),
        ..LinkSettings::default()
    };
    assert!(none.extra_schemes().is_empty());
    assert_eq!(DelimiterSettings::default().pair(), Some(('"', '"')));
    let brackets = DelimiterSettings {
        open: "(".into(),
        close: ")".into(),
        multi_line: true,
        ..DelimiterSettings::default()
    };
    assert_eq!(brackets.pair(), Some(('(', ')')));
    for (open, close) in [("", ")"), ("((", ")"), ("(", "")] {
        let wrong = DelimiterSettings {
            open: open.into(),
            close: close.into(),
            multi_line: false,
            ..DelimiterSettings::default()
        };
        assert_eq!(wrong.pair(), None, "{open:?} {close:?}");
    }
}
