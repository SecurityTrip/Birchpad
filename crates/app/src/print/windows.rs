//! Printing through Windows' own printing (GDI): its print dialog chooses the printer, the
//! pages and the copies, or Print Now takes the default printer; the pages are drawn as the
//! print preview shows them, at the same places on the paper.

use std::ptr::null_mut;

use birchpad_theme::{Color, Style};
use birchpad_tools::print::{PageSetup, Pages};
use windows_sys::Win32::Foundation::{COLORREF, GlobalFree, HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_QUALITY, DeleteDC,
    DeleteObject, FF_MODERN, FIXED_PITCH, FW_BOLD, FW_NORMAL, FillRect, GetDeviceCaps, HDC, HFONT,
    LOGPIXELSX, LOGPIXELSY, OPAQUE, OUT_DEFAULT_PRECIS, PHYSICALOFFSETX, PHYSICALOFFSETY,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, PD_NOSELECTION, PD_PAGENUMS, PD_RETURNDC, PD_RETURNDEFAULT,
    PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW, PrintDlgW,
};

/// What a printing ended with.
#[derive(Debug, PartialEq, Eq)]
pub enum Printed {
    /// The pages went to the printer.
    Done,
    /// The user closed the print dialog without printing.
    Cancelled,
}

/// A printer's drawing context, deleted when dropped.
struct Printer(HDC);

impl Drop for Printer {
    fn drop(&mut self) {
        // SAFETY: the context came from PrintDlgW for this printing and is deleted once, here.
        #[allow(unsafe_code)]
        unsafe {
            DeleteDC(self.0);
        }
    }
}

/// A printer chosen, and the pages chosen (from 1) if the user chose a range.
type Chosen = (Printer, Option<(usize, usize)>);

/// Asks for a printer (or takes the default one without asking); `None` if the user did not
/// choose one.
fn choose(owner: HWND, ask: bool, pages: usize) -> anyhow::Result<Option<Chosen>> {
    let last = u16::try_from(pages).unwrap_or(u16::MAX);
    let mut dialog = PRINTDLGW {
        lStructSize: u32::try_from(size_of::<PRINTDLGW>()).expect("a small structure"),
        hwndOwner: owner,
        Flags: PD_RETURNDC
            | PD_USEDEVMODECOPIESANDCOLLATE
            | PD_NOSELECTION
            | if ask { 0 } else { PD_RETURNDEFAULT },
        nFromPage: 1,
        nToPage: last,
        nMinPage: 1,
        nMaxPage: last,
        nCopies: 1,
        ..PRINTDLGW::default()
    };
    // SAFETY: `dialog` is a PRINTDLGW of its own size whose pointers are null or owned by this
    // function; PrintDlgW fills in the handles, which are freed below.
    #[allow(unsafe_code)]
    let chosen = unsafe { PrintDlgW(&mut dialog) } != 0;
    // SAFETY: the handles are PrintDlgW's global memory (or null, which GlobalFree ignores),
    // freed once; the context is kept.
    #[allow(unsafe_code)]
    unsafe {
        GlobalFree(dialog.hDevMode);
        GlobalFree(dialog.hDevNames);
    }
    if !chosen {
        // SAFETY: no arguments; it reports the last common dialog's error.
        #[allow(unsafe_code)]
        let error = unsafe { CommDlgExtendedError() };
        if error == 0 {
            return Ok(None);
        }
        anyhow::bail!("the print dialog failed (error {error:#x})");
    }
    if dialog.hDC.is_null() {
        anyhow::bail!("there is no printer");
    }
    let range = (dialog.Flags & PD_PAGENUMS != 0)
        .then(|| (usize::from(dialog.nFromPage), usize::from(dialog.nToPage)));
    Ok(Some((Printer(dialog.hDC), range)))
}

