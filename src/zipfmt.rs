//! The ZIP container for `unzip` and `zip` (included with `#[path]`): reading the central
//! directory, streaming one entry's data out with a CRC check, and Info-ZIP's wildcard rules.
//! Only stored and deflated entries are handled (what Info-ZIP's `zip` writes); no encryption.
#![allow(dead_code)]

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};

use flate2::Crc;

pub const SIG_LOCAL: u32 = 0x0403_4b50;
pub const SIG_CENTRAL: u32 = 0x0201_4b50;
pub const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_EOCD64: u32 = 0x0606_4b50;
const SIG_LOC64: u32 = 0x0706_4b50;

/// One central-directory record.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub version_made: u16,
    pub version_needed: u16,
    pub flags: u16,
    pub method: u16,
    pub dos_time: u16,
    pub dos_date: u16,
    pub crc: u32,
    pub csize: u64,
    pub usize: u64,
    pub offset: u64,
    pub int_attr: u16,
    pub ext_attr: u32,
    /// Modification time from the extended-timestamp (`UT`) extra field, Unix seconds.
    pub mtime_utc: Option<i64>,
    /// The raw record, kept so an updated archive can carry it over unchanged.
    pub central: Vec<u8>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }

    /// The Unix mode stored by a Unix host (zero otherwise).
    pub fn unix_mode(&self) -> u32 {
        if self.version_made >> 8 == 3 {
            self.ext_attr >> 16
        } else {
            0
        }
    }
}

pub struct Archive {
    pub entries: Vec<Entry>,
    pub comment: Vec<u8>,
    pub size: u64,
    pub cd_offset: u64,
}

#[derive(Debug)]
pub enum ZipError {
    /// No end-of-central-directory record: not a zip file.
    NotZip,
    Corrupt(String),
    Io(io::Error),
}

impl From<io::Error> for ZipError {
    fn from(e: io::Error) -> Self {
        ZipError::Io(e)
    }
}

