//! `xxd -r`: a port of xxd's `huntype`, including its quirks (a single space after the hex
//! digits does not end the hex part, so ASCII text next to it can be taken for hex).

use std::fs::File;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

use super::opts::{Fail, HexType, Opts};
use skua_tools::gnu_error;

/// Where reverted bytes go: a regular file can seek, a pipe only moves forward.
pub trait Sink {
    /// Moves `delta` bytes forward (or backward for a file).
    fn skip(&mut self, delta: i64) -> Result<(), Fail>;
    fn put(&mut self, b: u8) -> io::Result<()>;
    fn finish(&mut self) -> io::Result<()>;
}

pub struct PipeSink<'a> {
    out: &'a mut dyn Write,
}

impl<'a> PipeSink<'a> {
    pub fn new(out: &'a mut dyn Write) -> Self {
        PipeSink { out }
    }
}

impl Sink for PipeSink<'_> {
    fn skip(&mut self, delta: i64) -> Result<(), Fail> {
        if delta < 0 {
            return Err(Fail::msg(5, "Sorry, cannot seek backwards."));
        }
        let zeros = [0u8; 4096];
        let mut left = delta as u64;
        while left > 0 {
            let k = left.min(zeros.len() as u64) as usize;
            self.out.write_all(&zeros[..k]).map_err(write_fail)?;
            left -= k as u64;
        }
        Ok(())
    }
    fn put(&mut self, b: u8) -> io::Result<()> {
        self.out.write_all(&[b])
    }
    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// An output file, patched in place (not truncated).
pub struct FileSink {
    f: io::BufWriter<File>,
}

impl FileSink {
    pub fn new(f: File) -> Self {
        FileSink {
            f: io::BufWriter::new(f),
        }
    }
}

impl Sink for FileSink {
    fn skip(&mut self, delta: i64) -> Result<(), Fail> {
        match self.f.seek(SeekFrom::Current(delta)) {
            Ok(_) => Ok(()),
            Err(_) if delta < 0 => Err(Fail::msg(5, "Sorry, cannot seek backwards.")),
            Err(e) => Err(write_fail(e)),
        }
    }
    fn put(&mut self, b: u8) -> io::Result<()> {
        self.f.write_all(&[b])
    }
    fn finish(&mut self) -> io::Result<()> {
        self.f.flush()
    }
}

fn write_fail(e: io::Error) -> Fail {
    Fail::msg(3, gnu_error(&e))
}

fn hex_digit(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => i32::from(c - b'0'),
        b'a'..=b'f' => i32::from(c - b'a' + 10),
        b'A'..=b'F' => i32::from(c - b'A' + 10),
        _ => -1,
    }
}

struct Bytes<'a> {
    r: io::BufReader<&'a mut dyn Read>,
}

impl Bytes<'_> {
    fn next(&mut self) -> Option<u8> {
        let b = {
            let buf = self.r.fill_buf().ok()?;
            *buf.first()?
        };
        self.r.consume(1);
        Some(b)
    }
    /// `skip_to_eol`: consumes up to and including the newline; returns the last byte read.
    fn skip_to_eol(&mut self, mut c: Option<u8>) -> Option<u8> {
        while c != Some(b'\n') && c.is_some() {
            c = self.next();
        }
        c
    }
}

pub fn revert(o: &Opts, input: &mut dyn Read, sink: &mut dyn Sink) -> Result<(), Fail> {
    let mut rd = Bytes {
        r: io::BufReader::new(input),
    };
    let base_off: i64 = match o.seek {
        Some(s) if s.negative => -s.amount,
        Some(s) => s.amount,
        None => 0,
    };
    let r = if o.hextype == HexType::Bits {
        hunt_bits(o, &mut rd, sink, base_off)
    } else {
        hunt(o, &mut rd, sink, base_off)
    };
    r?;
    sink.finish().map_err(write_fail)
}

/// Moves the output to `target` (zeros on a pipe, a seek on a file).
fn goto(sink: &mut dyn Sink, have: &mut i64, target: i64) -> Result<(), Fail> {
    if target != *have {
        sink.skip(target - *have)?;
        *have = target;
    }
    Ok(())
}

