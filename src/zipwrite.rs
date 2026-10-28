//! Writing ZIP archives for `zip` (a child module of `bin/zip.rs`): local headers patched after
//! the data (the output is a seekable file), central directory, raw copy of kept entries.
//! No Zip64: members and archives are limited to 4 GiB and 65535 entries.

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};

use crate::zipfmt::{Entry, SIG_CENTRAL, SIG_EOCD, SIG_LOCAL};

/// Version made by: Unix, Info-ZIP 3.0.
const VERSION_MADE: u16 = 0x031e;

pub struct New {
    pub name: String,
    pub deflated: bool,
    pub level: u32,
    pub dos_time: u16,
    pub dos_date: u16,
    pub mtime_utc: i64,
    pub crc: u32,
    pub csize: u32,
    pub usize: u32,
    pub offset: u32,
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

    fn extra(&self) -> Vec<u8> {
        let mut x = vec![0x55, 0x54, 5, 0, 1];
        x.extend_from_slice(&(self.mtime_utc.clamp(0, i64::from(u32::MAX)) as u32).to_le_bytes());
        x
    }

    fn version_needed(&self) -> u16 {
        if self.deflated || self.name.ends_with('/') {
            20
        } else {
            10
        }
    }

    pub fn local_header(&self) -> Vec<u8> {
        let extra = self.extra();
        let mut h = Vec::with_capacity(30 + self.name.len() + extra.len());
        h.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        h.extend_from_slice(&self.version_needed().to_le_bytes());
        h.extend_from_slice(&self.flags().to_le_bytes());
        h.extend_from_slice(&(if self.deflated { 8u16 } else { 0 }).to_le_bytes());
        h.extend_from_slice(&self.dos_time.to_le_bytes());
        h.extend_from_slice(&self.dos_date.to_le_bytes());
        for v in [self.crc, self.csize, self.usize] {
            h.extend_from_slice(&v.to_le_bytes());
        }
        h.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        h.extend_from_slice(self.name.as_bytes());
        h.extend_from_slice(&extra);
        h
    }

    pub fn central(&self) -> Vec<u8> {
        let extra = self.extra();
        let mut c = Vec::new();
        c.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
        c.extend_from_slice(&VERSION_MADE.to_le_bytes());
        c.extend_from_slice(&self.version_needed().to_le_bytes());
        c.extend_from_slice(&self.flags().to_le_bytes());
        c.extend_from_slice(&(if self.deflated { 8u16 } else { 0 }).to_le_bytes());
        c.extend_from_slice(&self.dos_time.to_le_bytes());
        c.extend_from_slice(&self.dos_date.to_le_bytes());
        for v in [self.crc, self.csize, self.usize] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        c.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0, 0, 0, 0]); // comment length, disk number
        c.extend_from_slice(&u16::from(self.text).to_le_bytes());
        c.extend_from_slice(&self.ext_attr.to_le_bytes());
        c.extend_from_slice(&self.offset.to_le_bytes());
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
        if start > u64::from(u32::MAX) {
            return Err(io::Error::other("Zip64 is not supported"));
        }
        n.offset = start as u32;
        let Some(src) = src else {
            n.deflated = false;
            self.w.write_all(&n.local_header())?;
            self.central.push(n.central());
            return Ok(n);
        };
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
        n.usize = u32::try_from(total).map_err(|_| io::Error::other("Zip64 is not supported"))?;
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
        n.csize = (end - data_start) as u32;
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
        if start > u64::from(u32::MAX) {
            return Err(io::Error::other("Zip64 is not supported"));
        }
        old.seek(SeekFrom::Start(e.offset))?;
        let mut lh = [0u8; 30];
        old.read_exact(&mut lh)?;
        let skip = u64::from(u16::from_le_bytes([lh[26], lh[27]]))
            + u64::from(u16::from_le_bytes([lh[28], lh[29]]));
        old.seek(SeekFrom::Start(e.offset))?;
        io::copy(&mut Read::take(&mut *old, 30 + skip + e.csize), &mut self.w)?;
        if e.flags & 8 != 0 {
            // A data descriptor follows: 12 bytes, or 16 with its signature.
            let mut d = [0u8; 4];
            old.read_exact(&mut d)?;
            let more = if u32::from_le_bytes(d) == 0x0807_4b50 {
                12
            } else {
                8
            };
            self.w.write_all(&d)?;
            io::copy(&mut Read::take(&mut *old, more), &mut self.w)?;
        }
        let mut rec = e.central.clone();
        rec[42..46].copy_from_slice(&(start as u32).to_le_bytes());
        self.central.push(rec);
        Ok(())
    }

    /// Writes the central directory and end record; returns the archive size.
    pub fn finish(mut self, comment: &[u8]) -> io::Result<u64> {
        if self.central.len() > 0xfffe {
            return Err(io::Error::other("Zip64 is not supported"));
        }
        let cd_start = self.pos()?;
        let mut size = 0u64;
        for rec in &self.central {
            self.w.write_all(rec)?;
            size += rec.len() as u64;
        }
        let mut e = Vec::new();
        e.extend_from_slice(&SIG_EOCD.to_le_bytes());
        e.extend_from_slice(&[0, 0, 0, 0]);
        let n = self.central.len() as u16;
        e.extend_from_slice(&n.to_le_bytes());
        e.extend_from_slice(&n.to_le_bytes());
        e.extend_from_slice(&(size as u32).to_le_bytes());
        e.extend_from_slice(&(cd_start as u32).to_le_bytes());
        e.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        e.extend_from_slice(comment);
        self.w.write_all(&e)?;
        self.w.flush()?;
        self.pos()
    }
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
