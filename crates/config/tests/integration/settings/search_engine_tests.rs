use birchpad_config::*;

#[test]
fn engines_search_for_the_words() {
    let mut settings = SearchEngineSettings::default();
    assert_eq!(
        settings.url("rust fmt").as_deref(),
        Some("https://duckduckgo.com/?q=rust+fmt")
    );
    settings.engine = SearchEngine::Google;
    assert_eq!(
        settings.url("a&b=c/d").as_deref(),
        Some("https://www.google.com/search?q=a%26b%3Dc%2Fd")
    );
    settings.engine = SearchEngine::StackOverflow;
    assert_eq!(
        settings.url("é").as_deref(),
        Some("https://stackoverflow.com/search?q=%C3%A9")
    );
    assert_eq!(
        settings.url("").as_deref(),
        Some("https://stackoverflow.com/search?q=")
    );
}

#[test]
fn a_custom_address() {
    let mut settings = SearchEngineSettings {
        engine: SearchEngine::Custom,
        custom_url: " https://example.com/s?w=$(CURRENT_WORD)&x=1 ".into(),
    };
    assert_eq!(
        settings.url("a b").as_deref(),
        Some("https://example.com/s?w=a+b&x=1")
    );
    // Without the placeholder, the words go at the end.
    settings.custom_url = "HTTP://example.com/?q=".into();
    assert_eq!(
        settings.url("x").as_deref(),
        Some("HTTP://example.com/?q=x")
    );
    // Only web addresses.
    for bad in [
        "",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "example.com",
    ] {
        settings.custom_url = bad.into();
        assert_eq!(settings.url("x"), None, "{bad}");
    }
}
