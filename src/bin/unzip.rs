//! `unzip` for Skua Bash: Info-ZIP UnZip 6.00 as Git Bash ships it, for what agents use:
//! `unzip a.zip`, `-d DIR`, `-o -n -q -qq -j -p -l -v -t -z -f -u -x -C -L -s`, member
//! patterns (Info-ZIP wildcards: `*` stops at `/`, `**` does not), `-Z` / `-Z1`. Never waits on
//! a prompt unless stdin is a terminal or a pipe that has data. Stored and deflated entries
//! only; encrypted and exotic-method entries are reported and skipped (exit 81).

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

#[path = "../unzip_extract.rs"]
mod extract;
#[path = "../unzip_info.rs"]
mod info;
#[path = "../zipfmt.rs"]
mod zipfmt;
#[path = "../ziptime.rs"]
mod ziptime;

use zipfmt::{Archive, Entry, ZipError, glob_match, read_archive};

const USAGE: &str = "UnZip 6.00 of 20 April 2009, by Info-ZIP.  Maintained by C. Spieler.  Send
bug reports using http://www.info-zip.org/zip-bug.html; see README for details.

Usage: unzip [-Z] [-opts[modifiers]] file[.zip] [list] [-x xlist] [-d exdir]
  Default action is to extract files in list, except those in xlist, to exdir;
  file[.zip] may be a wildcard.  -Z => ZipInfo mode (\"unzip -Z\" for usage).

  -p  extract files to pipe, no messages     -l  list files (short format)
  -f  freshen existing files, create none    -t  test compressed archive data
  -u  update files, create if necessary      -z  display archive comment only
  -v  list verbosely/show version info       -T  timestamp archive to latest
  -x  exclude files that follow (in xlist)   -d  extract files into exdir
modifiers:
  -n  never overwrite existing files         -q  quiet mode (-qq => quieter)
  -o  overwrite files WITHOUT prompting      -a  auto-convert any text files
  -j  junk paths (do not make directories)   -aa treat ALL files as text
  -U  use escapes for all non-ASCII Unicode  -UU ignore any Unicode fields
  -C  match filenames case-insensitively     -L  make (some) names lowercase
  -X  restore UID/GID info                   -V  retain VMS version numbers
  -K  keep setuid/setgid/tacky permissions   -M  pipe through \"more\" pager
See \"unzip -hh\" or unzip.txt for more help.  Examples:
  unzip data1 -x joe   => extract all files except joe from zipfile data1.zip
  unzip -p foo | more  => send contents of foo.zip via pipe into program more
  unzip -fo foo ReadMe => quietly replace existing ReadMe if archive file newer
";

#[derive(Default)]
pub struct Opts {
    pub zipfile: String,
    pub patterns: Vec<String>,
    pub excludes: Vec<String>,
    pub exdir: Option<String>,
    pub list: bool,
    pub verbose: bool,
    pub test: bool,
    pub pipe: bool,
    pub comment: bool,
    pub freshen: bool,
    pub update: bool,
    pub overwrite: bool,
    pub never: bool,
    pub junk: bool,
    pub ci: bool,
    pub lower: bool,
    pub quiet: u8,
    /// `Some(one_per_line)` in ZipInfo mode.
    pub zipinfo: Option<bool>,
}

impl Opts {
    pub fn is_match(&self, name: &str) -> bool {
        let hit = |p: &String| p == name || glob_match(p, name, true, self.ci);
        (self.patterns.is_empty() || self.patterns.iter().any(hit))
            && !self.excludes.iter().any(hit)
    }
}

pub fn usage(code: u8, to_stdout: bool) -> ExitCode {
    let _ = if to_stdout {
        io::stdout().write_all(USAGE.as_bytes())
    } else {
        io::stderr().write_all(USAGE.as_bytes())
    };
    ExitCode::from(code)
}

fn parse(args: Vec<String>) -> Result<Opts, ExitCode> {
    let mut o = Opts::default();
    let mut it = args.into_iter().peekable();
    let mut excluding = false;
    let mut first = true;
    while let Some(a) = it.next() {
        let is_opt = a.len() > 1 && a.starts_with('-');
        if !is_opt {
            if o.zipfile.is_empty() {
                o.zipfile = a;
                excluding = false;
            } else if excluding {
                o.excludes.push(a);
            } else {
                o.patterns.push(a);
            }
            first = false;
            continue;
        }
        let flags = &a[1..];
        if let Some(zi) = flags.strip_prefix('Z') {
            if !first {
                eprintln!(
                    "\nerror:  -Z must be first option for ZipInfo mode (check UNZIP variable?)"
                );
                return Err(usage(10, false));
            }
            o.zipinfo = Some(zi.contains('1'));
            first = false;
            continue;
        }
        first = false;
        let chars = flags.char_indices();
        for (i, c) in chars {
            match c {
                'l' => o.list = true,
                'v' => o.verbose = true,
                't' => o.test = true,
                'p' => o.pipe = true,
                'z' => o.comment = true,
                'f' => o.freshen = true,
                'u' => o.update = true,
                'o' => o.overwrite = true,
                'n' => o.never = true,
                'j' => o.junk = true,
                'C' => o.ci = true,
                'L' => o.lower = true,
                'q' => o.quiet = o.quiet.saturating_add(1),
                'x' => excluding = true,
                'd' => {
                    let rest = &flags[i + 1..];
                    o.exdir = Some(if rest.is_empty() {
                        it.next().unwrap_or_default()
                    } else {
                        rest.to_owned()
                    });
                    break;
                }
                // Accepted and ignored: text conversion, Unicode escapes, ownership, pager.
                'a' | 'b' | 'c' | 'B' | 'D' | 'F' | 'K' | 'M' | 'T' | 'U' | 'V' | 'W' | 'X' => {}
                'h' => return Err(usage(0, true)),
                _ => return Err(usage(10, false)),
            }
        }
    }
    if o.zipfile.is_empty() {
        return Err(usage(if o.zipinfo.is_some() { 10 } else { 0 }, true));
    }
    Ok(o)
}

/// Opens the archive as Info-ZIP does: the name, then `.zip`, then `.ZIP`.
fn open(o: &Opts) -> Result<(File, String), ExitCode> {
    for cand in [
        o.zipfile.clone(),
        format!("{}.zip", o.zipfile),
        format!("{}.ZIP", o.zipfile),
    ] {
        if Path::new(&cand).is_file()
            && let Ok(f) = File::open(&cand)
        {
            return Ok((f, cand));
        }
    }
    let z = &o.zipfile;
    eprintln!("unzip:  cannot find or open {z}, {z}.zip or {z}.ZIP.");
    Err(ExitCode::from(9))
}

fn corrupt(name: &str, e: &ZipError) -> ExitCode {
    let z = name;
    match e {
        ZipError::NotZip => {
            if !name.is_empty() {
                eprintln!(
                    "  End-of-central-directory signature not found.  Either this file is not\n  a zipfile, or it constitutes one disk of a multi-part archive.  In the\n  latter case the central directory and zipfile comment will be found on\n  the last disk(s) of this archive."
                );
                eprintln!(
                    "unzip:  cannot find zipfile directory in one of {z} or\n        {z}.zip, and cannot find {z}.ZIP, period."
                );
            }
            ExitCode::from(9)
        }
        ZipError::Corrupt(m) => {
            eprintln!("error [{z}]:  {m}");
            ExitCode::from(3)
        }
        ZipError::Io(e) => {
            eprintln!("unzip:  cannot read {z}: {}", skua_tools::gnu_error(e));
            ExitCode::from(3)
        }
    }
}

fn main() -> ExitCode {
    skua_tools::quiet_exit_on_stdout_failure(|| {});
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "unzip")
        && std::env::args()
            .next()
            .is_some_and(|a| !a.to_lowercase().contains("unzip"))
    {
        args.remove(0); // multicall form: `skua-tools.exe unzip ARGS`
    }
    let o = match parse(args) {
        Ok(o) => o,
        Err(c) => return c,
    };
    let (mut f, name) = match open(&o) {
        Ok(v) => v,
        Err(c) => return c,
    };
    let arch: Archive = match read_archive(&mut f) {
        Ok(a) => a,
        Err(e) => {
            if o.quiet == 0 && !o.pipe {
                println!("Archive:  {name}");
            }
            return corrupt(&o.zipfile, &e);
        }
    };
    let code = if o.zipinfo.is_some() {
        info::zipinfo(&o, &name, &arch)
    } else if o.comment {
        println!("Archive:  {name}");
        print_comment(&arch);
        0
    } else if o.list || o.verbose {
        info::list(&o, &name, &arch)
    } else {
        extract::run(&o, &name, &arch, &mut f)
    };
    let _ = io::stdout().flush();
    ExitCode::from(code)
}

pub fn print_comment(arch: &Archive) {
    if !arch.comment.is_empty() {
        let mut out = io::stdout();
        let _ = out.write_all(&arch.comment);
        let _ = out.write_all(
            b"
",
        );
    }
}

/// Entries selected by the patterns.
pub fn selected<'a>(o: &'a Opts, arch: &'a Archive) -> impl Iterator<Item = &'a Entry> {
    arch.entries.iter().filter(|e| o.is_match(&e.name))
}

/// Patterns (without wildcards or with) that matched nothing, for the `caution` lines.
pub fn unmatched(o: &Opts, arch: &Archive) -> Vec<String> {
    o.patterns
        .iter()
        .filter(|p| {
            !arch
                .entries
                .iter()
                .any(|e| e.name == **p || glob_match(p, &e.name, true, o.ci))
        })
        .cloned()
        .collect()
}
