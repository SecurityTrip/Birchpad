use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Holder {
    style: Style,
}

fn read(text: &str) -> Result<Style, toml::de::Error> {
    toml::from_str::<Holder>(text).map(|holder| holder.style)
}

fn write(style: Style) -> String {
    toml::to_string(&Holder { style }).unwrap()
}

#[test]
fn a_color_alone_or_a_table() {
    assert_eq!(
        read("style = '#0000ff'").unwrap(),
        Style::color(Color::rgb(0xff))
    );
    let full = read(
            "style = { color = '#ff0000', background = '#ffffc080', bold = true, italic = true, underline = true }",
        )
        .unwrap();
    assert_eq!(
        full,
        Style {
            color: Some(Color::rgb(0xff0000)),
            background: Some(Color(0xffffc080)),
            bold: true,
            italic: true,
            underline: true,
        }
    );
    // Every key is optional, and keys of a newer version are skipped.
    assert_eq!(read("style = {}").unwrap(), Style::default());
    assert_eq!(
        read("style = { italic = true, glow = 3 }").unwrap(),
        Style {
            italic: true,
            ..Style::default()
        }
    );
}

#[test]
fn refuses_wrong_values() {
    for text in [
        "style = 'blue'",
        "style = 3",
        "style = true",
        "style = ['#0000ff']",
        "style = { color = 'blue' }",
        "style = { bold = 'yes' }",
        "style = { color = 255 }",
    ] {
        assert!(read(text).is_err(), "{text}");
    }
}

#[test]
fn writes_the_shortest_form_and_reads_it_back() {
    let plain = Style::color(Color::rgb(0x008000));
    assert_eq!(write(plain), "style = \"#008000\"\n");
    for style in [
        plain,
        Style::default(),
        Style {
            bold: true,
            ..plain
        },
        Style {
            color: None,
            background: Some(Color(0x00ff0040)),
            underline: true,
            ..Style::default()
        },
    ] {
        assert_eq!(read(&write(style)).unwrap(), style, "{}", write(style));
    }
}
