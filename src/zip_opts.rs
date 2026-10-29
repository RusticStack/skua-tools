//! Command line of `zip`: Info-ZIP's option letters (bundled, anywhere), the `-x` / `-i` lists,
//! the help text. A child module of `bin/zip.rs`.

use super::*;

pub const HELP: &str = "Copyright (c) 1990-2008 Info-ZIP - Type 'zip \"-L\"' for software license.
Zip 3.0 (July 5th 2008). Usage:
zip [-options] [-b path] [-t mmddyyyy] [-n suffixes] [zipfile list] [-xi list]
  The default action is to add or replace zipfile entries from list, which
  can include the special name - to compress standard input.
  If zipfile and list are omitted, zip compresses stdin to stdout.
  -f   freshen: only changed files  -u   update: only changed or new files
  -d   delete entries in zipfile    -m   move into zipfile (delete OS files)
  -r   recurse into directories     -j   junk (don't record) directory names
  -0   store only                   -l   convert LF to CR LF (-ll CR LF to LF)
  -1   compress faster              -9   compress better
  -q   quiet operation              -v   verbose operation/print version info
  -c   add one-line comments        -z   add zipfile comment
  -@   read names from stdin        -o   make zipfile as old as latest entry
  -x   exclude the following names  -i   include only the following names
  -F   fix zipfile (-FF try harder) -D   do not add directory entries
  -A   adjust self-extracting exe   -J   junk zipfile prefix (unzipsfx)
  -T   test zipfile integrity       -X   eXclude eXtra file attributes
  -y   store symbolic links as the link instead of the referenced file
  -e   encrypt (not supported)      -n   don't compress these suffixes
  -h2  show more help
";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Add,
    Update,
    Freshen,
    Delete,
}

pub struct Opts {
    pub recurse: bool,
    pub junk: bool,
    pub quiet: bool,
    pub level: u32,
    pub mode: Mode,
    pub moving: bool,
    pub no_dirs: bool,
    pub stdin_list: bool,
    pub test: bool,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub zipfile: String,
    pub names: Vec<String>,
}

pub fn invalid(what: &str) -> ExitCode {
    eprintln!("\nzip error: Invalid command arguments ({what})");
    ExitCode::from(16)
}

pub fn parse(args: Vec<String>) -> Result<Opts, ExitCode> {
    let mut o = Opts {
        recurse: false,
        junk: false,
        quiet: false,
        level: 6,
        mode: Mode::Add,
        moving: false,
        no_dirs: false,
        stdin_list: false,
        test: false,
        excludes: vec![],
        includes: vec![],
        zipfile: String::new(),
        names: vec![],
    };
    // 0 = plain operands, 1 = after -x, 2 = after -i.
    let mut list = 0u8;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "-" || !a.starts_with('-') {
            match list {
                1 => o.excludes.push(a),
                2 => o.includes.push(a),
                _ if o.zipfile.is_empty() => o.zipfile = a,
                _ => o.names.push(a),
            }
            continue;
        }
        list = 0;
        if a == "--" {
            continue;
        }
        if a == "--help" || a == "-h" || a == "-h2" {
            print!("{HELP}");
            return Err(ExitCode::SUCCESS);
        }
        if a == "--version" || a == "-v" {
            println!(
                "Copyright (c) 1990-2008 Info-ZIP - Type 'zip \"-L\"' for software license.\nThis is Zip 3.0 (July 5th 2008), by Info-ZIP."
            );
            return Err(ExitCode::SUCCESS);
        }
        for c in a[1..].chars() {
            match c {
                'r' => o.recurse = true,
                'j' => o.junk = true,
                'q' => o.quiet = true,
                'm' => o.moving = true,
                'D' => o.no_dirs = true,
                'T' => o.test = true,
                'u' => o.mode = Mode::Update,
                'f' => o.mode = Mode::Freshen,
                'd' => o.mode = Mode::Delete,
                '@' => o.stdin_list = true,
                '0'..='9' => o.level = c.to_digit(10).unwrap_or(6),
                'x' => list = 1,
                'i' => list = 2,
                // Accepted, no effect here: no extra attributes written beyond the timestamp,
                // symlinks are followed, no text conversion, no verbose listing.
                'X' | 'y' | 'S' | 'g' | 'k' | 'l' | 'v' | 'V' | 'w' | 'o' | 'A' | 'J' => {}
                'e' => return Err(no_encryption()),
                'P' => return Err(no_encryption()),
                'b' | 't' | 'n' | 'O' | 'F' => {
                    let _ = it.next();
                    break;
                }
                c => return Err(invalid(&format!("no such option: {c}"))),
            }
        }
    }
    Ok(o)
}

/// `-e` and `-P`: writing an unencrypted archive after being asked to encrypt would be a silent
/// security failure, so refuse and write nothing.
fn no_encryption() -> ExitCode {
    invalid("encryption (-e, -P) is not supported by Skua Bash's zip; no archive was written")
}
