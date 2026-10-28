//! `unzip` extraction, `-t` and `-p` (a child module of `bin/unzip.rs`): Info-ZIP's messages,
//! the overwrite prompt (only when stdin can answer), zip-slip protection.

use super::*;
use std::fs;
use std::io::{BufRead, BufWriter};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use zipfmt::{DataError, read_entry};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overwrite {
    Prompt,
    All,
    None,
}

struct Ctx<'a> {
    o: &'a Opts,
    rc: u8,
    ask: Overwrite,
    exdir: String,
}

impl Ctx<'_> {
    fn bump(&mut self, code: u8) {
        self.rc = self.rc.max(code);
    }
}

/// A path inside the extraction directory, after Info-ZIP's safety rules.
/// `None` when nothing is left to extract.
fn safe_name(c: &mut Ctx, name: &str) -> Option<String> {
    let n = name.replace('\\', "/");
    let trimmed = n.trim_start_matches('/');
    let absolute = trimmed.len() != n.len();
    let n = trimmed.to_owned();
    if absolute {
        eprintln!("warning:  stripped absolute path spec from {name}");
        c.bump(1);
    }
    let dir = n.ends_with('/');
    let mut parts: Vec<String> = Vec::new();
    let mut skipped = false;
    for p in n.split('/') {
        match p {
            "" | "." => {}
            ".." => skipped = true,
            p => parts.push(
                p.chars()
                    .map(|ch| if ":*?\"<>|".contains(ch) { '_' } else { ch })
                    .collect(),
            ),
        }
    }
    if skipped {
        println!("warning:  skipped \"../\" path component(s) in {name}");
        c.bump(1);
    }
    if c.o.junk {
        if dir {
            return None;
        }
        parts = parts.pop().into_iter().collect();
    }
    if parts.is_empty() {
        return None;
    }
    let mut out = parts.join("/");
    if c.o.lower {
        out = out.to_lowercase();
    }
    Some(if dir { out + "/" } else { out })
}

fn entry_unix(e: &Entry) -> i64 {
    e.mtime_utc.unwrap_or_else(|| {
        ziptime::unix_from_local(&ziptime::civil_from_dos(e.dos_date, e.dos_time))
    })
}

fn file_unix(m: &fs::Metadata) -> i64 {
    m.modified()
        .map_or(0, |t| match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(e) => -(e.duration().as_secs() as i64),
        })
}

