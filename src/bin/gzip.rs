//! `gzip` for Skua Bash (also `gunzip` and `zcat`, by the name it runs as): GNU gzip 1.14 as
//! Git Bash ships it, for the options agents use: `-c -d -k -f -l -t -v -q -r -n -N -S -1..-9`,
//! stdin to stdout, `.gz` naming, gzip's messages and exit codes (1 error, 2 warning). Never
//! waits for a prompt unless stdin is a terminal. The deflate output is flate2's, so compressed
//! bytes and sizes differ from GNU gzip's; everything else is its output.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "../gzip_fmt.rs"]
mod gzip_fmt;
#[path = "../ziptime.rs"]
mod ziptime;

use gzip_fmt::{Counting, DecodeError, Header, compress, decode_body, ratio, read_header};

const KNOWN_SUFFIXES: [&str; 7] = [".gz", ".z", ".taz", ".tgz", "-gz", "-z", "_z"];

struct Gz {
    o: Opts,
    exit: u8,
    total: (u64, u64, u64),
    listed: u32,
    printed_header: bool,
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

fn strip_suffix(name: &str, suffix: &str) -> Option<String> {
    let all = std::iter::once(suffix).chain(KNOWN_SUFFIXES);
    for s in all.filter(|s| !s.is_empty()) {
        if name.len() > s.len()
            && name.is_char_boundary(name.len() - s.len())
            && name[name.len() - s.len()..].eq_ignore_ascii_case(s)
        {
            let stem = &name[..name.len() - s.len()];
            return Some(
                if s.eq_ignore_ascii_case(".tgz") || s.eq_ignore_ascii_case(".taz") {
                    format!("{stem}.tar")
                } else {
                    stem.to_owned()
                },
            );
        }
    }
    None
}

fn known_suffix(name: &str, suffix: &str) -> Option<String> {
    std::iter::once(suffix)
        .chain(KNOWN_SUFFIXES)
        .filter(|s| !s.is_empty())
        .find_map(|s| {
            (name.len() > s.len()
                && name.is_char_boundary(name.len() - s.len())
                && name[name.len() - s.len()..].eq_ignore_ascii_case(s))
            .then(|| name[name.len() - s.len()..].to_owned())
        })
}

impl Gz {
    fn warn(&mut self, msg: &str) {
        if !self.o.quiet {
            eprintln!("gzip: {msg}");
        }
        if self.exit == 0 {
            self.exit = 2;
        }
    }

    fn error(&mut self, msg: &str) {
        eprintln!("gzip: {msg}");
        self.exit = 1;
    }

    fn decode_error(&mut self, name: &str, e: &DecodeError) {
        eprintln!("\ngzip: {name}: {}", e.text());
        self.exit = 1;
    }

    /// Compresses or decompresses one operand (`-` is stdin).
    fn operand(&mut self, name: &str) {
        if name == "-" {
            return self.stdin();
        }
        let mut path = PathBuf::from(name);
        let mut shown = name.to_owned();
        let mut meta = fs::metadata(&path);
        let decoding = self.o.decompress || self.o.test || self.o.list;
        if decoding
            && known_suffix(name, &self.o.suffix).is_none()
            && matches!(&meta, Err(e) if e.kind() == io::ErrorKind::NotFound)
        {
            shown = format!("{name}.gz");
            path = PathBuf::from(&shown);
            meta = fs::metadata(&path);
        }
        let meta = match meta {
            Ok(m) => m,
            Err(e) => return self.error(&format!("{shown}: {}", skua_tools::gnu_error(&e))),
        };
        if meta.is_dir() {
            if self.o.recursive {
                return self.directory(&path);
            }
            return self.warn(&format!("{shown} is a directory -- ignored"));
        }
        if self.o.list {
            self.list_file(&shown, &path, &meta);
        } else if self.o.decompress || self.o.test {
            self.decompress_file(&shown, &path, &meta);
        } else {
            self.compress_file(&shown, &path, &meta);
        }
    }

