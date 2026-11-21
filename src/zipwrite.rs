//! Writing ZIP archives for `zip` (a child module of `bin/zip.rs`): local headers patched after
//! the data (the output is a seekable file), central directory, raw copy of kept entries.
//! Zip64 records are written only where a classic field would overflow: a member or an offset
//! of 4 GiB or more, or 65535 entries or more.

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};

use crate::zipfmt::{Entry, SIG_CENTRAL, SIG_EOCD, SIG_EOCD64, SIG_LOC64, SIG_LOCAL, each_extra};

/// Version made by: Unix, Info-ZIP 3.0.
const VERSION_MADE: u16 = 0x031e;
/// The value a 32-bit field holds when the real one is in the Zip64 extra field.
const MAX32: u64 = 0xffff_ffff;

fn field32(v: u64) -> [u8; 4] {
    (v.min(MAX32) as u32).to_le_bytes()
}

/// The Zip64 extra field (id 1) holding `vals`; nothing when `vals` is empty.
fn zip64_extra(vals: &[u64]) -> Vec<u8> {
    let mut x = Vec::new();
    if !vals.is_empty() {
        x.extend_from_slice(&[1, 0]);
        x.extend_from_slice(&((vals.len() * 8) as u16).to_le_bytes());
        for v in vals {
            x.extend_from_slice(&v.to_le_bytes());
        }
    }
    x
}

pub struct New {
    pub name: String,
    pub deflated: bool,
    pub level: u32,
    pub dos_time: u16,
    pub dos_date: u16,
    pub mtime_utc: i64,
    pub crc: u32,
    pub csize: u64,
    pub usize: u64,
    pub offset: u64,
    /// The source is 4 GiB or more: the local header carries the sizes in a Zip64 extra field.
    pub zip64: bool,
    pub text: bool,
    pub ext_attr: u32,
}

impl New {
    fn flags(&self) -> u16 {
        let mut f = if self.deflated {
            match self.level {
                8 | 9 => 2,
                2 => 4,
                1 => 6,
                _ => 0,
            }
        } else {
            0
        };
        if !self.name.is_ascii() {
            f |= 0x800;
        }
        f
    }

    /// The extra fields: the Zip64 one with `zip64` values first, then the timestamp.
    fn extra(&self, zip64: &[u64]) -> Vec<u8> {
        let mut x = zip64_extra(zip64);
        x.extend_from_slice(&[0x55, 0x54, 5, 0, 1]);
        x.extend_from_slice(&(self.mtime_utc.clamp(0, i64::from(u32::MAX)) as u32).to_le_bytes());
        x
    }

    /// The values that do not fit their 32-bit central directory fields, in Zip64 order.
    fn central_zip64(&self) -> Vec<u64> {
        [self.usize, self.csize, self.offset]
            .into_iter()
            .filter(|&v| v >= MAX32)
            .collect()
    }

    fn version_needed(&self) -> u16 {
        if self.zip64 || !self.central_zip64().is_empty() {
            45
        } else if self.deflated || self.name.ends_with('/') {
            20
        } else {
            10
        }
    }

    pub fn local_header(&self) -> Vec<u8> {
        let extra = if self.zip64 {
            self.extra(&[self.usize, self.csize])
        } else {
            self.extra(&[])
        };
        let mut h = Vec::with_capacity(30 + self.name.len() + extra.len());
        h.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        h.extend_from_slice(&self.version_needed().to_le_bytes());
        h.extend_from_slice(&self.flags().to_le_bytes());
        h.extend_from_slice(&(if self.deflated { 8u16 } else { 0 }).to_le_bytes());
        h.extend_from_slice(&self.dos_time.to_le_bytes());
        h.extend_from_slice(&self.dos_date.to_le_bytes());
        h.extend_from_slice(&self.crc.to_le_bytes());
        for v in [self.csize, self.usize] {
            h.extend_from_slice(&field32(if self.zip64 { MAX32 } else { v }));
        }
        h.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        h.extend_from_slice(self.name.as_bytes());
        h.extend_from_slice(&extra);
        h
    }

