use birchpad_print_dialog::{Device, colorref, wide};
use birchpad_theme::Color;

#[test]
fn colors_are_windows_blue_green_red() {
    assert_eq!(colorref(Color(0xFF00_00FF)), 0x0000_00FF, "red");
    assert_eq!(colorref(Color(0x00FF_00FF)), 0x0000_FF00, "green");
    assert_eq!(colorref(Color(0x0000_FFFF)), 0x00FF_0000, "blue");
    assert_eq!(colorref(Color(0x1234_56FF)), 0x0056_3412);
}

#[test]
fn a_color_s_alpha_is_left_out() {
    assert_eq!(colorref(Color(0x1234_5600)), colorref(Color(0x1234_56FF)));
    assert_eq!(colorref(Color(0x0000_0000)), 0, "black");
    assert_eq!(colorref(Color(0xFFFF_FFFF)), 0x00FF_FFFF, "white");
}

#[test]
fn text_for_windows_ends_with_a_zero() {
    assert_eq!(wide(""), [0]);
    assert_eq!(wide("A"), [0x41, 0]);
    // Outside the basic plane: two units.
    assert_eq!(wide("Ж😀"), [0x0416, 0xD83D, 0xDE00, 0]);
}

#[test]
fn millimeters_become_the_printer_s_dots_from_its_printable_area() {
    // 600 dots per inch, the printable area 50 and 60 dots in from the paper's corner.
    let device = Device {
        dots_x: 600. / 25.4,
        dots_y: 600. / 25.4,
        offset_x: 50.,
        offset_y: 60.,
    };
    assert_eq!(device.at(25.4, 25.4), (550, 540));
    // The paper's corner is outside the printable area.
    assert_eq!(device.at(0., 0.), (-50, -60));
    // A printer without margins, at 300 by 150 dots per inch.
    let device = Device {
        dots_x: 300. / 25.4,
        dots_y: 150. / 25.4,
        offset_x: 0.,
        offset_y: 0.,
    };
    assert_eq!(device.at(254., 254.), (3000, 1500));
}