    fn directory(&mut self, dir: &Path) {
        let mut entries: Vec<PathBuf> = match fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(e) => {
                return self.error(&format!("{}: {}", dir.display(), skua_tools::gnu_error(&e)));
            }
        };
        entries.sort();
        for p in entries {
            let shown = p.to_string_lossy().replace('\\', "/");
            self.operand(&shown);
        }
    }

    fn stdin(&mut self) {
        let force = self.o.force;
        if self.o.decompress || self.o.test || self.o.list {
            if io::stdin().is_terminal() && !force {
                eprintln!(
                    "gzip: compressed data not read from a terminal. Use -f to force decompression.\nFor help, type: gzip -h"
                );
                self.exit = 1;
                return;
            }
            let mut r = Counting::new(io::stdin().lock());
            return self.decode_stream("stdin", &mut r, None);
        }
        if io::stdout().is_terminal() && !force {
            eprintln!(
                "gzip: compressed data not written to a terminal. Use -f to force compression.\nFor help, type: gzip -h"
            );
            self.exit = 1;
            return;
        }
        let mut out = BufWriter::new(io::stdout().lock());
        let res = compress(&mut io::stdin().lock(), &mut out, None, 0, self.o.level)
            .and_then(|r| out.flush().map(|()| r));
        match res {
            Ok((i, o)) => {
                if self.o.verbose > 0 {
                    eprintln!(
                        "stdin:\t{} -- created stdout",
                        ratio(i as i64 - (o as i64 - 18), i as i64)
                    );
                }
            }
            Err(e) => self.io_failure("stdout", &e),
        }
    }

    fn io_failure(&mut self, name: &str, e: &io::Error) {
        if e.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(141);
        }
        self.error(&format!("{name}: {}", skua_tools::gnu_error(e)));
    }

    fn compress_file(&mut self, shown: &str, path: &Path, meta: &fs::Metadata) {
        let suffix = if self.o.suffix.is_empty() {
            ".gz"
        } else {
            &self.o.suffix
        }
        .to_owned();
        if !self.o.stdout
            && let Some(s) = known_suffix(shown, &suffix)
        {
            if !self.o.quiet {
                eprintln!("gzip: {shown} already has {s} suffix -- unchanged");
            }
            return;
        }
        let ofname = format!("{shown}{suffix}");
        let mut input = match File::open(path) {
            Ok(f) => f,
            Err(e) => return self.error(&format!("{shown}: {}", skua_tools::gnu_error(&e))),
        };
        let save = self.o.name != Some(false);
        let mtime = if save {
            meta.modified()
                .map_or(0, unix_secs)
                .clamp(0, i64::from(u32::MAX)) as u32
        } else {
            0
        };
        let base = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let name = base.filter(|_| save).map(String::into_bytes);
        let level = self.o.level;
        if self.o.stdout {
            let mut out = BufWriter::new(io::stdout().lock());
            let res = compress(&mut input, &mut out, name.as_deref(), mtime, level)
                .and_then(|r| out.flush().map(|()| r));
            return match res {
                Ok((i, o)) => {
                    let hdr = 18 + name.as_ref().map_or(0, |n| n.len() as u64 + 1);
                    self.verbose_line(shown, "created stdout", i, o, hdr);
                }
                Err(e) => self.io_failure(shown, &e),
            };
        }
        let Some(file) = self.open_output(&ofname) else {
            return;
        };
        let mut out = BufWriter::new(file);
        let res = compress(&mut input, &mut out, name.as_deref(), mtime, level)
            .and_then(|r| out.flush().map(|()| r));
        drop(out);
        match res {
            Ok((i, o)) => {
                set_mtime(&ofname, meta.modified().ok());
                ziptime::copy_readonly(meta, Path::new(&ofname));
                let hdr = 10 + name.as_ref().map_or(0, |n| n.len() as u64 + 1) + 8;
                self.finish(shown, path, &ofname, (i, o, hdr));
            }
            Err(e) => {
                let _ = fs::remove_file(&ofname);
                self.io_failure(&ofname, &e);
            }
        }
    }

    /// Creates the output file; a refusal prints gzip's message and returns `None`.
    fn open_output(&mut self, ofname: &str) -> Option<File> {
        let mut oo = OpenOptions::new();
        oo.write(true);
        if self.o.force {
            let _ = ziptime::force_remove(Path::new(ofname));
            oo.create(true).truncate(true);
        } else {
            oo.create_new(true);
        }
        match oo.open(ofname) {
            Ok(f) => Some(f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if io::stdin().is_terminal() && !self.o.quiet {
                    eprint!("gzip: {ofname} already exists; do you wish to overwrite (y or n)? ");
                    let mut line = String::new();
                    let _ = io::stdin().lock().read_line(&mut line);
                    if line.trim_start().starts_with(['y', 'Y']) {
                        return OpenOptions::new()
                            .write(true)
                            .truncate(true)
                            .open(ofname)
                            .ok();
                    }
                    eprintln!("\tnot overwritten");
                    if self.exit == 0 {
                        self.exit = 2;
                    }
                } else {
                    self.warn(&format!("{ofname} already exists;\tnot overwritten"));
                }
                None
            }
            Err(e) => {
                self.error(&format!("{ofname}: {}", skua_tools::gnu_error(&e)));
                None
            }
        }
    }

    /// `-v` line for one file; `i` and `o` are bytes read and written, `hdr` the header and
    /// trailer bytes of the compressed side.
    fn verbose_line(&self, shown: &str, tail: &str, i: u64, o: u64, hdr: u64) {
        if self.o.verbose > 0 {
            let (num, den) = if self.o.decompress {
                (o as i64 - (i as i64 - hdr as i64), o as i64)
            } else {
                (i as i64 - (o as i64 - hdr as i64), i as i64)
            };
            eprintln!("{shown}:	{} -- {tail}", ratio(num, den));
        }
    }

    /// After a successful file to file run: drop the input unless `-k`, then the `-v` line.
    fn finish(&mut self, shown: &str, ipath: &Path, ofname: &str, sizes: (u64, u64, u64)) {
        if !self.o.keep
            && let Err(e) = ziptime::force_remove(ipath)
        {
            self.error(&format!("{shown}: {}", skua_tools::gnu_error(&e)));
        }
        let how = if self.o.keep {
            "created"
        } else {
            "replaced with"
        };
        self.verbose_line(shown, &format!("{how} {ofname}"), sizes.0, sizes.1, sizes.2);
    }
}

