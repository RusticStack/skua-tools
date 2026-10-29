//! `zip` for Skua Bash: Info-ZIP Zip 3.0 behaviour for what agents use (Git for Windows ships
//! `unzip` but no `zip`, so there is no Git Bash output to match; this follows Info-ZIP's):
//! `zip [-r -q -j -D -X -m -u -f -9..-0 -T -x LIST -i LIST -@] out.zip files...`, `zip -d out.zip
//! patterns`, update of an existing archive, `  adding: x (deflated 63%)` lines, names of
//! directories with a trailing `/`, `-` for stdin and for stdout. Deflate and store only,
//! no Zip64, no encryption, no comments.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

#[path = "../zip_files.rs"]
mod files;
#[path = "../zip_opts.rs"]
mod opts;
use opts::{HELP, Mode, Opts, parse};
#[path = "../zipfmt.rs"]
mod zipfmt;
#[path = "../ziptime.rs"]
mod ziptime;
#[path = "../zipwrite.rs"]
mod zipwrite;

use zipfmt::{Archive, ZipError, glob_match, read_archive, read_entry};
use zipwrite::Out;

/// `out.zip`, or `out` + `.zip` when `out` does not exist and has no extension.
fn archive_path(name: &str) -> String {
    let p = Path::new(name);
    if name == "-" || p.exists() || p.extension().is_some() {
        name.to_owned()
    } else {
        format!("{name}.zip")
    }
}

pub fn warn(msg: &str) {
    eprintln!("\tzip warning: {msg}");
}

fn nothing_to_do(zipfile: &str) -> ExitCode {
    eprintln!("\nzip error: Nothing to do! ({zipfile})");
    ExitCode::from(12)
}

