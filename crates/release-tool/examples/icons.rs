//! Renders Birchpad's icon into the files the packages use: `packaging/icons/birchpad.svg`, and
//! `birchpad-small.svg` for 32 pixels and less, where fine detail turns to blur. Run
//! `cargo run -p birchpad-release-tool --example icons` from the repository root.
//!
//! - `birchpad.ico`: the Windows executable, installers and shortcuts (16 to 256 pixels);
//! - `Birchpad.icns`: the macOS app bundle (16 to 1024 pixels);
//! - `birchpad.png`: Linux, 512 pixels.
//!
//! Both containers hold PNG images, which Windows Vista and later and macOS 10.7 and later read.

#![allow(
    clippy::print_stdout,
    reason = "a command-line tool prints what it wrote"
)]

use std::fs;
use std::path::Path;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

const DIR: &str = "packaging/icons";

fn main() {
    let load = |name: &str| {
        let svg = fs::read_to_string(Path::new(DIR).join(name))
            .expect("run this from the repository root");
        Tree::from_str(&svg, &Options::default()).expect("a valid SVG")
    };
    let (large, small) = (load("birchpad.svg"), load("birchpad-small.svg"));
    let png = |size: u32| -> Vec<u8> {
        let tree = if size <= 32 { &small } else { &large };
        let mut pixmap = Pixmap::new(size, size).expect("a positive size");
        let scale = size as f32 / tree.size().width();
        resvg::render(
            tree,
            Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap.encode_png().expect("PNG encoding")
    };

    let ico = ico(&[16, 20, 24, 32, 40, 48, 64, 128, 256].map(|size| (size, png(size))));
    write("birchpad.ico", &ico);

    let icns = icns(&[
        (*b"icp4", png(16)),
        (*b"icp5", png(32)),
        (*b"icp6", png(64)),
        (*b"ic07", png(128)),
        (*b"ic08", png(256)),
        (*b"ic09", png(512)),
        (*b"ic10", png(1024)),
        (*b"ic11", png(32)),
        (*b"ic12", png(64)),
        (*b"ic13", png(256)),
        (*b"ic14", png(512)),
    ]);
    write("Birchpad.icns", &icns);

    write("birchpad.png", &png(512));
}

fn write(name: &str, bytes: &[u8]) {
    let path = Path::new(DIR).join(name);
    fs::write(&path, bytes).expect("cannot write the icon");
    println!("{} ({} KB)", path.display(), bytes.len() / 1024);
}

/// A Windows icon of PNG images: a header, a directory entry per image, then the images.
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = u16::try_from(images.len()).expect("a few images");
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // An icon, not a cursor.
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for (size, png) in images {
        // 256 is written as 0.
        let side = u8::try_from(*size).unwrap_or(0);
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // Planes.
        out.extend_from_slice(&32u16.to_le_bytes()); // Bits per pixel.
        let length = u32::try_from(png.len()).expect("a small image");
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += length;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

/// An Apple icon of PNG images: `icns` and the total length, then a typed, sized entry per image.
fn icns(images: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (kind, png) in images {
        body.extend_from_slice(kind);
        let length = u32::try_from(png.len() + 8).expect("a small image");
        body.extend_from_slice(&length.to_be_bytes());
        body.extend_from_slice(png);
    }
    let mut out = b"icns".to_vec();
    let total = u32::try_from(body.len() + 8).expect("a small icon");
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(&body);
    out
}
