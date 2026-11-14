//! Where the floating find panel goes when its title is dragged.

use birchpad_find_bar::FloatingPlace;
use gpui_kit::{Pixels, Point, Size, point, px, size};

const AREA: Size<Pixels> = Size {
    width: px(1000.),
    height: px(600.),
};

fn place(right: f32, top: f32) -> FloatingPlace {
    FloatingPlace {
        right: px(right),
        top: px(top),
    }
}

fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[test]
fn the_panel_opens_in_the_top_right_corner() {
    assert_eq!(FloatingPlace::default(), place(16., 8.));
}

#[test]
fn the_panel_follows_the_mouse() {
    // To the left and down: further from the right edge, further from the top.
    assert_eq!(
        place(16., 8.).moved(at(900., 20.), at(700., 120.), AREA),
        place(216., 108.)
    );
    // Back to the right and up.
    assert_eq!(
        place(216., 108.).moved(at(700., 120.), at(750., 100.), AREA),
        place(166., 88.)
    );
    // Not moved: where it was.
    assert_eq!(
        place(40., 30.).moved(at(500., 50.), at(500., 50.), AREA),
        place(40., 30.)
    );
}

#[test]
fn the_panel_stays_in_the_documents_area() {
    // Past the right edge and over the top: in the corner.
    assert_eq!(
        place(16., 8.).moved(at(900., 20.), at(2000., -300.), AREA),
        place(0., 0.)
    );
    // Past the left edge and the bottom: enough of its title stays to take it back.
    let kept = FloatingPlace::KEPT;
    assert_eq!(
        place(16., 8.).moved(at(900., 20.), at(-5000., 5000.), AREA),
        FloatingPlace {
            right: AREA.width - kept.width,
            top: AREA.height - kept.height,
        }
    );
}

#[test]
fn the_panel_can_be_put_right_at_the_edges() {
    let kept = FloatingPlace::KEPT;
    let (far_right, far_top) = (
        f32::from(AREA.width - kept.width),
        f32::from(AREA.height - kept.height),
    );
    // Exactly at the limits it is where it was put, one pixel past them it is not.
    assert_eq!(
        place(0., 0.).moved(at(0., 0.), at(-far_right, far_top), AREA),
        place(far_right, far_top)
    );
    assert_eq!(
        place(0., 0.).moved(at(0., 0.), at(-far_right - 1., far_top + 1.), AREA),
        place(far_right, far_top)
    );
    assert_eq!(
        place(1., 1.).moved(at(0., 0.), at(1., -1.), AREA),
        place(0., 0.)
    );
}

#[test]
fn a_window_smaller_than_what_stays_keeps_the_panel_in_its_corner() {
    let tiny = size(px(50.), px(10.));
    assert_eq!(
        place(16., 8.).moved(at(0., 0.), at(-300., 300.), tiny),
        place(0., 0.)
    );
    let none = size(px(0.), px(0.));
    assert_eq!(
        place(0., 0.).moved(at(0., 0.), at(10., 10.), none),
        place(0., 0.)
    );
}
