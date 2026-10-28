//! `gzip -d`, `-t` and `-l`: the decoding half of `bin/gzip.rs` (a child module of it).

use super::*;

/// What a whole input (all members) held.
struct Decoded {
    isize: u64,
    crc: u32,
    members: u32,
    first_header_len: u64,
    consumed: u64,
    garbage: bool,
}

/// Decodes every member after `first` (already read) into `out`.
fn decode_members<R: BufRead, W: Write>(
    r: &mut Counting<R>,
    first: &Header,
    out: &mut W,
) -> Result<Decoded, DecodeError> {
    let mut d = Decoded {
        isize: 0,
        crc: 0,
        members: 0,
        first_header_len: first.len,
        consumed: 0,
        garbage: false,
    };
    loop {
        let m = decode_body(r, out)?;
        d.isize += u64::from(m.size);
        d.crc = m.crc;
        d.members += 1;
        if r.fill_buf()?.is_empty() {
            break;
        }
        if !gzip_fmt::at_magic(r)? {
            d.garbage = true;
            break;
        }
        read_header(r)?;
    }
    d.consumed = r.consumed;
    Ok(d)
}

impl Gz {
    /// Decodes stdin or an opened file for `-d -c`, `-t` or `-l`; `shown` names it in messages.
    pub(super) fn decode_stream<R: BufRead>(
        &mut self,
        shown: &str,
        r: &mut Counting<R>,
        meta: Option<&fs::Metadata>,
    ) {
        let first = match read_header(r) {
            Ok(Some(h)) => h,
            Ok(None) => return self.decode_error(shown, &DecodeError::Truncated),
            Err(e) => return self.decode_error(shown, &e),
        };
        if self.o.list {
            let mut sink = io::sink();
            return match decode_members(r, &first, &mut sink) {
                Ok(d) => self.list_row(shown, &d, meta),
                Err(e) => self.decode_error(shown, &e),
            };
        }
        let res = if self.o.test {
            decode_members(r, &first, &mut io::sink())
        } else {
            let mut out = BufWriter::new(io::stdout().lock());
            let res = decode_members(r, &first, &mut out);
            if let Err(e) = out.flush() {
                self.io_failure("stdout", &e);
            }
            res
        };
        match res {
            Ok(d) => {
                if d.garbage {
                    self.garbage_warning(shown);
                }
                if self.o.test && self.o.verbose > 0 {
                    eprintln!("{shown}:\t OK");
                }
            }
            Err(DecodeError::Io(e)) => self.io_failure("stdout", &e),
            Err(e) => {
                if self.o.test && self.o.verbose > 0 {
                    eprint!("{shown}:\t");
                }
                self.decode_error(shown, &e);
            }
        }
    }

    fn garbage_warning(&mut self, shown: &str) {
        if !self.o.quiet {
            eprintln!("\ngzip: {shown}: decompression OK, trailing garbage ignored");
        }
        if self.exit == 0 {
            self.exit = 2;
        }
    }