fn u16le(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn u32le(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}
fn u64le(b: &[u8], i: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(a)
}

/// Decodes a stored name: UTF-8 when flagged or valid, otherwise Latin-1.
fn decode_name(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_owned(),
        Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Walks extra fields: calls `f(id, data)` for each.
pub fn each_extra(extra: &[u8], mut f: impl FnMut(u16, &[u8])) {
    let mut i = 0;
    while i + 4 <= extra.len() {
        let (id, len) = (u16le(extra, i), usize::from(u16le(extra, i + 2)));
        let end = (i + 4 + len).min(extra.len());
        f(id, &extra[i + 4..end]);
        i += 4 + len;
    }
}

/// Reads the central directory of an open archive.
pub fn read_archive(f: &mut File) -> Result<Archive, ZipError> {
    let size = f.seek(SeekFrom::End(0))?;
    let tail_len = size.min(65_557 + 22);
    f.seek(SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; tail_len as usize];
    f.read_exact(&mut tail)?;
    let pos = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            u32le(&tail, i) == SIG_EOCD && i + 22 + usize::from(u16le(&tail, i + 20)) <= tail.len()
        })
        .ok_or(ZipError::NotZip)?;
    let eocd = &tail[pos..];
    let mut count = u64::from(u16le(eocd, 10));
    let mut cd_size = u64::from(u32le(eocd, 12));
    let mut cd_offset = u64::from(u32le(eocd, 16));
    let comment = eocd[22..22 + usize::from(u16le(eocd, 20))].to_vec();
    let eocd_abs = size - tail_len + pos as u64;
    // Zip64 locator right before the EOCD.
    if eocd_abs >= 20 && (count == 0xffff || cd_size == 0xffff_ffff || cd_offset == 0xffff_ffff) {
        f.seek(SeekFrom::Start(eocd_abs - 20))?;
        let mut loc = [0u8; 20];
        f.read_exact(&mut loc)?;
        if u32le(&loc, 0) == SIG_LOC64 {
            f.seek(SeekFrom::Start(u64le(&loc, 8)))?;
            let mut z = [0u8; 56];
            f.read_exact(&mut z)?;
            if u32le(&z, 0) == SIG_EOCD64 {
                count = u64le(&z, 32);
                cd_size = u64le(&z, 40);
                cd_offset = u64le(&z, 48);
            }
        }
    }
    if cd_offset.checked_add(cd_size).is_none_or(|e| e > size) {
        return Err(ZipError::Corrupt("central directory out of range".into()));
    }
    f.seek(SeekFrom::Start(cd_offset))?;
    let mut cd = vec![0u8; cd_size as usize];
    f.read_exact(&mut cd)?;
    let mut entries = Vec::new();
    let mut i = 0usize;
    for _ in 0..count {
        if i + 46 > cd.len() || u32le(&cd, i) != SIG_CENTRAL {
            return Err(ZipError::Corrupt("bad central directory entry".into()));
        }
        let (n, m, k) = (
            usize::from(u16le(&cd, i + 28)),
            usize::from(u16le(&cd, i + 30)),
            usize::from(u16le(&cd, i + 32)),
        );
        let end = i + 46 + n + m + k;
        if end > cd.len() {
            return Err(ZipError::Corrupt("bad central directory entry".into()));
        }
        let mut e = Entry {
            name: decode_name(&cd[i + 46..i + 46 + n]),
            version_made: u16le(&cd, i + 4),
            version_needed: u16le(&cd, i + 6),
            flags: u16le(&cd, i + 8),
            method: u16le(&cd, i + 10),
            dos_time: u16le(&cd, i + 12),
            dos_date: u16le(&cd, i + 14),
            crc: u32le(&cd, i + 16),
            csize: u64::from(u32le(&cd, i + 20)),
            usize: u64::from(u32le(&cd, i + 24)),
            offset: u64::from(u32le(&cd, i + 42)),
            int_attr: u16le(&cd, i + 36),
            ext_attr: u32le(&cd, i + 38),
            mtime_utc: None,
            central: cd[i..end].to_vec(),
        };
        let (want_u, want_c, want_o) = (
            e.usize == 0xffff_ffff,
            e.csize == 0xffff_ffff,
            e.offset == 0xffff_ffff,
        );
        each_extra(&cd[i + 46 + n..i + 46 + n + m], |id, d| match id {
            0x0001 => {
                let mut p = 0;
                for (want, slot) in [
                    (want_u, &mut e.usize),
                    (want_c, &mut e.csize),
                    (want_o, &mut e.offset),
                ] {
                    if want && p + 8 <= d.len() {
                        *slot = u64le(d, p);
                        p += 8;
                    }
                }
            }
            0x5455 if d.len() >= 5 && d[0] & 1 != 0 => {
                e.mtime_utc = Some(i64::from(u32le(d, 1)));
            }
            _ => {}
        });
        entries.push(e);
        i = end;
    }
    Ok(Archive {
        entries,
        comment,
        size,
        cd_offset,
    })
}

/// What can fail while extracting one entry.
#[derive(Debug)]
pub enum DataError {
    BadLocalHeader,
    Method(u16),
    Inflate,
    Crc(u32),
    Truncated,
    Io(io::Error),
}

/// Streams the uncompressed data of `e` into `out`, checking size and CRC.
pub fn read_entry<W: Write>(f: &mut File, e: &Entry, out: &mut W) -> Result<(), DataError> {
    if e.method != 0 && e.method != 8 {
        return Err(DataError::Method(e.method));
    }
    f.seek(SeekFrom::Start(e.offset)).map_err(DataError::Io)?;
    let mut lh = [0u8; 30];
    f.read_exact(&mut lh)
        .map_err(|_| DataError::BadLocalHeader)?;
    if u32le(&lh, 0) != SIG_LOCAL {
        return Err(DataError::BadLocalHeader);
    }
    let skip = i64::from(u16le(&lh, 26)) + i64::from(u16le(&lh, 28));
    f.seek(SeekFrom::Current(skip)).map_err(DataError::Io)?;
    let raw = BufReader::new(Read::take(&*f, e.csize));
    let mut crc = Crc::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    let mut src: Box<dyn Read> = if e.method == 8 {
        Box::new(flate2::bufread::DeflateDecoder::new(raw))
    } else {
        Box::new(raw)
    };
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(DataError::Truncated);
            }
            Err(_) => return Err(DataError::Inflate),
        };
        crc.update(&buf[..n]);
        total += n as u64;
        out.write_all(&buf[..n]).map_err(DataError::Io)?;
    }
    if total != e.usize {
        return Err(DataError::Truncated);
    }
    if crc.sum() != e.crc {
        return Err(DataError::Crc(crc.sum()));
    }
    Ok(())
}

