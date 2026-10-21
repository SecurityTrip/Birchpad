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
