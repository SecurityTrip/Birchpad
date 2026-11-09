//! What a terminal shows: rows of cells with their colors resolved through a [`Palette`], and
//! the cursor.

use alacritty_terminal::Term;
use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// A color as `0xRRGGBB`.
pub type Rgb = u32;

/// The colors of a terminal before programs set their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    /// The 16 colors of ANSI: black, red, green, yellow, blue, magenta, cyan, white, then their
    /// bright ones.
    pub ansi: [Rgb; 16],
}

impl Palette {
    /// For a light theme.
    pub const LIGHT: Self = Self {
        foreground: 0x1f2328,
        background: 0xffffff,
        cursor: 0x1f2328,
        ansi: [
            0x000000, 0xc91b00, 0x00a600, 0xa68b00, 0x0037da, 0xb000b0, 0x00a6b2, 0xbfbfbf,
            0x666666, 0xe50000, 0x00c200, 0xc4a000, 0x3b78ff, 0xd600d6, 0x00c5c7, 0xffffff,
        ],
    };

    /// For a dark theme.
    pub const DARK: Self = Self {
        foreground: 0xd4d4d4,
        background: 0x1e1e1e,
        cursor: 0xd4d4d4,
        ansi: [
            0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5,
            0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
        ],
    };

    /// Color `index` of the 256 of xterm: the 16 of ANSI, a 6×6×6 cube, 24 grays.
    pub fn indexed(&self, index: u8) -> Rgb {
        match index {
            0..=15 => self.ansi[usize::from(index)],
            16..=231 => {
                let cube = index - 16;
                let level = |value: u8| -> Rgb {
                    if value == 0 {
                        0
                    } else {
                        Rgb::from(55 + value * 40)
                    }
                };
                (level(cube / 36) << 16) | (level(cube / 6 % 6) << 8) | level(cube % 6)
            }
            232..=255 => {
                let gray = Rgb::from(8 + (index - 232) * 10);
                (gray << 16) | (gray << 8) | gray
            }
        }
    }
}

/// A cell of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub character: char,
    pub foreground: Rgb,
    pub background: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// The cell holds a character two cells wide; the next cell is part of it.
    pub wide: bool,
}

/// What the terminal shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub columns: usize,
    pub lines: usize,
    /// `lines` rows of `columns` cells each; the cell after a wide character is a space with its
    /// colors.
    pub rows: Vec<Vec<Cell>>,
    /// The cursor's line and column, when it shows (not while scrolled back, nor when the
    /// program hid it).
    pub cursor: Option<(usize, usize)>,
    /// Lines scrolled back from the bottom.
    pub scrolled: usize,
    /// The program asked for bracketed paste.
    pub bracketed_paste: bool,
    /// The program asked for application cursor keys.
    pub app_cursor: bool,
    /// A full-screen program's screen is shown.
    pub alternate: bool,
}

impl Screen {
    /// The text shown, a line per row, without the spaces at the end of rows nor the empty rows
    /// at the end.
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let line: String = row.iter().map(|cell| cell.character).collect();
                line.trim_end().to_owned()
            })
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }
}

fn rgb(color: alacritty_terminal::vte::ansi::Rgb) -> Rgb {
    (Rgb::from(color.r) << 16) | (Rgb::from(color.g) << 8) | Rgb::from(color.b)
}

/// A color the way it shows: one the program set (OSC 4, 10, 11), else the palette's.
fn resolve<T>(term: &Term<T>, color: Color, palette: &Palette) -> Rgb {
    match color {
        Color::Spec(spec) => rgb(spec),
        Color::Indexed(index) => term.colors()[usize::from(index)]
            .map(rgb)
            .unwrap_or_else(|| palette.indexed(index)),
        Color::Named(named) => {
            if let Some(set) = term.colors()[named] {
                return rgb(set);
            }
            let index = named as usize;
            match named {
                NamedColor::Foreground | NamedColor::BrightForeground => palette.foreground,
                NamedColor::DimForeground => dim(palette.foreground, palette.background),
                NamedColor::Background => palette.background,
                NamedColor::Cursor => palette.cursor,
                _ if index < 16 => palette.ansi[index],
                // The dim ones: their color, toward the background.
                _ => {
                    let base = index - NamedColor::DimBlack as usize;
                    dim(palette.ansi[base.min(7)], palette.background)
                }
            }
        }
    }
}

/// `color` two thirds of the way from `background`.
fn dim(color: Rgb, background: Rgb) -> Rgb {
    let channel = |shift: u32| {
        let (c, b) = ((color >> shift) & 0xff, (background >> shift) & 0xff);
        ((c * 2 + b) / 3) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// The screen of `term`, its colors through `palette`.
pub(crate) fn screen_of<T: EventListener>(term: &Term<T>, palette: &Palette) -> Screen {
    use alacritty_terminal::grid::Dimensions as _;
    let content = term.renderable_content();
    let (columns, lines) = (term.columns(), term.screen_lines());
    let blank = Cell {
        character: ' ',
        foreground: palette.foreground,
        background: palette.background,
        bold: false,
        italic: false,
        underline: false,
        wide: false,
    };
    let mut rows = vec![vec![blank; columns]; lines];
    let offset = content.display_offset as i32;
    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        let column = indexed.point.column.0;
        if row < 0 || row as usize >= lines || column >= columns {
            continue;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        let mut foreground = resolve(term, cell.fg, palette);
        let mut background = resolve(term, cell.bg, palette);
        if flags.contains(Flags::DIM) {
            foreground = dim(foreground, background);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut foreground, &mut background);
        }
        if flags.contains(Flags::HIDDEN) {
            foreground = background;
        }
        let spacer = flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        rows[row as usize][column] = Cell {
            character: if spacer || cell.c == '\0' {
                ' '
            } else {
                cell.c
            },
            foreground,
            background,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            wide: flags.contains(Flags::WIDE_CHAR),
        };
    }
    let mode = content.mode;
    let cursor_point = content.cursor.point;
    let cursor_row = cursor_point.line.0 + offset;
    let cursor = (mode.contains(TermMode::SHOW_CURSOR)
        && content.display_offset == 0
        && cursor_row >= 0
        && (cursor_row as usize) < lines)
        .then(|| {
            (
                cursor_row as usize,
                cursor_point.column.0.min(columns.saturating_sub(1)),
            )
        });
    Screen {
        columns,
        lines,
        rows,
        cursor,
        scrolled: content.display_offset,
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        alternate: mode.contains(TermMode::ALT_SCREEN),
    }
}
