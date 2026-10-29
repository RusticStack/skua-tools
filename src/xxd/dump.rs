//! Forward direction: bytes in, hex dump out (normal, `-b`, `-e`, `-ps`, `-i`).

use std::io::{self, Read, Write};

use super::opts::{HexType, Opts};

const EBCDIC: &[u8; 256] = include_bytes!("../data/xxd_ebcdic.dat");

const RESET: &str = "\x1b[0m";

/// Reads until `buf` is full or the input ends; returns the count.
fn read_fill(r: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Dumps `r` to `w`. `name` is the C identifier for `-i` (None: unnamed stdin). `start` is the
/// absolute position the input begins at (after `-s`).
pub fn dump(
    o: &Opts,
    name: Option<&str>,
    r: &mut dyn Read,
    w: &mut dyn Write,
    start: u64,
    color: bool,
) -> io::Result<()> {
    match o.hextype {
        HexType::Postscript => plain(o, r, w),
        HexType::Include => include(o, name, r, w),
        _ => lines(o, r, w, start, color),
    }
}

fn hex(o: &Opts, b: u8, out: &mut String) {
    let t = if o.upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    out.push(t[usize::from(b >> 4)] as char);
    out.push(t[usize::from(b & 15)] as char);
}

fn plain(o: &Opts, r: &mut dyn Read, w: &mut dyn Write) -> io::Result<()> {
    let mut left = o.length;
    let mut col = 0usize;
    let mut out = String::new();
    let mut buf = [0u8; 8192];
    loop {
        let want = left.map_or(buf.len(), |l| l.min(buf.len() as u64) as usize);
        if want == 0 {
            break;
        }
        let n = read_fill(r, &mut buf[..want])?;
        if n == 0 {
            break;
        }
        if let Some(l) = left.as_mut() {
            *l -= n as u64;
        }
        for &b in &buf[..n] {
            hex(o, b, &mut out);
            col += 1;
            if o.cols != 0 && col == o.cols {
                out.push('\n');
                col = 0;
            }
        }
        w.write_all(out.as_bytes())?;
        out.clear();
    }
    if col > 0 {
        w.write_all(b"\n")?;
    }
    Ok(())
}

fn include(o: &Opts, name: Option<&str>, r: &mut dyn Read, w: &mut dyn Write) -> io::Result<()> {
    let var = name.map(|n| {
        if o.capitalize {
            n.to_ascii_uppercase()
        } else {
            n.to_owned()
        }
    });
    if let Some(v) = &var {
        writeln!(w, "unsigned char {v}[] = {{")?;
    }
    let mut p: u64 = 0;
    let cols = o.cols as u64;
    let mut left = o.length;
    let mut eof = false;
    let item = |w: &mut dyn Write, p: u64, text: &str| -> io::Result<()> {
        let sep = if !p.is_multiple_of(cols) {
            ", "
        } else if p == 0 {
            "  "
        } else {
            ",\n  "
        };
        write!(w, "{sep}{text}")
    };
    let mut buf = [0u8; 4096];
    'outer: loop {
        let want = left.map_or(buf.len(), |l| l.min(buf.len() as u64) as usize);
        if want == 0 {
            break;
        }
        let n = read_fill(r, &mut buf[..want])?;
        if n == 0 {
            eof = true;
            break;
        }
        for &b in &buf[..n] {
            let text = if o.bits_include {
                format!("0b{b:08b}")
            } else if o.upper {
                format!("0X{b:02X}")
            } else {
                format!("0x{b:02x}")
            };
            item(w, p, &text)?;
            p += 1;
        }
        if let Some(l) = left.as_mut() {
            *l -= n as u64;
        }
        if n < want {
            eof = true;
            break 'outer;
        }
    }
    if eof && o.terminate {
        let text = if o.bits_include {
            "0b00000000"
        } else if o.upper {
            "0X00"
        } else {
            "0x00"
        };
        item(w, p, text)?;
    }
    if let Some(v) = &var {
        if p > 0 {
            w.write_all(b"\n")?;
        }
        w.write_all(b"};\n")?;
        let len = if o.capitalize {
            format!("{v}_LEN")
        } else {
            format!("{v}_len")
        };
        writeln!(w, "unsigned int {len} = {p};")?;
    } else if p > 0 {
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// The colour class of a byte for `-R`: SGR digit.
fn color_of(b: u8) -> u8 {
    match b {
        0 => 7,
        b'\t' | b'\n' | b'\r' => 3,
        0x20..=0x7e => 2,
        0xff => 4,
        _ => 1,
    }
}

fn start_color(out: &mut String, c: u8) {
    out.push_str("\x1b[1;3");
    out.push((b'0' + c) as char);
    out.push('m');
}

/// State of `-a` (autoskip), a port of xxd's `xxdline`.
struct Skip {
    z: String,
    zero_seen: i32,
}

impl Skip {
    fn line(&mut self, w: &mut dyn Write, l: &str, nz: i64) -> io::Result<()> {
        if nz == 0 && self.zero_seen == 1 {
            self.z = l.to_owned();
        }
        let go = if nz != 0 {
            true
        } else {
            let first = self.zero_seen == 0;
            self.zero_seen += 1;
            first
        };
        if go {
            if nz != 0 {
                if nz < 0 {
                    self.zero_seen -= 1;
                }
                if self.zero_seen == 2 {
                    w.write_all(self.z.as_bytes())?;
                }
                if self.zero_seen > 2 {
                    w.write_all(b"*\n")?;
                }
            }
            if nz >= 0 || self.zero_seen > 0 {
                w.write_all(l.as_bytes())?;
            }
            if nz != 0 {
                self.zero_seen = 0;
            }
        }
        Ok(())
    }
}

fn lines(o: &Opts, r: &mut dyn Read, w: &mut dyn Write, start: u64, color: bool) -> io::Result<()> {
    let color = color && matches!(o.hextype, HexType::Normal | HexType::Bits);
    let mut left = o.length;
    let mut pos = start.wrapping_add(o.display_off as u64);
    let mut buf = vec![0u8; o.cols];
    let mut skip = Skip {
        z: String::new(),
        zero_seen: 0,
    };
    let mut last = String::new();
    let mut partial = false;
    loop {
        let want = left.map_or(o.cols, |l| l.min(o.cols as u64) as usize);
        if want == 0 {
            break;
        }
        let n = read_fill(r, &mut buf[..want])?;
        if n == 0 {
            break;
        }
        if let Some(l) = left.as_mut() {
            *l -= n as u64;
        }
        let text = line(o, &buf[..n], pos, color);
        pos = pos.wrapping_add(n as u64);
        if n < o.cols {
            skip.line(w, &text, 1)?;
            partial = true;
            break;
        }
        let nonzero = buf[..n].iter().filter(|b| **b != 0).count() as i64;
        skip.line(w, &text, if o.autoskip { nonzero } else { 1 })?;
        last = text;
    }
    if !partial && o.autoskip {
        skip.line(w, &last, -1)?;
    }
    Ok(())
}

fn offset(o: &Opts, pos: u64) -> String {
    if o.decimal {
        format!("{pos:08}: ")
    } else {
        format!("{pos:08x}: ")
    }
}

/// One dump line (with newline) for `data` (at most `cols` bytes) at position `pos`.
pub fn line(o: &Opts, data: &[u8], pos: u64, color: bool) -> String {
    let bits = o.hextype == HexType::Bits;
    let cols = o.cols;
    let g = if o.group == 0 { usize::MAX } else { o.group };
    let cell_w = if bits { 8 } else { 2 };
    let n = data.len();
    let cell = |b: u8, out: &mut String| {
        if bits {
            out.push_str(&format!("{b:08b}"));
        } else {
            hex(o, b, out);
        }
    };
    // A group separator follows octet `i` unless it is the last column.
    let sep_after = |i: usize| usize::from((i + 1).is_multiple_of(g) && i + 1 < cols);
    let mut s = offset(o, pos);
    let mut field = String::new();
    if o.hextype == HexType::Little {
        let mut gs = 0;
        while gs < cols {
            let ge = if g == usize::MAX { cols } else { gs + g };
            for i in (gs..ge).rev() {
                if i < n {
                    cell(data[i], &mut field);
                } else {
                    field.push_str("  ");
                }
            }
            gs = ge;
            if gs < cols {
                field.push(' ');
            }
        }
        s.push_str(&field);
        s.push_str("  ");
        return text_area(o, s, data, false);
    }
    let mut open: Option<u8> = None;
    for (i, &byte) in data.iter().enumerate() {
        if color {
            let c = color_of(byte);
            if open != Some(c) {
                if open.is_some() {
                    field.push_str(RESET);
                }
                start_color(&mut field, c);
                open = Some(c);
            }
        }
        cell(byte, &mut field);
        if sep_after(i) == 1 && (i + 1 < n || !color || bits) {
            if open.take().is_some() {
                field.push_str(RESET);
            }
            field.push(' ');
        }
    }
    if open.take().is_some() {
        field.push_str(RESET);
    }
    // The blanks from the last real octet to the text.
    let missing = cols - n;
    let mut gap = 2;
    if n > 0 && (color && !bits && missing > 0) {
        gap += sep_after(n - 1);
    }
    if !(color && !bits) || missing == 0 {
        // Plain layout: pad every missing octet, with its separator.
        for i in n..cols {
            field.push_str(&" ".repeat(cell_w));
            if sep_after(i) == 1 {
                field.push(' ');
            }
        }
        s.push_str(&field);
        s.push_str("  ");
        return text_area(o, s, data, color);
    }
    for i in n..cols {
        gap += 2 + sep_after(i);
    }
    // xxd paints as many blanks as there are missing octets red, just before the text.
    let post = 2 - sep_after(n - 1);
    let pre = gap.saturating_sub(missing + post);
    s.push_str(&field);
    s.push_str(&" ".repeat(pre));
    start_color(&mut s, 1);
    s.push_str(&" ".repeat(missing));
    s.push_str(RESET);
    s.push_str(&" ".repeat(post));
    text_area(o, s, data, color)
}

fn text_area(o: &Opts, mut s: String, data: &[u8], color: bool) -> String {
    let mut open: Option<u8> = None;
    for &b in data {
        let ch = if o.ebcdic {
            EBCDIC[usize::from(b)] as char
        } else if (0x20..0x7f).contains(&b) {
            b as char
        } else {
            '.'
        };
        if color {
            let c = color_of(b);
            if open != Some(c) {
                if open.is_some() {
                    s.push_str(RESET);
                }
                start_color(&mut s, c);
                open = Some(c);
            }
        }
        s.push(ch);
    }
    if open.is_some() {
        s.push_str(RESET);
    }
    s.push('\n');
    s
}
