//! How a kind of text looks. A theme file writes a style as a color alone (`"#0000ff"`) or as a
//! table: `{ color = "#0000ff", background = "#ffffc0", bold = true, italic = true,
//! underline = true }`, every key optional.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Color;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// The text color; unset is the theme's text color.
    pub color: Option<Color>,
    /// A background behind the text; unset is none.
    pub background: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    pub const fn color(color: Color) -> Self {
        Self {
            color: Some(color),
            background: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    /// Just a color, which files write without a table.
    fn is_plain_color(&self) -> bool {
        self.color.is_some()
            && self.background.is_none()
            && !self.bold
            && !self.italic
            && !self.underline
    }
}

impl Serialize for Style {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let (true, Some(color)) = (self.is_plain_color(), self.color) {
            return color.serialize(serializer);
        }
        let mut map = serializer.serialize_map(None)?;
        if let Some(color) = &self.color {
            map.serialize_entry("color", color)?;
        }
        if let Some(background) = &self.background {
            map.serialize_entry("background", background)?;
        }
        for (key, set) in [
            ("bold", self.bold),
            ("italic", self.italic),
            ("underline", self.underline),
        ] {
            if set {
                map.serialize_entry(key, &true)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Style {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StyleVisitor)
    }
}

struct StyleVisitor;

impl<'de> Visitor<'de> for StyleVisitor {
    type Value = Style;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(
            "a color (\"#rrggbb\") or a table with color, background, bold, italic, underline",
        )
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Style, E> {
        text.parse().map(Style::color).map_err(E::custom)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Style, A::Error> {
        let mut style = Style::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "color" => style.color = Some(map.next_value()?),
                "background" => style.background = Some(map.next_value()?),
                "bold" => style.bold = map.next_value()?,
                "italic" => style.italic = map.next_value()?,
                "underline" => style.underline = map.next_value()?,
                // Keys of a newer version.
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(style)
    }
}

#[cfg(test)]
mod tests {
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
}