/// One line from stdin, or `None` at end of file (or when nothing can answer).
fn read_answer() -> Option<String> {
    if !ziptime::stdin_ready(300) {
        return None;
    }
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

enum Choice {
    Yes,
    No,
    Rename(String),
}

/// Info-ZIP's `replace X? [y]es, [n]o, [A]ll, [N]one, [r]ename:` prompt.
fn prompt(c: &mut Ctx, display: &str) -> Choice {
    loop {
        eprint!("replace {display}? [y]es, [n]o, [A]ll, [N]one, [r]ename: ");
        let _ = io::stderr().flush();
        let Some(line) = read_answer() else {
            eprint!(" NULL\n(EOF or read error, treating as \"[N]one\" ...)\n");
            c.ask = Overwrite::None;
            c.bump(1);
            return Choice::No;
        };
        match line.trim_start().chars().next() {
            Some('y' | 'Y') => return Choice::Yes,
            Some('n') => return Choice::No,
            Some('A') => {
                c.ask = Overwrite::All;
                return Choice::Yes;
            }
            Some('N') => {
                c.ask = Overwrite::None;
                return Choice::No;
            }
            Some('r' | 'R') => {
                eprint!("new name: ");
                let _ = io::stderr().flush();
                match read_answer() {
                    Some(n) if !n.trim().is_empty() => return Choice::Rename(n.trim().to_owned()),
                    _ => return Choice::No,
                }
            }
            other => eprintln!("error:  invalid response [{}]", other.unwrap_or('\n')),
        }
    }
}

fn data_error(c: &mut Ctx, e: &Entry, err: DataError) {
    println!();
    match err {
        DataError::Crc(got) => {
            eprintln!(
                "{:<22}   bad CRC {got:08x}  (should be {:08x})",
                e.name, e.crc
            );
            c.bump(2);
        }
        DataError::Inflate => {
            eprintln!("error:  invalid compressed data to inflate");
            c.bump(2);
        }
        DataError::Io(err) => {
            if err.kind() == io::ErrorKind::BrokenPipe {
                std::process::exit(141);
            }
            eprintln!(
                "error:  cannot write {}: {}",
                e.name,
                skua_tools::gnu_error(&err)
            );
            c.bump(50);
        }
        DataError::Truncated | DataError::BadLocalHeader | DataError::Method(_) => {
            eprintln!(
                "error:  unexpected end of file or bad local header for {}",
                e.name
            );
            c.bump(51);
        }
    }
}

pub fn run(o: &Opts, name: &str, arch: &Archive, f: &mut File) -> u8 {
    let mut c = Ctx {
        o,
        rc: 0,
        ask: Overwrite::Prompt,
        exdir: String::new(),
    };
    if o.quiet == 0 && !o.pipe {
        println!("Archive:  {name}");
    }
    if let Some(d) = &o.exdir {
        let path = Path::new(d);
        if !path.is_dir()
            && let Err(e) = fs::create_dir(path)
        {
            eprintln!(
                "checkdir:  cannot create extraction directory: {d}\n           {}",
                skua_tools::gnu_error(&e)
            );
            return 2;
        }
        c.exdir = d.replace('\\', "/");
        if !c.exdir.ends_with('/') {
            c.exdir.push('/');
        }
    }
    for e in selected(o, arch) {
        if e.flags & 1 != 0 {
            eprintln!("   skipping: {:<22}  unable to get password", e.name);
            c.bump(82);
        } else if e.method != 0 && e.method != 8 {
            eprintln!(
                "   skipping: {:<22}  unsupported compression method {}",
                e.name, e.method
            );
            c.bump(81);
        } else if o.test {
            test_entry(&mut c, f, e);
        } else if o.pipe {
            if !e.is_dir() {
                let mut out = BufWriter::new(io::stdout().lock());
                let res =
                    read_entry(f, e, &mut out).and_then(|()| out.flush().map_err(DataError::Io));
                if let Err(err) = res {
                    data_error(&mut c, e, err);
                }
            }
        } else {
            extract_entry(&mut c, f, e);
        }
    }
    let missing = unmatched(o, arch);
    for p in &missing {
        if o.test {
            println!("caution: filename not matched:  {p}");
        } else {
            eprintln!("caution: filename not matched:  {p}");
        }
    }
    if !missing.is_empty() {
        c.bump(11);
    }
    if o.test && o.quiet < 2 {
        if c.rc == 0 {
            println!("No errors detected in compressed data of {name}.");
        } else {
            println!("At least one error was detected in {name}.");
        }
    }
    c.rc
}

fn test_entry(c: &mut Ctx, f: &mut File, e: &Entry) {
    let label = format!("    testing: {:<22}", e.name);
    let res = if e.is_dir() {
        Ok(())
    } else {
        read_entry(f, e, &mut io::sink())
    };
    match res {
        Ok(()) => {
            if c.o.quiet == 0 {
                println!("{label}   OK");
            }
        }
        Err(err) => {
            print!("{label}");
            data_error(c, e, err);
        }
    }
}

fn extract_entry(c: &mut Ctx, f: &mut File, e: &Entry) {
    let Some(rel) = safe_name(c, &e.name) else {
        return;
    };
    let mut display = format!("{}{rel}", c.exdir);
    let mut path = PathBuf::from(&display);
    let quiet = c.o.quiet > 0;
    if e.is_dir() {
        if c.o.freshen {
            return;
        }
        if !path.is_dir() {
            if let Err(err) = fs::create_dir_all(&path) {
                eprintln!(
                    "error:  cannot create {display}\n        {}",
                    skua_tools::gnu_error(&err)
                );
                return c.bump(2);
            }
            if !quiet {
                println!("   creating: {display}");
            }
        }
        return;
    }
    if c.o.freshen && !path.exists() {
        return;
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
        && let Err(err) = fs::create_dir_all(parent)
    {
        eprintln!(
            "error:  cannot create {display}\n        {}",
            skua_tools::gnu_error(&err)
        );
        return c.bump(2);
    }
    if let Ok(m) = fs::metadata(&path) {
        if c.o.freshen || c.o.update {
            if entry_unix(e) <= file_unix(&m) {
                return;
            }
        } else if c.o.never || c.ask == Overwrite::None && !c.o.overwrite {
            return;
        } else if !c.o.overwrite && c.ask != Overwrite::All {
            match prompt(c, &display) {
                Choice::Yes => {}
                Choice::No => return,
                Choice::Rename(n) => {
                    display = n.clone();
                    path = PathBuf::from(n);
                }
            }
        }
    } else if c.o.freshen {
        return;
    }
    let label = if e.method == 8 {
        "  inflating:"
    } else {
        " extracting:"
    };
    if !quiet {
        print!("{label} {display:<22}  ");
        let _ = io::stdout().flush();
    }
    let file = match File::create(&path).or_else(|err| {
        if err.kind() == io::ErrorKind::PermissionDenied {
            let _ = ziptime::force_remove(&path);
            File::create(&path)
        } else {
            Err(err)
        }
    }) {
        Ok(f) => f,
        Err(err) => {
            println!();
            eprintln!(
                "error:  cannot create {display}\n        {}",
                skua_tools::gnu_error(&err)
            );
            return c.bump(2);
        }
    };
    let mut out = BufWriter::new(file);
    let res = read_entry(f, e, &mut out).and_then(|()| out.flush().map_err(DataError::Io));
    let file = out.into_inner().ok();
    if let Err(err) = res {
        drop(file);
        let _ = fs::remove_file(&path);
        return data_error(c, e, err);
    }
    if !quiet {
        println!();
    }
    if let Some(file) = file {
        let secs = entry_unix(e);
        if secs >= 0 {
            let _ = file.set_modified(UNIX_EPOCH + Duration::from_secs(secs as u64));
        }
    }
    let mode = e.unix_mode();
    let readonly = if e.version_made >> 8 == 3 && mode != 0 {
        mode & 0o200 == 0
    } else {
        e.ext_attr & 1 != 0
    };
    if readonly && let Ok(m) = fs::metadata(&path) {
        let mut perms = m.permissions();
        perms.set_readonly(true);
        let _ = fs::set_permissions(&path, perms);
    }
}
