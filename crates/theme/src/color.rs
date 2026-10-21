//! Colors as theme files write them: `#rrggbb`, or `#rrggbbaa` for a translucent one.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A color with its opacity, `0xRRGGBBAA`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    /// An opaque color from `0xRRGGBB`.
    pub const fn rgb(rgb: u32) -> Self {
        Self((rgb << 8) | 0xff)
    }

    /// `0xRRGGBB`, without the opacity.
    pub const fn to_rgb(self) -> u32 {
        self.0 >> 8
    }

    pub const fn alpha(self) -> u8 {
        self.0 as u8
    }

    /// The same color with another opacity.
    pub const fn with_alpha(self, alpha: u8) -> Self {
        Self((self.0 & !0xff) | alpha as u32)
    }

    /// Whether this is a dark color, by its perceived brightness: a theme with a dark
    /// background is a dark theme.
    pub fn is_dark(self) -> bool {
        let [r, g, b, _] = self.0.to_be_bytes();
        299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) < 128_000
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.alpha() == 0xff {
            write!(f, "#{:06x}", self.to_rgb())
        } else {
            write!(f, "#{:08x}", self.0)
        }
    }
}

/// Why a color did not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", birchpad_i18n::t!("theme.not-a-color", text = .0))]
pub struct ColorError(String);

impl FromStr for Color {
    type Err = ColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ColorError(text.to_owned());
        let digits = text.strip_prefix('#').ok_or_else(error)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(error());
        }
        let value = u32::from_str_radix(digits, 16).map_err(|_| error())?;
        match digits.len() {
            6 => Ok(Self::rgb(value)),
            8 => Ok(Self(value)),
            _ => Err(error()),
        }
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