    pub fn central(&self) -> Vec<u8> {
        let extra = self.extra(&self.central_zip64());
        let mut c = Vec::new();
        c.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
        c.extend_from_slice(&VERSION_MADE.to_le_bytes());
        c.extend_from_slice(&self.version_needed().to_le_bytes());
        c.extend_from_slice(&self.flags().to_le_bytes());
        c.extend_from_slice(&(if self.deflated { 8u16 } else { 0 }).to_le_bytes());
        c.extend_from_slice(&self.dos_time.to_le_bytes());
        c.extend_from_slice(&self.dos_date.to_le_bytes());
        c.extend_from_slice(&self.crc.to_le_bytes());
        c.extend_from_slice(&field32(self.csize));
        c.extend_from_slice(&field32(self.usize));
        c.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        c.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0, 0, 0, 0]); // comment length, disk number
        c.extend_from_slice(&u16::from(self.text).to_le_bytes());
        c.extend_from_slice(&self.ext_attr.to_le_bytes());
        c.extend_from_slice(&field32(self.offset));
        c.extend_from_slice(self.name.as_bytes());
        c.extend_from_slice(&extra);
        c
    }
}

/// The archive being written.
pub struct Out {
    pub w: BufWriter<File>,
    pub central: Vec<Vec<u8>>,
}

impl Out {
    pub fn new(f: File) -> Self {
        Out {
            w: BufWriter::new(f),
            central: Vec::new(),
        }
    }

    pub fn pos(&mut self) -> io::Result<u64> {
        self.w.stream_position()
    }

    /// Writes an entry whose data comes from `src` (`None`: a directory).
    /// Returns the finished record and the compressed size for the percentage.
    pub fn add(
        &mut self,
        mut n: New,
        src: Option<&mut File>,
        reopen: &mut dyn FnMut() -> io::Result<File>,
    ) -> io::Result<New> {
        let start = self.pos()?;
        n.offset = start;
        let Some(src) = src else {
            n.deflated = false;
            self.w.write_all(&n.local_header())?;
            self.central.push(n.central());
            return Ok(n);
        };
        n.zip64 = src.metadata()?.len() >= MAX32;
        self.w.write_all(&n.local_header())?;
        let hdr_len = n.local_header().len() as u64;
        let data_start = start + hdr_len;
        let level = n.level;
        let (crc, total, text) = if level == 0 {
            copy_crc(src, &mut self.w)?
        } else {
            let mut enc =
                flate2::write::DeflateEncoder::new(&mut self.w, flate2::Compression::new(level));
            let r = copy_crc(src, &mut enc)?;
            enc.finish()?;
            r
        };
        n.crc = crc;
        if total >= MAX32 && !n.zip64 {
            // The local header was written without room for 64-bit sizes.
            return Err(io::Error::other("file grew past 4 GiB while being read"));
        }
        n.usize = total;
        n.text = text;
        let mut end = self.pos()?;
        n.deflated = level != 0;
        if n.deflated && (end - data_start >= total || total == 0) {
            // Deflate did not help: store, as Info-ZIP does.
            self.w.flush()?;
            self.w.get_mut().set_len(start)?;
            self.w.seek(SeekFrom::Start(start))?;
            n.deflated = false;
            self.w.write_all(&n.local_header())?;
            let mut again = reopen()?;
            copy_crc(&mut again, &mut self.w)?;
            end = self.pos()?;
        }
        n.csize = end - data_start;
        // Patch method, CRC and sizes in the local header.
        self.w.seek(SeekFrom::Start(start))?;
        self.w.write_all(&n.local_header())?;
        self.w.seek(SeekFrom::Start(end))?;
        self.central.push(n.central());
        Ok(n)
    }

    /// Copies a kept entry of the old archive (local header, data, descriptor) unchanged.
    pub fn copy_raw(&mut self, old: &mut File, e: &Entry) -> io::Result<()> {
        let start = self.pos()?;
        let rec = relocated(e, start)?;
        old.seek(SeekFrom::Start(e.offset))?;
        let mut lh = [0u8; 30];
        old.read_exact(&mut lh)?;
        let name_len = usize::from(u16::from_le_bytes([lh[26], lh[27]]));
        let mut rest = vec![0u8; name_len + usize::from(u16::from_le_bytes([lh[28], lh[29]]))];
        old.read_exact(&mut rest)?;
        self.w.write_all(&lh)?;
        self.w.write_all(&rest)?;
        io::copy(&mut Read::take(&mut *old, e.csize), &mut self.w)?;
        if e.flags & 8 != 0 {
            // A data descriptor follows: CRC and two sizes, 8 bytes each when the local header
            // has a Zip64 extra field, with or without a signature in front.
            let mut wide = false;
            each_extra(&rest[name_len..], |id, _| wide |= id == 1);
            let sizes = if wide { 16 } else { 8 };
            let mut d = [0u8; 4];
            old.read_exact(&mut d)?;
            let more = if u32::from_le_bytes(d) == 0x0807_4b50 {
                4 + sizes
            } else {
                sizes
            };
            self.w.write_all(&d)?;
            io::copy(&mut Read::take(&mut *old, more), &mut self.w)?;
        }
        self.central.push(rec);
        Ok(())
    }