fn percent(n: u64, m: u64) -> u64 {
    if n > m {
        (200 * (n - m) / n).div_ceil(2)
    } else {
        0
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let mut o = match parse(args) {
        Ok(o) => o,
        Err(c) => return c,
    };
    if o.zipfile.is_empty() {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let zipfile = archive_path(&o.zipfile);
    let to_stdout = zipfile == "-";
    let existing = (!to_stdout && Path::new(&zipfile).is_file())
        .then(|| File::open(&zipfile).ok())
        .flatten();
    let (mut old, old_arch) = match existing {
        Some(mut f) if f.metadata().is_ok_and(|m| m.len() > 0) => match read_archive(&mut f) {
            Ok(a) => (Some(f), a),
            Err(ZipError::NotZip | ZipError::Corrupt(_)) => {
                eprintln!("\nzip error: Zip file structure invalid ({zipfile})");
                return ExitCode::from(3);
            }
            Err(ZipError::Io(e)) => {
                eprintln!(
                    "\nzip error: Could not open input file ({zipfile}): {}",
                    skua_tools::gnu_error(&e)
                );
                return ExitCode::from(18);
            }
        },
        _ => (
            None,
            Archive {
                entries: vec![],
                comment: vec![],
                size: 0,
                cd_offset: 0,
            },
        ),
    };
    if o.stdin_list {
        let mut text = String::new();
        let _ = io::stdin().read_to_string(&mut text);
        o.names
            .extend(text.lines().filter(|l| !l.is_empty()).map(str::to_owned));
    }
    let msg_to_stderr = to_stdout;
    let say = |line: String| {
        if !o.quiet {
            if msg_to_stderr {
                eprintln!("{line}")
            } else {
                println!("{line}")
            }
        }
    };

    let mut deleting: Vec<String> = Vec::new();
    let mut cands: Vec<files::Cand> = Vec::new();
    let mut rc = 0u8;
    if o.mode == Mode::Delete {
        for p in &o.names {
            let hits: Vec<_> = old_arch
                .entries
                .iter()
                .filter(|e| e.name == *p || glob_match(p, &e.name, false, false))
                .collect();
            if hits.is_empty() {
                warn(&format!("name not matched: {p}"));
            }
            deleting.extend(hits.iter().map(|e| e.name.clone()));
        }
        if deleting.is_empty() {
            return nothing_to_do(&zipfile);
        }
    } else {
        let skip = fs::canonicalize(&zipfile).ok();
        cands = files::collect(&o, skip.as_deref(), &mut rc);
    }

    // Decide what happens to each candidate.
    let index: HashMap<&str, usize> = old_arch
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.as_str(), i))
        .collect();
    let mut replaced: HashMap<String, (files::Cand, &'static str)> = HashMap::new();
    let mut added: Vec<(files::Cand, &'static str)> = Vec::new();
    for c in cands {
        match index.get(c.name.as_str()) {
            Some(&i) => {
                let e = &old_arch.entries[i];
                let newer = files::is_newer(&c, e);
                match o.mode {
                    Mode::Add if !c.dir => {
                        replaced.insert(c.name.clone(), (c, "updating"));
                    }
                    Mode::Update if newer && !c.dir => {
                        replaced.insert(c.name.clone(), (c, "updating"));
                    }
                    Mode::Freshen if newer && !c.dir => {
                        replaced.insert(c.name.clone(), (c, "freshening"));
                    }
                    _ => {}
                }
            }
            None if o.mode != Mode::Freshen => added.push((c, "adding")),
            None => {}
        }
    }
    if o.mode != Mode::Delete && replaced.is_empty() && added.is_empty() {
        return nothing_to_do(&zipfile);
    }

    // Write the new archive next to the target, then move it into place.
    let tmp_path: PathBuf = if to_stdout {
        std::env::temp_dir().join(format!("zi{}", std::process::id()))
    } else {
        let dir = Path::new(&zipfile)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        dir.join(format!("zi{}", std::process::id()))
    };
    let tmp = match File::create(&tmp_path) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("\nzip error: Could not create output file ({zipfile})");
            return ExitCode::from(15);
        }
    };
    let mut out = Out::new(tmp);
    let mut failed: Option<io::Error> = None;
    let mut written: Vec<files::Cand> = Vec::new();
    let put = |out: &mut Out, c: &files::Cand, label: &str| -> io::Result<()> {
        match files::write(out, c, o.level) {
            Ok(Some(n)) => {
                let how = if n.deflated { "deflated" } else { "stored" };
                let pct = percent(u64::from(n.usize), u64::from(n.csize));
                say(format!("{label:>8}: {} ({how} {pct}%)", c.name));
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        }
    };
    for e in &old_arch.entries {
        if deleting.contains(&e.name) {
            say(format!("deleting: {}", e.name));
            continue;
        }
        let res = match (replaced.get(&e.name), old.as_mut()) {
            (Some((c, label)), _) => {
                written.push(c.clone());
                put(&mut out, c, label)
            }
            (None, Some(f)) => out.copy_raw(f, e),
            (None, None) => Ok(()),
        };
        if let Err(err) = res {
            failed = Some(err);
            break;
        }
    }
    if failed.is_none() {
        for (c, label) in &added {
            written.push(c.clone());
            if let Err(err) = put(&mut out, c, label) {
                failed = Some(err);
                break;
            }
        }
    }
    let empty = out.central.is_empty();
    let finished = match failed {
        None if !empty => out.finish(&old_arch.comment).map(|_| ()),
        None => Ok(()),
        Some(e) => Err(e),
    };
    if let Err(e) = finished {
        let _ = fs::remove_file(&tmp_path);
        eprintln!(
            "\nzip error: Output file write failure ({zipfile}): {}",
            skua_tools::gnu_error(&e)
        );
        return ExitCode::from(14);
    }
    drop(old);
    if empty {
        let _ = fs::remove_file(&tmp_path);
        if !to_stdout {
            let _ = fs::remove_file(&zipfile);
        }
        warn("zip file empty");
        return ExitCode::SUCCESS;
    }
    if to_stdout {
        let res =
            File::open(&tmp_path).and_then(|mut f| io::copy(&mut f, &mut io::stdout().lock()));
        let _ = fs::remove_file(&tmp_path);
        if res.is_err() {
            return ExitCode::from(14);
        }
    } else if let Err(e) = fs::rename(&tmp_path, &zipfile) {
        let _ = fs::remove_file(&tmp_path);
        eprintln!(
            "\nzip error: Could not replace {zipfile}: {}",
            skua_tools::gnu_error(&e)
        );
        return ExitCode::from(15);
    }
    if o.test && !to_stdout {
        if test_archive(&zipfile) {
            if !o.quiet {
                println!("test of {zipfile} OK");
            }
        } else {
            eprintln!("\nzip error: Zip file structure invalid ({zipfile})");
            return ExitCode::from(8);
        }
    }
    if o.moving {
        files::remove_moved(&written);
    }
    let _ = io::stdout().flush();
    ExitCode::from(rc)
}

fn test_archive(path: &str) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let Ok(arch) = read_archive(&mut f) else {
        return false;
    };
    arch.entries
        .iter()
        .all(|e| e.is_dir() || read_entry(&mut f, e, &mut io::sink()).is_ok())
}

pub fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}
