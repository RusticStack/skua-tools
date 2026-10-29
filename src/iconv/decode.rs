//! Decoding: bytes to Unicode scalar values with the position of each, as libiconv reports.

use super::charset::{ISO9, Kind, rs_encoding, undefined_c1};

/// What the decoder found at one place in the input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dec {
    Ch(u32),
    /// Not a valid sequence; `len` bytes starting at `start` are skipped.
    Bad,
    /// The input ends inside a character.
    Incomplete,
}

pub struct Item {
    pub dec: Dec,
    pub start: usize,
    pub len: usize,
}

fn item(dec: Dec, start: usize, len: usize) -> Item {
    Item { dec, start, len }
}

/// Receives the decoded items; `stop` is set when the consumer wants no more.
pub struct Out<'a> {
    f: &'a mut dyn FnMut(Item) -> bool,
    stop: bool,
}

impl Out<'_> {
    fn push(&mut self, it: Item) {
        if !self.stop && !(self.f)(it) {
            self.stop = true;
        }
    }
}

/// Decodes `data`, handing each item to `f` until it returns false.
pub fn decode(kind: Kind, data: &[u8], f: &mut dyn FnMut(Item) -> bool) {
    let mut v = Out { f, stop: false };
    match kind {
        Kind::Ascii => {
            for (i, &b) in data.iter().enumerate() {
                v.push(item(
                    if b < 0x80 {
                        Dec::Ch(u32::from(b))
                    } else {
                        Dec::Bad
                    },
                    i,
                    1,
                ));
                if v.stop {
                    break;
                }
            }
        }
        Kind::Latin1 | Kind::Iso9 => {
            for (i, &b) in data.iter().enumerate() {
                let cp = if kind == Kind::Iso9 {
                    iso9_to_char(b)
                } else {
                    u32::from(b)
                };
                v.push(item(Dec::Ch(cp), i, 1));
                if v.stop {
                    break;
                }
            }
        }
        Kind::Utf8 => decode_utf8(data, &mut v),
        Kind::Utf16 {
            le, bom_in, pairs, ..
        } => decode_utf16(data, le, bom_in, pairs, &mut v),
        Kind::Utf32 { le, bom_in, .. } => decode_utf32(data, le, bom_in, &mut v),
        Kind::Rs(key) => decode_rs(key, data, &mut v),
    }
}

fn iso9_to_char(b: u8) -> u32 {
    ISO9.iter()
        .find(|(x, _)| *x == b)
        .map_or(u32::from(b), |(_, c)| *c)
}

fn decode_utf8(data: &[u8], v: &mut Out) {
    let mut pos = 0;
    while pos < data.len() && !v.stop {
        let (good, err) = match std::str::from_utf8(&data[pos..]) {
            Ok(s) => (s, None),
            Err(e) => {
                let ok = e.valid_up_to();
                // `valid_up_to` bytes are valid UTF-8 by construction.
                (
                    std::str::from_utf8(&data[pos..pos + ok]).unwrap_or(""),
                    Some(e),
                )
            }
        };
        for (i, c) in good.char_indices() {
            v.push(item(Dec::Ch(c as u32), pos + i, c.len_utf8()));
        }
        pos += good.len();
        match err {
            None => break,
            Some(e) => match e.error_len() {
                Some(n) => {
                    v.push(item(Dec::Bad, pos, n));
                    pos += n;
                }
                None => {
                    v.push(item(Dec::Incomplete, pos, data.len() - pos));
                    break;
                }
            },
        }
    }
}

fn decode_utf16(data: &[u8], mut le: bool, bom_in: bool, pairs: bool, v: &mut Out) {
    let mut pos = 0;
    let unit = |p: usize, le: bool| -> u32 {
        let (a, b) = (u32::from(data[p]), u32::from(data[p + 1]));
        if le { (b << 8) | a } else { (a << 8) | b }
    };
    while pos < data.len() && !v.stop {
        if pos + 2 > data.len() {
            v.push(item(Dec::Incomplete, pos, data.len() - pos));
            break;
        }
        let u = unit(pos, le);
        if bom_in && (u == 0xfeff || u == 0xfffe) {
            // A byte order mark anywhere is consumed; a swapped one flips the byte order.
            if u == 0xfffe {
                le = !le;
            }
            pos += 2;
            continue;
        }
        if (0xd800..0xdc00).contains(&u) && pairs {
            if pos + 4 > data.len() {
                v.push(item(Dec::Incomplete, pos, data.len() - pos));
                break;
            }
            let u2 = unit(pos + 2, le);
            if (0xdc00..0xe000).contains(&u2) {
                let cp = 0x10000 + ((u - 0xd800) << 10) + (u2 - 0xdc00);
                v.push(item(Dec::Ch(cp), pos, 4));
                pos += 4;
            } else {
                v.push(item(Dec::Bad, pos, 2));
                pos += 2;
            }
        } else if (0xd800..0xe000).contains(&u) {
            v.push(item(Dec::Bad, pos, 2));
            pos += 2;
        } else {
            v.push(item(Dec::Ch(u), pos, 2));
            pos += 2;
        }
    }
}

fn decode_utf32(data: &[u8], mut le: bool, bom_in: bool, v: &mut Out) {
    let mut pos = 0;
    while pos < data.len() && !v.stop {
        if pos + 4 > data.len() {
            v.push(item(Dec::Incomplete, pos, data.len() - pos));
            break;
        }
        let b = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        let u = if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        };
        if bom_in && (u == 0xfeff || u == 0xfffe_0000) {
            if u == 0xfffe_0000 {
                le = !le;
            }
            pos += 4;
            continue;
        }
        let ok = u <= 0x10ffff && !(0xd800..0xe000).contains(&u);
        v.push(item(if ok { Dec::Ch(u) } else { Dec::Bad }, pos, 4));
        pos += 4;
    }
}

/// Table-driven sets via `encoding_rs`. Undefined input shows up as U+FFFD (or, for Windows
/// code pages, a C1 control, which libiconv leaves undefined); positions are only known for
/// single-byte sets.
fn decode_rs(key: &str, data: &[u8], v: &mut Out) {
    let Some(enc) = rs_encoding(key) else { return };
    let c1_undefined = undefined_c1(key);
    let single = enc.is_single_byte();
    let (text, _) = enc.decode_without_bom_handling(data);
    for (i, c) in text.chars().enumerate() {
        let bad = c == '\u{fffd}' || (c1_undefined && ('\u{80}'..='\u{9f}').contains(&c));
        let dec = if bad { Dec::Bad } else { Dec::Ch(c as u32) };
        v.push(if single {
            item(dec, i, 1)
        } else {
            item(dec, 0, 0)
        });
        if v.stop {
            break;
        }
    }
}