fn hunt(o: &Opts, rd: &mut Bytes, sink: &mut dyn Sink, base_off: i64) -> Result<(), Fail> {
    let plain = o.hextype == HexType::Postscript;
    let cols = if o.cols == 0 { usize::MAX } else { o.cols };
    let (mut n1, mut n2, mut n3): (i32, i32, i32);
    n1 = -1;
    n2 = 0;
    n3 = 0;
    let _ = n3;
    let mut p = cols;
    let mut ign_garb = true;
    let (mut have_off, mut want_off): (i64, i64) = (0, 0);
    while let Some(mut c) = rd.next() {
        if c == b'\r' {
            continue;
        }
        if plain && matches!(c, b' ' | b'\n' | b'\t') {
            continue;
        }
        n3 = n2;
        n2 = n1;
        n1 = hex_digit(c);
        if n1 == -1 && ign_garb {
            continue;
        }
        ign_garb = false;
        if !plain && p >= cols {
            if n1 < 0 {
                p = 0;
                continue;
            }
            want_off = (want_off << 4) | i64::from(n1);
            continue;
        }
        goto(sink, &mut have_off, base_off.wrapping_add(want_off))?;
        if n2 >= 0 && n1 >= 0 {
            sink.put(((n2 << 4) | n1) as u8).map_err(write_fail)?;
            have_off += 1;
            want_off += 1;
            n1 = -1;
            if !plain {
                p += 1;
                if p >= cols {
                    c = rd.skip_to_eol(Some(c)).unwrap_or(0);
                }
            }
        } else if n1 < 0 && n2 < 0 && n3 < 0 {
            c = rd.skip_to_eol(Some(c)).unwrap_or(0);
        }
        if c == b'\n' {
            if !plain {
                want_off = 0;
            }
            p = cols;
            ign_garb = true;
        }
    }
    Ok(())
}

/// `-r -b`: offsets are hex, the data is groups of eight binary digits.
fn hunt_bits(o: &Opts, rd: &mut Bytes, sink: &mut dyn Sink, base_off: i64) -> Result<(), Fail> {
    let cols = o.cols.max(1);
    let mut have_off: i64 = 0;
    let mut want_off: i64;
    let mut line: Vec<u8> = Vec::new();
    loop {
        line.clear();
        let mut eof = true;
        while let Some(c) = rd.next() {
            eof = false;
            if c == b'\n' {
                break;
            }
            if c != b'\r' {
                line.push(c);
            }
        }
        if eof && line.is_empty() {
            return Ok(());
        }
        let Some(colon) = line.iter().position(|b| !b.is_ascii_hexdigit()) else {
            continue;
        };
        if line[colon] != b':' {
            continue;
        }
        want_off = line[..colon]
            .iter()
            .fold(0i64, |a, b| (a << 4) | i64::from(hex_digit(*b)));
        let mut acc = 0u32;
        let mut nbits = 0;
        let mut bytes = 0;
        let mut blanks = 0;
        for &b in &line[colon + 1..] {
            match b {
                b'0' | b'1' => {
                    blanks = 0;
                    acc = (acc << 1) | u32::from(b - b'0');
                    nbits += 1;
                    if nbits == 8 {
                        goto(sink, &mut have_off, base_off.wrapping_add(want_off))?;
                        sink.put(acc as u8).map_err(write_fail)?;
                        have_off += 1;
                        want_off += 1;
                        (acc, nbits) = (0, 0);
                        bytes += 1;
                        if bytes >= cols {
                            break;
                        }
                    }
                }
                b' ' if nbits == 0 => {
                    blanks += 1;
                    if blanks >= 2 {
                        break;
                    }
                }
                _ => break,
            }
        }
        let _ = want_off;
        if eof {
            return Ok(());
        }
    }
}

/// The process's stdout as a seekable sink when it is a regular file (`xxd -r > file`), as
/// the C original seeks there; `None` for a pipe or console (use [`PipeSink`]).
pub fn stdout_file_sink() -> Option<FileSink> {
    #[cfg(windows)]
    let f = {
        use std::os::windows::io::AsHandle;
        let h = io::stdout().as_handle().try_clone_to_owned().ok()?;
        File::from(h)
    };
    #[cfg(unix)]
    let f = {
        use std::os::fd::AsFd;
        let h = io::stdout().as_fd().try_clone_to_owned().ok()?;
        File::from(h)
    };
    let mut f = f;
    // Seeking only works on regular files: a Windows anonymous pipe accepts SetFilePointerEx
    // without moving anything (Skua Bash's pipes), so ask for the file type first.
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        unsafe extern "system" {
            fn GetFileType(handle: *mut std::ffi::c_void) -> u32;
        }
        const FILE_TYPE_DISK: u32 = 1;
        // SAFETY: a plain query on a handle that `f` owns.
        if unsafe { GetFileType(f.as_raw_handle()) } != FILE_TYPE_DISK {
            return None;
        }
    }
    f.stream_position().ok()?;
    Some(FileSink::new(f))
}