    /// Writes the central directory and end record; returns the archive size.
    pub fn finish(mut self, comment: &[u8]) -> io::Result<u64> {
        let count = self.central.len() as u64;
        let cd_start = self.pos()?;
        let mut size = 0u64;
        for rec in &self.central {
            self.w.write_all(rec)?;
            size += rec.len() as u64;
        }
        let mut e = Vec::new();
        if count >= 0xffff || size >= MAX32 || cd_start >= MAX32 {
            // Zip64 end of central directory record, then its locator.
            e.extend_from_slice(&SIG_EOCD64.to_le_bytes());
            e.extend_from_slice(&44u64.to_le_bytes());
            e.extend_from_slice(&VERSION_MADE.to_le_bytes());
            e.extend_from_slice(&45u16.to_le_bytes());
            e.extend_from_slice(&[0; 8]); // this disk, disk of the directory
            for v in [count, count, size, cd_start] {
                e.extend_from_slice(&v.to_le_bytes());
            }
            e.extend_from_slice(&SIG_LOC64.to_le_bytes());
            e.extend_from_slice(&[0; 4]);
            e.extend_from_slice(&(cd_start + size).to_le_bytes());
            e.extend_from_slice(&1u32.to_le_bytes());
        }
        e.extend_from_slice(&SIG_EOCD.to_le_bytes());
        e.extend_from_slice(&[0, 0, 0, 0]);
        let n = (count.min(0xffff) as u16).to_le_bytes();
        e.extend_from_slice(&n);
        e.extend_from_slice(&n);
        e.extend_from_slice(&field32(size));
        e.extend_from_slice(&field32(cd_start));
        e.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        e.extend_from_slice(comment);
        self.w.write_all(&e)?;
        self.w.flush()?;
        self.pos()
    }
}

/// The central record of a kept entry for its new offset `start`. Sizes and offset go into
/// the Zip64 extra field exactly when they do not fit, so the old one is rebuilt.
fn relocated(e: &Entry, start: u64) -> io::Result<Vec<u8>> {
    let old = &e.central;
    let len = |at: usize| usize::from(u16::from_le_bytes([old[at], old[at + 1]]));
    let (name_end, extra_end) = (46 + len(28), 46 + len(28) + len(30));
    let mut rec = old[..name_end].to_vec();
    let mut wide = Vec::new();
    for (v, at) in [(e.usize, 24), (e.csize, 20), (start, 42)] {
        rec[at..at + 4].copy_from_slice(&field32(v));
        if v >= MAX32 {
            wide.push(v);
        }
    }
    if !wide.is_empty() && u16::from_le_bytes([rec[6], rec[7]]) < 45 {
        rec[6..8].copy_from_slice(&45u16.to_le_bytes());
    }
    let mut extra = zip64_extra(&wide);
    each_extra(&old[name_end..extra_end], |id, d| {
        if id != 1 {
            extra.extend_from_slice(&id.to_le_bytes());
            extra.extend_from_slice(&(d.len() as u16).to_le_bytes());
            extra.extend_from_slice(d);
        }
    });
    let n = u16::try_from(extra.len()).map_err(|_| io::Error::other("extra field too long"))?;
    rec[30..32].copy_from_slice(&n.to_le_bytes());
    rec.extend_from_slice(&extra);
    rec.extend_from_slice(&old[extra_end..]);
    Ok(rec)
}

/// Copies `src` to `dst`, returning (CRC-32, bytes, looks like text).
fn copy_crc<R: Read, W: Write>(src: &mut R, dst: &mut W) -> io::Result<(u32, u64, bool)> {
    let mut crc = flate2::Crc::new();
    let mut total = 0u64;
    let mut text = true;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        total += n as u64;
        text &= !buf[..n]
            .iter()
            .any(|&b| b < 7 || (14..26).contains(&b) || (28..32).contains(&b));
        dst.write_all(&buf[..n])?;
    }
    Ok((crc.sum(), total, text))
}
