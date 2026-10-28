//! The gzip container (RFC 1952) for `gzip`: member headers, deflate streams through flate2's
//! pure-Rust backend, CRC and length trailers, and the error kinds gzip reports.
#![allow(dead_code)]

use std::io::{self, BufRead, Read, Write};

use flate2::{Compression, Crc};

/// Operating system byte of the headers written: Unix, as Git Bash's gzip.
const OS_UNIX: u8 = 3;

/// A parsed member header.
#[derive(Clone, Debug)]
pub struct Header {
    pub mtime: u32,
    pub name: Option<Vec<u8>>,
    /// Header length in bytes (ID bytes through the optional fields).
    pub len: u64,
}

/// What can go wrong while decoding, with gzip's wording in [`DecodeError::text`].
#[derive(Debug)]
pub enum DecodeError {
    NotGzip,
    Truncated,
    Format,
    Crc,
    Length,
    UnsupportedMethod(u8),
    Io(io::Error),
}

impl DecodeError {
    /// The message after `gzip: NAME: `.
    pub fn text(&self) -> String {
        match self {
            Self::NotGzip => "not in gzip format".to_owned(),
            Self::Truncated => "unexpected end of file".to_owned(),
            Self::Format => "invalid compressed data--format violated".to_owned(),
            Self::Crc => "invalid compressed data--crc error".to_owned(),
            Self::Length => "invalid compressed data--length error".to_owned(),
            Self::UnsupportedMethod(m) => format!("unknown method {m} -- not supported"),
            Self::Io(e) => skua_tools::gnu_error(e),
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Self::Truncated
        } else {
            Self::Io(e)
        }
    }
}

/// A `BufRead` that counts the bytes consumed from it.
pub struct Counting<R> {
    inner: R,
    pub consumed: u64,
}

impl<R> Counting<R> {
    pub fn new(inner: R) -> Self {
        Counting { inner, consumed: 0 }
    }
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.consumed += n as u64;
        Ok(n)
    }
}

impl<R: BufRead> BufRead for Counting<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.consumed += amt as u64;
        self.inner.consume(amt);
    }
}

fn byte<R: BufRead>(r: &mut R) -> Result<u8, DecodeError> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn cstring<R: BufRead>(r: &mut R) -> Result<Vec<u8>, DecodeError> {
    let mut v = Vec::new();
    r.read_until(0, &mut v)?;
    if v.last() != Some(&0) {
        return Err(DecodeError::Truncated);
    }
    v.pop();
    Ok(v)
}

/// True when the next bytes start a gzip member.
pub fn at_magic<R: BufRead>(r: &mut R) -> io::Result<bool> {
    Ok(r.fill_buf()?.starts_with(&[0x1f, 0x8b]))
}

/// Reads a member header. `Ok(None)` at a clean end of input.
pub fn read_header<R: BufRead>(r: &mut R) -> Result<Option<Header>, DecodeError> {
    if r.fill_buf()?.is_empty() {
        return Ok(None);
    }
    let mut fixed = [0u8; 10];
    let mut got = 0;
    while got < 10 {
        let n = r.read(&mut fixed[got..])?;
        if n == 0 {
            // Fewer than ten bytes: not a gzip file unless the magic matched.
            return Err(if fixed[..got].starts_with(&[0x1f, 0x8b]) {
                DecodeError::Truncated
            } else {
                DecodeError::NotGzip
            });
        }
        got += n;
    }
    if fixed[..2] != [0x1f, 0x8b] {
        return Err(DecodeError::NotGzip);
    }
    if fixed[2] != 8 {
        return Err(DecodeError::UnsupportedMethod(fixed[2]));
    }
    let flags = fixed[3];
    let mtime = u32::from_le_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]);
    let mut len = 10u64;
    if flags & 4 != 0 {
        let mut l = [0u8; 2];
        r.read_exact(&mut l)?;
        let xlen = u64::from(u16::from_le_bytes(l));
        io::copy(&mut r.by_ref().take(xlen), &mut io::sink())?;
        len += 2 + xlen;
    }
    let mut name = None;
    if flags & 8 != 0 {
        let n = cstring(r)?;
        len += n.len() as u64 + 1;
        name = Some(n);
    }
    if flags & 16 != 0 {
        len += cstring(r)?.len() as u64 + 1;
    }
    if flags & 2 != 0 {
        let mut c = [0u8; 2];
        r.read_exact(&mut c)?;
        len += 2;
    }
    Ok(Some(Header { mtime, name, len }))
}

/// What a decoded member contained.
pub struct Member {
    pub crc: u32,
    pub size: u32,
}