    pub(super) fn decompress_file(&mut self, shown: &str, path: &Path, meta: &fs::Metadata) {
        let to_file = !self.o.stdout && !self.o.test;
        if to_file && strip_suffix(shown, &self.o.suffix).is_none() {
            return self.warn(&format!("{shown}: unknown suffix -- ignored"));
        }
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => return self.error(&format!("{shown}: {}", skua_tools::gnu_error(&e))),
        };
        let mut r = Counting::new(BufReader::new(file));
        if !to_file {
            let force_copy = self.o.force && self.o.stdout;
            if force_copy && !gzip_fmt::at_magic(&mut r).unwrap_or(true) {
                let mut out = BufWriter::new(io::stdout().lock());
                let res = io::copy(&mut r, &mut out).and_then(|_| out.flush());
                if let Err(e) = res {
                    self.io_failure("stdout", &e);
                }
                return;
            }
            return self.decode_stream(shown, &mut r, Some(meta));
        }
        let first = match read_header(&mut r) {
            Ok(Some(h)) => h,
            Ok(None) => return self.decode_error(shown, &DecodeError::Truncated),
            Err(e) => return self.decode_error(shown, &e),
        };
        let restore = self.o.name == Some(true);
        let mut ofname = strip_suffix(shown, &self.o.suffix).unwrap_or_default();
        if let (true, Some(stored)) = (restore, first.name.as_ref()) {
            let stored = String::from_utf8_lossy(stored).into_owned();
            let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
            ofname = dir.map_or(stored.clone(), |d| {
                d.join(&stored).to_string_lossy().replace('\\', "/")
            });
        }
        let Some(file) = self.open_output(&ofname) else {
            return;
        };
        let mut out = BufWriter::new(file);
        let res = decode_members(&mut r, &first, &mut out)
            .and_then(|d| out.flush().map(|()| d).map_err(DecodeError::Io));
        drop(out);
        match res {
            Ok(d) => {
                let stamp = if restore && first.mtime != 0 {
                    Some(UNIX_EPOCH + Duration::from_secs(u64::from(first.mtime)))
                } else {
                    meta.modified().ok()
                };
                set_mtime(&ofname, stamp);
                ziptime::copy_readonly(meta, Path::new(&ofname));
                if d.garbage {
                    self.garbage_warning(shown);
                }
                self.finish(
                    shown,
                    path,
                    &ofname,
                    (d.consumed, d.isize, d.first_header_len + 8),
                );
            }
            Err(e) => {
                let _ = fs::remove_file(&ofname);
                match e {
                    DecodeError::Io(e) => self.io_failure(&ofname, &e),
                    e => self.decode_error(shown, &e),
                }
            }
        }
    }

    pub(super) fn list_file(&mut self, shown: &str, path: &Path, meta: &fs::Metadata) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => return self.error(&format!("{shown}: {}", skua_tools::gnu_error(&e))),
        };
        let mut r = Counting::new(BufReader::new(file));
        self.decode_stream(shown, &mut r, Some(meta));
    }

    fn list_row(&mut self, shown: &str, d: &Decoded, meta: Option<&fs::Metadata>) {
        if !self.printed_header && !self.o.quiet {
            let verbose = self.o.verbose > 0;
            let head = if verbose {
                "method  crc     date  time  "
            } else {
                ""
            };
            println!("{head}         compressed        uncompressed  ratio uncompressed_name");
        }
        self.printed_header = true;
        let compressed = meta.map_or(d.consumed, fs::Metadata::len);
        // gzip only discounts the header and trailer of a single-member file.
        let hdr = if d.members > 1 {
            0
        } else {
            d.first_header_len + 8
        };
        let name = if shown == "stdin" {
            "stdout".to_owned()
        } else {
            strip_suffix(shown, &self.o.suffix).unwrap_or_else(|| shown.to_owned())
        };
        if self.o.verbose > 0 {
            let secs = meta.and_then(|m| m.modified().ok()).map_or(0, unix_secs);
            let c = ziptime::local_from_unix(secs);
            print!(
                "{:>5} {:08x} {} {:2} {:02}:{:02} ",
                "defla",
                d.crc,
                ziptime::MONTHS[(c.month as usize).clamp(1, 12) - 1],
                c.day,
                c.hour,
                c.min
            );
        }
        let num = d.isize as i64 - (compressed as i64 - hdr as i64);
        println!(
            "{compressed:>19} {:>19} {} {name}",
            d.isize,
            ratio(num, d.isize as i64)
        );
        self.listed += 1;
        self.total = (self.total.0 + compressed, self.total.1 + d.isize, hdr);
    }

    /// The `(totals)` row of `-l` with several files.
    pub(super) fn finish_list(&mut self, operands: usize) {
        if !self.o.list || self.o.quiet || self.listed < 2 || operands < 2 {
            return;
        }
        let (cin, cout, hdr) = self.total;
        let pad = if self.o.verbose > 0 {
            " ".repeat(28)
        } else {
            String::new()
        };
        let num = cout as i64 - (cin as i64 - hdr as i64);
        println!(
            "{pad}{cin:>19} {cout:>19} {} (totals)",
            ratio(num, cout as i64)
        );
    }
}