/// Info-ZIP's wildcard match. `*` and `?` stop at `/` when `stop_at_slash` (unzip's rule; `**`
/// still matches everything). `[...]` classes with `!`/`^`, `\` escapes.
pub fn glob_match(pattern: &str, text: &str, stop_at_slash: bool, ignore_case: bool) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    go(&p, &t, stop_at_slash, ignore_case)
}

fn eq(a: char, b: char, ic: bool) -> bool {
    a == b || (ic && a.to_lowercase().eq(b.to_lowercase()))
}

fn go(p: &[char], t: &[char], stop: bool, ic: bool) -> bool {
    let Some(&c) = p.first() else {
        return t.is_empty();
    };
    match c {
        '*' => {
            let double = p.get(1) == Some(&'*');
            let rest = if double { &p[2..] } else { &p[1..] };
            let free = double || !stop;
            let mut i = 0;
            loop {
                if go(rest, &t[i..], stop, ic) {
                    return true;
                }
                if i >= t.len() || (!free && t[i] == '/') {
                    return false;
                }
                i += 1;
            }
        }
        '?' => !t.is_empty() && (!stop || t[0] != '/') && go(&p[1..], &t[1..], stop, ic),
        '[' => {
            let Some(&ch) = t.first() else { return false };
            let Some(close) = p[1..].iter().skip(1).position(|&x| x == ']').map(|i| i + 2) else {
                return eq('[', ch, ic) && go(&p[1..], &t[1..], stop, ic);
            };
            let set = &p[1..close];
            let (neg, set) = match set.first() {
                Some('!' | '^') => (true, &set[1..]),
                _ => (false, set),
            };
            let mut hit = false;
            let mut k = 0;
            while k < set.len() {
                if k + 2 < set.len() && set[k + 1] == '-' {
                    let lo = set[k].min(set[k + 2]);
                    let hi = set[k].max(set[k + 2]);
                    let lc = ch.to_lowercase().next().unwrap_or(ch);
                    hit |= (lo..=hi).contains(&ch) || (ic && (lo..=hi).contains(&lc));
                    k += 3;
                } else {
                    hit |= eq(set[k], ch, ic);
                    k += 1;
                }
            }
            hit != neg && go(&p[close + 1..], &t[1..], stop, ic)
        }
        '\\' if p.len() > 1 => {
            !t.is_empty() && eq(p[1], t[0], ic) && go(&p[2..], &t[1..], stop, ic)
        }
        _ => !t.is_empty() && eq(c, t[0], ic) && go(&p[1..], &t[1..], stop, ic),
    }
}

/// Whether the pattern has wildcard characters.
pub fn has_wildcards(p: &str) -> bool {
    p.contains(['*', '?', '['])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unzip_wildcards() {
        assert!(glob_match("d/*", "d/x.txt", true, false));
        assert!(glob_match("d/*", "d/", true, false));
        assert!(!glob_match("d/*", "d/s/y.txt", true, false));
        assert!(glob_match("d/**", "d/s/y.txt", true, false));
        assert!(!glob_match("*y*", "d/s/y.txt", true, false));
        assert!(glob_match("*.txt", "top.txt", true, false));
        assert!(!glob_match("*.txt", "d/x.txt", true, false));
        assert!(glob_match("d/[xy].txt", "d/x.txt", true, false));
        assert!(glob_match("d/[!y].txt", "d/x.txt", true, false));
        assert!(glob_match("D/X.TXT", "d/x.txt", true, true));
        assert!(!glob_match("D/X.TXT", "d/x.txt", true, false));
    }

    #[test]
    fn zip_wildcards_cross_slash() {
        assert!(glob_match("*.o", "a/b/c.o", false, false));
        assert!(glob_match("a/*", "a/b/c", false, false));
    }
}
