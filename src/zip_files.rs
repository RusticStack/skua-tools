//! `zip`: which files go in (arguments, wildcards, recursion, `-x`/`-i`), how their entries are
//! named, and writing one of them (a child module of `bin/zip.rs`).

use super::*;
use std::time::SystemTime;
use zipfmt::{Entry, has_wildcards};
use zipwrite::New;

#[derive(Clone)]
pub struct Cand {
    pub disk: PathBuf,
    /// The entry name; directories end with `/`.
    pub name: String,
    pub dir: bool,
    /// A staged copy of stdin, deleted once it is written.
    pub temp: bool,
}

/// The stored name of an operand: forward slashes, no `./`, no drive, no leading `/`.
fn stored_name(raw: &str, junk: bool) -> String {
    let mut s = raw.replace('\\', "/");
    let b = s.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        s.drain(..2);
    }
    let mut s = s.trim_start_matches('/');
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.trim_start_matches('/');
    }
    let s = if s == "." { "" } else { s };
    if junk {
        s.rsplit('/').next().unwrap_or("").to_owned()
    } else {
        s.to_owned()
    }
}

fn allowed(o: &Opts, name: &str) -> bool {
    let bare = name.trim_end_matches('/');
    let hit = |p: &String| glob_match(p, bare, false, false) || glob_match(p, name, false, false);
    (o.includes.is_empty() || o.includes.iter().any(hit)) && !o.excludes.iter().any(hit)
}

/// Case-insensitive, sorted expansion of wildcards in a file-system path.
fn expand(pat: &str) -> Vec<PathBuf> {
    let mut cur: Vec<PathBuf> = vec![if pat.starts_with(['/', '\\']) {
        PathBuf::from("/")
    } else {
        PathBuf::new()
    }];
    for comp in pat.split(['/', '\\']).filter(|c| !c.is_empty()) {
        let mut next = Vec::new();
        for base in &cur {
            if has_wildcards(comp) {
                let dir = if base.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    base.as_path()
                };
                let Ok(rd) = fs::read_dir(dir) else { continue };
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .collect();
                names.sort_by_key(|n| n.to_uppercase());
                next.extend(
                    names
                        .into_iter()
                        .filter(|n| glob_match(comp, n, true, true))
                        .map(|n| base.join(n)),
                );
            } else {
                let p = base.join(comp);
                if p.exists() {
                    next.push(p);
                }
            }
        }
        cur = next;
    }
    cur.retain(|p| !p.as_os_str().is_empty());
    cur
}

fn add_path(
    o: &Opts,
    skip: Option<&Path>,
    path: &Path,
    raw: &str,
    top: bool,
    out: &mut Vec<Cand>,
    rc: &mut u8,
) {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            if top {
                warn(&format!("name not matched: {raw}"));
            } else {
                warn(&format!(
                    "could not open for reading: {raw} ({})",
                    skua_tools::gnu_error(&e)
                ));
                *rc = 18;
            }
            return;
        }
    };
    if skip.is_some_and(|s| fs::canonicalize(path).is_ok_and(|p| p == s)) {
        return;
    }
    let name = stored_name(raw, o.junk && !meta.is_dir());
    if meta.is_dir() {
        let dirname = name.trim_end_matches('/').to_owned();
        if !dirname.is_empty() && !o.no_dirs && !o.junk && allowed(o, &format!("{dirname}/")) {
            out.push(Cand {
                disk: path.to_path_buf(),
                name: format!("{dirname}/"),
                dir: true,
                temp: false,
            });
        }
        if o.recurse {
            let Ok(rd) = fs::read_dir(path) else { return };
            let mut kids: Vec<String> = rd
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect();
            kids.sort_by_key(|n| n.to_uppercase());
            for k in kids {
                let child_raw = if raw.ends_with(['/', '\\']) {
                    format!("{raw}{k}")
                } else {
                    format!("{raw}/{k}")
                };
                add_path(o, skip, &path.join(&k), &child_raw, false, out, rc);
            }
        }
    } else if !name.is_empty() && allowed(o, &name) && !out.iter().any(|c| c.name == name) {
        out.push(Cand {
            disk: path.to_path_buf(),
            name,
            dir: false,
            temp: false,
        });
    }
}