fn set_mtime(path: &str, t: Option<SystemTime>) {
    if let (Some(t), Ok(f)) = (t, OpenOptions::new().write(true).open(path)) {
        let _ = f.set_modified(t);
    }
}

#[path = "../gzip_decode.rs"]
mod decode;
#[path = "../gzip_opts.rs"]
mod opts;
use opts::{Opts, Parsed, parse};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    let stem = |a: &str| {
        Path::new(a)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().to_lowercase())
    };
    let mut name = stem(args.first().map_or("", String::as_str));
    // Multicall form (`gzip.exe gunzip ARGS`), for a bundle without `bin\` links.
    if matches!(
        args.get(1).map(String::as_str),
        Some("gunzip" | "zcat" | "uncompress")
    ) && name == "gzip"
    {
        name = args[1].clone();
        args.drain(..2);
    } else {
        args.drain(..1.min(args.len()));
    }
    let mut o = Opts {
        level: 6,
        ..Opts::default()
    };
    match name.as_str() {
        "gunzip" | "uncompress" => o.decompress = true,
        "zcat" => {
            o.decompress = true;
            o.stdout = true;
        }
        _ => {}
    }
    let mut o = match parse(args, o) {
        Parsed::Run(o) => o,
        Parsed::Exit(c) => return c,
    };
    if o.names.is_empty() {
        o.names.push("-".to_owned());
    }
    let names = std::mem::take(&mut o.names);
    let mut gz = Gz {
        o,
        exit: 0,
        total: (0, 0, 0),
        listed: 0,
        printed_header: false,
    };
    for n in &names {
        gz.operand(n);
    }
    gz.finish_list(names.len());
    let _ = io::stdout().flush();
    ExitCode::from(gz.exit)
}
