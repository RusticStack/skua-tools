//! Display width of a byte string (UTF-8; invalid bytes count 1): combining and control
//! characters take no room, East Asian wide characters take two cells.

const ZERO: &[(u32, u32)] = &[
    (0x0300, 0x036f),
    (0x0483, 0x0489),
    (0x0591, 0x05bd),
    (0x05bf, 0x05bf),
    (0x05c1, 0x05c2),
    (0x05c4, 0x05c5),
    (0x05c7, 0x05c7),
    (0x0610, 0x061a),
    (0x064b, 0x065f),
    (0x0670, 0x0670),
    (0x06d6, 0x06dc),
    (0x06df, 0x06e4),
    (0x06e7, 0x06e8),
    (0x06ea, 0x06ed),
    (0x0e31, 0x0e31),
    (0x0e34, 0x0e3a),
    (0x0e47, 0x0e4e),
    (0x1ab0, 0x1aff),
    (0x1dc0, 0x1dff),
    (0x200b, 0x200f),
    (0x202a, 0x202e),
    (0x2060, 0x2064),
    (0x20d0, 0x20ff),
    (0xfe00, 0xfe0f),
    (0xfe20, 0xfe2f),
    (0xfeff, 0xfeff),
    (0xe0100, 0xe01ef),
];

const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115f),
    (0x2e80, 0x303e),
    (0x3041, 0x33ff),
    (0x3400, 0x4dbf),
    (0x4e00, 0x9fff),
    (0xa000, 0xa4cf),
    (0xac00, 0xd7a3),
    (0xf900, 0xfaff),
    (0xfe30, 0xfe6f),
    (0xff00, 0xff60),
    (0xffe0, 0xffe6),
    (0x1f300, 0x1f64f),
    (0x1f900, 0x1f9ff),
    (0x20000, 0x3fffd),
];

fn in_table(t: &[(u32, u32)], c: u32) -> bool {
    t.iter().any(|&(lo, hi)| c >= lo && c <= hi)
}

pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || (0x7f..0xa0).contains(&u) || in_table(ZERO, u) {
        0
    } else if in_table(WIDE, u) {
        2
    } else {
        1
    }
}

/// Width in terminal cells of `s`.
pub fn width(s: &[u8]) -> usize {
    let mut w = 0;
    let mut rest = s;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(t) => {
                w += t.chars().map(char_width).sum::<usize>();
                break;
            }
            Err(e) => {
                let (good, bad) = rest.split_at(e.valid_up_to());
                // `good` is valid UTF-8 by construction.
                w += String::from_utf8_lossy(good)
                    .chars()
                    .map(char_width)
                    .sum::<usize>();
                let skip = e.error_len().unwrap_or(bad.len()).max(1);
                w += skip;
                rest = &bad[skip.min(bad.len())..];
            }
        }
    }
    w
}