/// Windows' color of `color` (`0x00BBGGRR`).
fn colorref(color: Color) -> COLORREF {
    let [r, g, b, _] = color.0.to_be_bytes();
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

/// UTF-16 with a terminating zero.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// The device's scale and origin: dots per millimeter, and where the paper's corner is from the
/// printable area's.
struct Device {
    dots_x: f32,
    dots_y: f32,
    offset_x: f32,
    offset_y: f32,
}

impl Device {
    fn of(dc: HDC) -> Self {
        // SAFETY: `dc` is a live printer context; GetDeviceCaps only reads its capabilities.
        #[allow(unsafe_code)]
        let caps = |index| unsafe { GetDeviceCaps(dc, index as i32) } as f32;
        Self {
            dots_x: caps(LOGPIXELSX) / 25.4,
            dots_y: caps(LOGPIXELSY) / 25.4,
            offset_x: caps(PHYSICALOFFSETX),
            offset_y: caps(PHYSICALOFFSETY),
        }
    }

    /// A point `x` and `y` millimeters from the paper's top left corner, in the device's dots.
    fn at(&self, x: f32, y: f32) -> (i32, i32) {
        (
            (x * self.dots_x - self.offset_x).round() as i32,
            (y * self.dots_y - self.offset_y).round() as i32,
        )
    }
}

/// The fonts of the text: regular, bold, italic and bold italic, underlined or not.
struct Fonts {
    fonts: Vec<HFONT>,
}

impl Fonts {
    fn new(setup: &PageSetup, device: &Device) -> Self {
        let height =
            -(setup.font_size * birchpad_tools::print::MM_PER_POINT * device.dots_y).round() as i32;
        let face = wide("Consolas");
        let fonts = (0..8u32)
            .map(|index| {
                let weight = if index & 1 != 0 { FW_BOLD } else { FW_NORMAL };
                // SAFETY: plain values and a zero-terminated face name that outlives the call;
                // the font is deleted when `Fonts` drops.
                #[allow(unsafe_code)]
                unsafe {
                    CreateFontW(
                        height,
                        0,
                        0,
                        0,
                        weight as i32,
                        (index >> 1) & 1,
                        (index >> 2) & 1,
                        0,
                        u32::from(DEFAULT_CHARSET),
                        u32::from(OUT_DEFAULT_PRECIS),
                        u32::from(CLIP_DEFAULT_PRECIS),
                        u32::from(DEFAULT_QUALITY),
                        u32::from(FIXED_PITCH | FF_MODERN),
                        face.as_ptr(),
                    )
                }
            })
            .collect();
        Self { fonts }
    }

    fn of(&self, style: Style) -> HFONT {
        let index = usize::from(style.bold)
            | (usize::from(style.italic) << 1)
            | (usize::from(style.underline) << 2);
        self.fonts[index]
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for font in &self.fonts {
            // SAFETY: each font was made by CreateFontW for this printing and is no longer
            // selected (the context is deleted or holds its default font by then).
            #[allow(unsafe_code)]
            unsafe {
                DeleteObject(*font);
            }
        }
    }
}

/// Draws `text` at `x`, `y` (dots) in `style`.
fn text_out(dc: HDC, fonts: &Fonts, x: i32, y: i32, text: &str, style: Style, paper: Color) {
    let text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: `dc` is a live printer context between StartPage and EndPage; the font is one of
    // `fonts`, alive during the call; `text` is a buffer of the length given.
    #[allow(unsafe_code)]
    unsafe {
        SelectObject(dc, fonts.of(style));
        SetTextColor(dc, colorref(style.color.unwrap_or(Color::rgb(0))));
        match style.background {
            Some(background) if background != paper => {
                SetBkMode(dc, OPAQUE as i32);
                SetBkColor(dc, colorref(background));
            }
            _ => {
                SetBkMode(dc, TRANSPARENT as i32);
            }
        }
        TextOutW(
            dc,
            x,
            y,
            text.as_ptr(),
            i32::try_from(text.len()).unwrap_or(i32::MAX),
        );
    }
}

/// Prints `pages` titled `title` as `setup` lays them out, on the printer the user chooses in
/// Windows' print dialog (owned by `owner`), or on the default printer without asking.
pub fn print(
    title: &str,
    pages: &Pages,
    setup: &PageSetup,
    owner: isize,
    ask: bool,
) -> anyhow::Result<Printed> {
    let Some((printer, range)) = choose(owner as HWND, ask, pages.pages.len())? else {
        return Ok(Printed::Cancelled);
    };
    let dc = printer.0;
    let device = Device::of(dc);
    let fonts = Fonts::new(setup, &device);
    let (width, height) = setup.page_size();
    let (cell, row) = setup.cell();
    let margins = setup.margins;
    let name = wide(title);
    let document = DOCINFOW {
        cbSize: i32::try_from(size_of::<DOCINFOW>()).expect("a small structure"),
        lpszDocName: name.as_ptr(),
        lpszOutput: null_mut(),
        lpszDatatype: null_mut(),
        fwType: 0,
    };
    // SAFETY: `dc` is the printer context chosen above; `document` and the name it points to
    // live until EndDoc.
    #[allow(unsafe_code)]
    if unsafe { StartDocW(dc, &document) } <= 0 {
        anyhow::bail!("{}", std::io::Error::last_os_error());
    }
    let plain = Style::default();
    let text_style = |color| Style {
        color: Some(color),
        ..plain
    };
    for page in &pages.pages {
        if range.is_some_and(|(from, to)| page.number < from || page.number > to) {
            continue;
        }
        // SAFETY: between StartDocW and EndDoc of a live context.
        #[allow(unsafe_code)]
        unsafe {
            StartPage(dc);
        }
        if pages.background != Color::rgb(0xffffff) {
            let (left, top) = device.at(0.0, 0.0);
            let (right, bottom) = device.at(width, height);
            let whole = RECT {
                left,
                top,
                right,
                bottom,
            };
            // SAFETY: a brush made, used and deleted here, on the live page.
            #[allow(unsafe_code)]
            unsafe {
                let brush = CreateSolidBrush(colorref(pages.background));
                FillRect(dc, &whole, brush);
                DeleteObject(brush);
            }
        }
        let parts = |parts: &[String; 3], y: f32| {
            let span = width - margins.left - margins.right;
            for (index, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }
                let length = part.chars().count() as f32 * cell;
                let x = match index {
                    0 => margins.left,
                    1 => margins.left + (span - length) / 2.0,
                    _ => margins.left + span - length,
                };
                let (x, y) = device.at(x.max(margins.left), y);
                text_out(
                    dc,
                    &fonts,
                    x,
                    y,
                    part,
                    text_style(pages.text),
                    pages.background,
                );
            }
        };
        let mut y = margins.top;
        if page.header.iter().any(|part| !part.is_empty()) {
            parts(&page.header, y);
            y += 2.0 * row;
        }
        for line in &page.rows {
            if let Some(number) = line.number.filter(|_| pages.number_width > 0) {
                let text = format!("{number:>width$}", width = pages.number_width - 1);
                let (x, top) = device.at(margins.left, y);
                text_out(
                    dc,
                    &fonts,
                    x,
                    top,
                    &text,
                    text_style(pages.numbers),
                    pages.background,
                );
            }
            for piece in &line.pieces {
                let x = margins.left + (pages.number_width + piece.column) as f32 * cell;
                let (x, top) = device.at(x, y);
                text_out(
                    dc,
                    &fonts,
                    x,
                    top,
                    &piece.text,
                    piece.style,
                    pages.background,
                );
            }
            y += row;
        }
        if page.footer.iter().any(|part| !part.is_empty()) {
            parts(&page.footer, height - margins.bottom - row);
        }
        // SAFETY: the page started above.
        #[allow(unsafe_code)]
        unsafe {
            EndPage(dc);
        }
    }
    // SAFETY: the document started above.
    #[allow(unsafe_code)]
    unsafe {
        EndDoc(dc);
    }
    // The context goes first, so that no font is selected in it when the fonts go.
    drop(printer);
    drop(fonts);
    Ok(Printed::Done)
}