/// Inflates one member's deflate stream (the header is already read) into `out`, checks the
/// trailer, and leaves `r` after it. Output already written stays written on an error, as gzip's.
pub fn decode_body<R: BufRead, W: Write>(r: &mut R, out: &mut W) -> Result<Member, DecodeError> {
    let mut dec = flate2::bufread::DeflateDecoder::new(r.by_ref());
    let mut crc = Crc::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match dec.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                return Err(match e.kind() {
                    io::ErrorKind::UnexpectedEof => DecodeError::Truncated,
                    io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => DecodeError::Format,
                    _ => DecodeError::Io(e),
                });
            }
        };
        crc.update(&buf[..n]);
        total += n as u64;
        out.write_all(&buf[..n]).map_err(DecodeError::Io)?;
    }
    drop(dec);
    let mut t = [0u8; 8];
    r.read_exact(&mut t)?;
    let want_crc = u32::from_le_bytes([t[0], t[1], t[2], t[3]]);
    let want_len = u32::from_le_bytes([t[4], t[5], t[6], t[7]]);
    if want_crc != crc.sum() {
        return Err(DecodeError::Crc);
    }
    if want_len != total as u32 {
        return Err(DecodeError::Length);
    }
    Ok(Member {
        crc: want_crc,
        size: want_len,
    })
}

/// A writer that counts what passes through it.
struct CountingWriter<W> {
    inner: W,
    n: u64,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(b)?;
        self.n += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Compresses `input` into one member on `out`; returns (bytes read, bytes written).
/// `name` and `mtime` go in the header (`None`/0 for stdin and `-n`).
pub fn compress<R: Read, W: Write>(
    input: &mut R,
    out: &mut W,
    name: Option<&[u8]>,
    mtime: u32,
    level: u32,
) -> io::Result<(u64, u64)> {
    let mut w = CountingWriter { inner: out, n: 0 };
    let xfl = match level {
        9 => 2,
        1 => 4,
        _ => 0,
    };
    let mut head = vec![0x1f, 0x8b, 8, if name.is_some() { 8 } else { 0 }];
    head.extend_from_slice(&mtime.to_le_bytes());
    head.extend_from_slice(&[xfl, OS_UNIX]);
    if let Some(n) = name {
        head.extend_from_slice(n);
        head.push(0);
    }
    w.write_all(&head)?;
    let mut enc = flate2::write::DeflateEncoder::new(&mut w, Compression::new(level));
    let mut crc = Crc::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        total += n as u64;
        enc.write_all(&buf[..n])?;
    }
    enc.finish()?;
    let mut trailer = crc.sum().to_le_bytes().to_vec();
    trailer.extend_from_slice(&(total as u32).to_le_bytes());
    w.write_all(&trailer)?;
    Ok((total, w.n))
}

/// gzip's `display_ratio`: ` 96.5%`, `-33.3%` (sign column, one decimal).
pub fn ratio(num: i64, den: i64) -> String {
    let mut r = if den == 0 {
        0
    } else if den < 2_147_483 {
        1000 * num / den
    } else {
        num / (den / 1000)
    };
    let sign = if r < 0 {
        r = -r;
        '-'
    } else {
        ' '
    };
    format!("{sign}{:2}.{}%", r / 10, r % 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_and_header() {
        let data = b"alpha\nbeta\n".repeat(30);
        let mut gz = Vec::new();
        let (i, o) = compress(&mut Cursor::new(&data), &mut gz, Some(b"a.txt"), 5, 6).unwrap();
        assert_eq!((i, o), (data.len() as u64, gz.len() as u64));
        assert_eq!(&gz[..10], &[0x1f, 0x8b, 8, 8, 5, 0, 0, 0, 0, 3]);
        let mut r = Cursor::new(gz);
        let h = read_header(&mut r).unwrap().unwrap();
        assert_eq!(h.name.as_deref(), Some(&b"a.txt"[..]));
        assert_eq!(h.len, 16);
        let mut out = Vec::new();
        let m = decode_body(&mut r, &mut out).unwrap();
        assert_eq!(out, data);
        assert_eq!(m.size as usize, data.len());
        assert!(read_header(&mut r).unwrap().is_none());
    }

    #[test]
    fn detects_damage() {
        let mut gz = Vec::new();
        compress(&mut Cursor::new(b"hello\n"), &mut gz, None, 0, 6).unwrap();
        let mut bad = gz.clone();
        let n = bad.len();
        bad[n - 8] ^= 0xff;
        let mut r = Cursor::new(bad);
        read_header(&mut r).unwrap();
        assert!(matches!(
            decode_body(&mut r, &mut Vec::new()),
            Err(DecodeError::Crc)
        ));
        let mut r = Cursor::new(&gz[..gz.len() - 4]);
        read_header(&mut r).unwrap();
        assert!(matches!(
            decode_body(&mut r, &mut Vec::new()),
            Err(DecodeError::Truncated)
        ));
        assert!(matches!(
            read_header(&mut Cursor::new(b"junk")),
            Err(DecodeError::NotGzip)
        ));
    }

    #[test]
    fn ratio_text() {
        assert_eq!(ratio(-2, 6), "-33.3%");
        assert_eq!(ratio(531, 550), " 96.5%");
        assert_eq!(ratio(0, 0), "  0.0%");
    }
}
