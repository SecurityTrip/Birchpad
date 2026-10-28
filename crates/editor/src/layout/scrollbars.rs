//! The scrollbars' tracks and thumbs.

use gpui_kit::{Bounds, Pixels, point, px, size};

use super::{Layout, SCROLLBAR, Scrollbar};

pub(super) fn vertical_scrollbar(
    bounds: Bounds<Pixels>,
    layout: &Layout,
    scroll_top: f64,
) -> Option<Scrollbar> {
    let track = Bounds::new(
        point(bounds.right() - SCROLLBAR, bounds.top()),
        size(SCROLLBAR, layout.text_bounds.size.height),
    );
    let total = layout.total_rows.max(1) as f32;
    let visible = (layout.full_rows as f32).min(total);
    let height = (track.size.height * (visible / total))
        .max(px(24.))
        .min(track.size.height);
    let max_top = (layout.total_rows.saturating_sub(layout.full_rows)) as f32;
    let fraction = if max_top > 0. {
        (scroll_top as f32 / max_top).clamp(0., 1.)
    } else {
        0.
    };
    let top = track.top() + (track.size.height - height) * fraction;
    Some(Scrollbar {
        track,
        thumb: Bounds::new(
            point(track.left() + px(2.), top),
            size(SCROLLBAR - px(4.), height),
        ),
        markers: Vec::new(),
    })
}

pub(super) fn horizontal_scrollbar(
    layout: &Layout,
    scroll_left: Pixels,
    scroll_width: Pixels,
) -> Scrollbar {
    let text = layout.text_bounds;
    let track = Bounds::new(
        point(text.left(), text.bottom()),
        size(text.size.width, SCROLLBAR),
    );
    let width = (track.size.width * (text.size.width / scroll_width).min(1.))
        .max(px(24.))
        .min(track.size.width);
    let max_left = (scroll_width - text.size.width).max(px(1.));
    let fraction = (scroll_left / max_left).clamp(0., 1.);
    let left = track.left() + (track.size.width - width) * fraction;
    Scrollbar {
        track,
        thumb: Bounds::new(
            point(left, track.top() + px(2.)),
            size(width, SCROLLBAR - px(4.)),
        ),
        markers: Vec::new(),
    }
}
