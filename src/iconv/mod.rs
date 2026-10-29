//! iconv: convert text between character sets, as GNU libiconv 1.19's `iconv` (the one in Git
//! for Windows): the same options (`-f -t -c -s -l`, `--unicode-subst` and friends,
//! `//TRANSLIT`, `//IGNORE`), the same `file:line:column: cannot convert` messages and exit 1.
//!
//! Differences: the character sets are the Unicode forms, ASCII, ISO-8859-x, the Windows code
//! pages, KOI8, Mac Roman and the common CJK sets (via `encoding_rs`, so a few unusual
//! mappings follow WHATWG); `//TRANSLIT` has libiconv's table for Latin, punctuation, symbols
//! and emoji but not its locale-specific entries; `-l` lists only what is supported.

mod args;
mod charset;
mod convert;
mod decode;
mod subst;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use args::parse_args;
use charset::{Encoder, Kind, entries, lookup};
use convert::Conv;
use skua_tools::gnu_error;
use subst::Format;

const HELP: &str = include_str!("../data/iconv_help.txt");
const VERSION: &str = include_str!("../data/iconv_version.txt");
const USAGE: &str = "Usage: iconv [-c] [-s] [-f fromcode] [-t tocode] [file ...]\nor:    iconv -l\nTry 'iconv --help' for more information.\n";

pub fn main_exit() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut err = io::stderr().lock();
    let env = |k: &str| std::env::var(k).ok();
    let code = run(&args, &mut io::stdin().lock(), &mut out, &mut err, &env);
    let _ = out.flush();
    ExitCode::from(code as u8)
}

/// The character set of the environment's locale (`LC_ALL`, `LC_CTYPE`, `LANG`).
fn locale_kind(env: &dyn Fn(&str) -> Option<String>) -> Kind {
    let loc = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|k| env(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default();
    let cs = loc
        .split_once('.')
        .map(|(_, c)| c.split('@').next().unwrap_or(c).to_owned());
    match cs {
        Some(c) => lookup(&c)
            .or_else(|| {
                if c.eq_ignore_ascii_case("utf8") {
                    lookup("UTF-8")
                } else {
                    None
                }
            })
            .unwrap_or(Kind::Ascii),
        None => Kind::Ascii,
    }
}

struct Spec {
    kind: Kind,
    translit: bool,
    ignore: bool,
}

/// Parses `NAME[//SUFFIX...]`; empty NAME is the locale's set.
fn spec(code: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Spec> {
    let mut parts = code.split("//");
    let base = parts.next().unwrap_or("");
    let (mut translit, mut ignore) = (false, false);
    for p in parts {
        match p.to_ascii_uppercase().as_str() {
            "TRANSLIT" => translit = true,
            "IGNORE" | "NON_IDENTICAL_DISCARD" => ignore = true,
            _ => return None,
        }
    }
    let kind = if base.is_empty() && !code.contains("//") {
        locale_kind(env)
    } else {
        lookup(base)?
    };
    Some(Spec {
        kind,
        translit,
        ignore,
    })
}

pub fn run(
    args: &[String],
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
    env: &dyn Fn(&str) -> Option<String>,
) -> i32 {
    let Ok(o) = parse_args(args) else {
        let _ = err.write_all(USAGE.as_bytes());
        return 1;
    };
    if o.version {
        let _ = out.write_all(VERSION.as_bytes());
        return 0;
    }
    if o.help {
        let _ = out.write_all(HELP.as_bytes());
        return 0;
    }
    if o.list {
        for e in entries() {
            let _ = writeln!(out, "{}", e.names);
        }
        return 0;
    }
    let from_name = o.from.clone().unwrap_or_default();
    let to_name = o.to.clone().unwrap_or_default();
    let (Some(from), to) = (spec(&from_name, env), spec(&to_name, env)) else {
        return unsupported(err, "from", &from_name);
    };
    let Some(to) = to else {
        return unsupported(err, "to", &to_name);
    };
    let mut fmts = [None, None];
    for (slot, (name, text)) in fmts.iter_mut().zip([
        ("unicode-subst", &o.unicode_subst),
        ("byte-subst", &o.byte_subst),
    ]) {
        if let Some(t) = text {
            match Format::parse(t) {
                Ok(f) => *slot = Some(f),
                Err(m) => {
                    let _ = writeln!(err, "iconv: --{name} argument: {m}");
                    return 1;
                }
            }
        }
    }
    if let Some(t) = &o.widechar_subst
        && let Err(m) = Format::parse(t)
    {
        let _ = writeln!(err, "iconv: --widechar-subst argument: {m}");
        return 1;
    }
    let [unicode_subst, byte_subst] = fmts;
    let mut enc = Encoder::new(to.kind);
    enc.strict_bmp = unicode_subst.is_some();
    let mut conv = Conv {
        enc,
        translit: to.translit,
        ignore: to.ignore,
        discard: o.discard,
        silent: o.silent,
        unicode_subst,
        byte_subst,
        omitted: false,
    };
    let mut rc = 0;
    let mut files = o.files.clone();
    if files.is_empty() {
        files.push("-".to_owned());
    }
    for f in &files {
        let (name, mut data) = (if f == "-" { "(stdin)" } else { f.as_str() }, Vec::new());
        let read = if f == "-" {
            stdin.read_to_end(&mut data).map(|_| ())
        } else {
            File::open(f)
                .and_then(|mut h| h.read_to_end(&mut data))
                .map(|_| ())
        };
        if let Err(e) = read {
            let _ = out.flush();
            let _ = writeln!(err, "iconv: {f}: {}", gnu_error(&e));
            rc = 1;
            continue;
        }
        if !conv.convert(name, &data, from.kind, out, err) {
            return 1;
        }
    }
    if conv.omitted {
        rc = 1;
    }
    rc
}

fn unsupported(err: &mut dyn Write, dir: &str, name: &str) -> i32 {
    let _ = writeln!(err, "iconv: conversion {dir} {name} unsupported");
    let _ = writeln!(
        err,
        "iconv: try 'iconv -l' to get the list of supported encodings"
    );
    1
}