/// The files named by the operands (and `-@` lines).
pub fn collect(o: &Opts, skip: Option<&Path>, rc: &mut u8) -> Vec<Cand> {
    let mut out = Vec::new();
    for arg in &o.names {
        if arg == "-" {
            let tmp = std::env::temp_dir().join(format!("zistdin{}", std::process::id()));
            let staged =
                File::create(&tmp).and_then(|mut f| io::copy(&mut io::stdin().lock(), &mut f));
            match staged {
                Ok(_) => out.push(Cand {
                    disk: tmp,
                    name: "-".to_owned(),
                    dir: false,
                    temp: true,
                }),
                Err(e) => warn(&format!(
                    "could not read stdin: {}",
                    skua_tools::gnu_error(&e)
                )),
            }
            continue;
        }
        let literal = Path::new(arg).exists();
        let paths = if has_wildcards(arg) && !literal {
            expand(arg)
        } else {
            vec![PathBuf::from(arg)]
        };
        if paths.is_empty() {
            warn(&format!("name not matched: {arg}"));
            continue;
        }
        for p in paths {
            let raw = p.to_string_lossy().replace('\\', "/");
            add_path(o, skip, &p, &raw, true, &mut out, rc);
        }
    }
    out
}

fn mtime_secs(c: &Cand) -> i64 {
    fs::metadata(&c.disk)
        .and_then(|m| m.modified())
        .map_or_else(|_| unix_secs(SystemTime::now()), unix_secs)
}

/// Whether the file is newer than the archive entry (for `-u` and `-f`).
pub fn is_newer(c: &Cand, e: &Entry) -> bool {
    let secs = mtime_secs(c);
    match e.mtime_utc {
        Some(t) => secs > t,
        None => {
            let (d, t) = ziptime::dos_from_civil(&ziptime::local_from_unix(secs));
            (d, t) > (e.dos_date, e.dos_time)
        }
    }
}

fn exec_name(name: &str) -> bool {
    let l = name.to_lowercase();
    [".exe", ".com", ".bat", ".cmd", ".sh"]
        .iter()
        .any(|x| l.ends_with(x))
}

/// Adds one candidate to the archive; `None` when the file vanished (a warning was printed).
pub fn write(out: &mut Out, c: &Cand, level: u32) -> io::Result<Option<New>> {
    let meta = match fs::metadata(&c.disk) {
        Ok(m) => m,
        Err(e) => {
            warn(&format!(
                "could not open for reading: {} ({})",
                c.name,
                skua_tools::gnu_error(&e)
            ));
            return Ok(None);
        }
    };
    let secs = meta
        .modified()
        .map_or_else(|_| unix_secs(SystemTime::now()), unix_secs);
    let (dos_date, dos_time) = ziptime::dos_from_civil(&ziptime::local_from_unix(secs));
    let readonly = meta.permissions().readonly();
    let ext_attr = if c.dir {
        (0o040_755u32 << 16) | 0x10
    } else if readonly {
        (0o100_444u32 << 16) | 1
    } else if exec_name(&c.name) {
        0o100_755u32 << 16
    } else {
        0o100_644u32 << 16
    };
    let n = New {
        name: c.name.clone(),
        deflated: !c.dir && level > 0,
        level,
        dos_time,
        dos_date,
        mtime_utc: secs,
        crc: 0,
        csize: 0,
        usize: 0,
        offset: 0,
        text: false,
        ext_attr,
    };
    if c.dir {
        return out.add(n, None, &mut || unreachable_reopen()).map(Some);
    }
    let mut f = match File::open(&c.disk) {
        Ok(f) => f,
        Err(e) => {
            warn(&format!(
                "could not open for reading: {} ({})",
                c.name,
                skua_tools::gnu_error(&e)
            ));
            return Ok(None);
        }
    };
    let disk = c.disk.clone();
    let res = out
        .add(n, Some(&mut f), &mut || File::open(&disk))
        .map(Some);
    drop(f);
    if c.temp {
        let _ = fs::remove_file(&c.disk);
    }
    res
}

fn unreachable_reopen() -> io::Result<File> {
    Err(io::Error::other("no data"))
}

/// `-m`: removes the files that went in, then directories that became empty.
pub fn remove_moved(written: &[Cand]) {
    for c in written.iter().filter(|c| !c.dir) {
        let _ = ziptime::force_remove(&c.disk);
    }
    for c in written.iter().rev().filter(|c| c.dir) {
        let _ = fs::remove_dir(&c.disk);
    }
}
