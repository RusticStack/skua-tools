//! Option parsing for `gzip`: GNU getopt-style short and long options (unique prefixes of long
//! names), the `--help` and `--version` texts. A child module of `bin/gzip.rs`.

use super::*;

pub const HELP: &str = "Usage: gzip [OPTION]... [FILE]...
Compress or uncompress FILEs (by default, compress FILES in-place).

Mandatory arguments to long options are mandatory for short options too.

  -a, --ascii       ascii text; convert end-of-line using local conventions
  -c, --stdout      write on standard output, keep original files unchanged
  -d, --decompress  decompress
  -f, --force       force overwrite of output file and compress links
  -h, --help        give this help
  -k, --keep        keep (don't delete) input files
  -l, --list        list compressed file contents
  -L, --license     display software license
  -n, --no-name     do not save or restore the original name and timestamp
  -N, --name        save or restore the original name and timestamp
  -q, --quiet       suppress all warnings
  -r, --recursive   operate recursively on directories
      --rsyncable   make rsync-friendly archive
  -S, --suffix=SUF  use suffix SUF on compressed files
      --synchronous synchronous output (safer if system crashes, but slower)
  -t, --test        test compressed file integrity
  -v, --verbose     verbose mode
  -V, --version     display version number
  -1, --fast        compress faster
  -9, --best        compress better

With no FILE, or when FILE is -, read standard input.

Report bugs to <bug-gzip@gnu.org>.
";

pub const VERSION: &str = "gzip 1.14
Copyright (C) 2025 Free Software Foundation, Inc.
Copyright (C) 1993 Jean-loup Gailly.
This is free software.  You may redistribute copies of it under the terms of
the GNU General Public License <https://www.gnu.org/licenses/gpl.html>.
There is NO WARRANTY, to the extent permitted by law.

Written by Jean-loup Gailly.
";

/// Suffixes gzip treats as "already compressed" (compared without case, as on Windows).
const LONG: [(&str, char); 24] = [
    ("ascii", 'a'),
    ("to-stdout", 'c'),
    ("stdout", 'c'),
    ("decompress", 'd'),
    ("uncompress", 'd'),
    ("force", 'f'),
    ("help", 'h'),
    ("keep", 'k'),
    ("list", 'l'),
    ("license", 'L'),
    ("no-name", 'n'),
    ("name", 'N'),
    ("quiet", 'q'),
    ("silent", 'q'),
    ("recursive", 'r'),
    ("rsyncable", 'R'),
    ("suffix", 'S'),
    ("synchronous", 'Y'),
    ("test", 't'),
    ("verbose", 'v'),
    ("version", 'V'),
    ("fast", '1'),
    ("best", '9'),
    ("bits", 'b'),
];

#[derive(Default)]
pub struct Opts {
    pub stdout: bool,
    pub decompress: bool,
    pub force: bool,
    pub keep: bool,
    pub list: bool,
    pub test: bool,
    pub verbose: u32,
    pub quiet: bool,
    pub recursive: bool,
    /// `Some(true)` for -N, `Some(false)` for -n.
    pub name: Option<bool>,
    pub suffix: String,
    pub level: u32,
    pub names: Vec<String>,
    /// The name the tool runs as (`gzip`, `gunzip`, `zcat`), for `--version`.
    pub prog: String,
}

pub enum Parsed {
    Run(Opts),
    Exit(ExitCode),
}

fn bad_option(what: &str) -> Parsed {
    eprintln!("gzip: unknown option -- {what}\nTry `gzip --help' for more information.");
    Parsed::Exit(ExitCode::from(1))
}

fn missing_arg(what: &str) -> Parsed {
    eprintln!(
        "gzip: {what}
Try `gzip --help' for more information."
    );
    Parsed::Exit(ExitCode::from(1))
}

/// `gzip 1.14 ...`; the other names show as `gunzip (gzip) 1.14 ...` as GNU's do.
fn version_text(prog: &str) -> String {
    match prog {
        "gunzip" | "zcat" | "uncompress" => {
            VERSION.replacen("gzip 1.14", &format!("{prog} (gzip) 1.14"), 1)
        }
        _ => VERSION.to_owned(),
    }
}

pub fn parse(args: Vec<String>, mut o: Opts) -> Parsed {
    let mut it = args.into_iter();
    let mut only_files = false;
    let mut flags: Vec<(char, Option<String>)> = Vec::new();
    while let Some(a) = it.next() {
        if only_files || a == "-" || !a.starts_with('-') {
            o.names.push(a);
        } else if a == "--" {
            only_files = true;
        } else if let Some(long) = a.strip_prefix("--") {
            let (key, val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_owned())),
                None => (long, None),
            };
            let hits: Vec<_> = LONG.iter().filter(|(n, _)| n.starts_with(key)).collect();
            let exact = hits.iter().find(|(n, _)| *n == key);
            let Some(&&(_, c)) = exact.or_else(|| {
                let first = hits.first()?;
                hits.iter().all(|h| h.1 == first.1).then_some(first)
            }) else {
                return bad_option(key);
            };
            let val = if matches!(c, 'S' | 'b') && val.is_none() {
                it.next()
            } else {
                val
            };
            if matches!(c, 'S' | 'b') && val.is_none() {
                return missing_arg(&format!("option '--{key}' requires an argument"));
            }
            flags.push((c, val));
        } else {
            let mut chars = a[1..].chars();
            while let Some(c) = chars.next() {
                if matches!(c, 'S' | 'b') {
                    let rest: String = chars.by_ref().collect();
                    let val = if rest.is_empty() {
                        it.next()
                    } else {
                        Some(rest)
                    };
                    if val.is_none() {
                        return missing_arg(&format!("option requires an argument -- {c}"));
                    }
                    flags.push((c, val));
                } else if "acdfhklLnNqrtvV123456789".contains(c) {
                    flags.push((c, None));
                } else {
                    return bad_option(&c.to_string());
                }
            }
        }
    }
    for (c, val) in flags {
        match c {
            'c' => o.stdout = true,
            'd' => o.decompress = true,
            'f' => o.force = true,
            'k' => o.keep = true,
            'l' => o.list = true,
            't' => o.test = true,
            'v' => o.verbose += 1,
            'q' => o.quiet = true,
            'r' => o.recursive = true,
            'n' => o.name = Some(false),
            'N' => o.name = Some(true),
            '1'..='9' => o.level = c.to_digit(10).unwrap_or(6),
            'S' => o.suffix = val.unwrap_or_default(),
            'b' => {
                if val.as_deref().is_none_or(|v| v.parse::<u32>().is_err()) {
                    eprintln!(
                        "gzip: -b operand is not an integer\nTry `gzip --help' for more information."
                    );
                    return Parsed::Exit(ExitCode::from(1));
                }
            }
            'h' => return Parsed::Exit(out_text(HELP)),
            'V' => return Parsed::Exit(out_text(&version_text(&o.prog))),
            'L' => return Parsed::Exit(out_text(&version_text(&o.prog))),
            _ => {}
        }
    }
    Parsed::Run(o)
}

fn out_text(t: &str) -> ExitCode {
    let _ = io::stdout().write_all(t.as_bytes());
    ExitCode::SUCCESS
}
