const std = @import("std");

/// A greeting, twice.
pub fn main() !void {
    const name = "world";
    var i: usize = 0;
    while (i < 2) : (i += 1) {
        std.debug.print("Hello, {s}! ", .{name});
    }
}
