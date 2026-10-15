const std = @import("std");
const Archive = struct { count: usize };
pub fn render(value: usize) usize {
    return value + 41;
}
test "original" { _ = render(1); }
