//! `file` for Skua Bash: names the type of each file the way Git Bash's `file` (libmagic 5.4x)
//! does for the files coding agents ask about: text (encoding, line terminators, long lines,
//! escape sequences), scripts by `#!`, a few structured text formats, common binary formats,
//! `empty`, `directory`, symbolic links. Anything it does not recognise is `data`.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use skua_tools::text::{Encoding, TextInfo, classify_text};

/// How many bytes are examined, as libmagic's default `bytes` limit.
const READ_LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Description,
    MimeType,
    MimeEncoding,
    Mime,
}

struct Options {
    brief: bool,
    follow: bool,
    mode: Mode,
    separator: String,
    no_pad: bool,
    files: Vec<String>,
    names_file: Option<String>,
}

/// Usage errors as Git Bash's file prints them: the usage block on stderr, no `Try` line,
/// exit 1 (M3-SH11). The block lists the options this `file` has.
fn usage() -> ExitCode {
    eprintln!(
        "Usage: file [-bhiLN] [--mime-type] [--mime-encoding] [-F <separator>] [-f <namefile>]"
    );
    eprintln!("            <file> ...");
    eprintln!("       file [--help]");
    ExitCode::from(1)
}

fn parse_args() -> Result<Options, ExitCode> {
    let mut o = Options {
        brief: false,
        follow: false,
        mode: Mode::Description,
        separator: ":".to_owned(),
        no_pad: false,
        files: Vec::new(),
        names_file: None,
    };
    let mut args = std::env::args().skip(1);
    let mut only_files = false;
    while let Some(arg) = args.next() {
        if only_files || arg == "-" || !arg.starts_with('-') {
            o.files.push(arg);
            continue;
        }
        match arg.as_str() {
            "--" => only_files = true,
            "--brief" => o.brief = true,
            "--dereference" => o.follow = true,
            "--no-dereference" => o.follow = false,
            "--mime" => o.mode = Mode::Mime,
            "--mime-type" => o.mode = Mode::MimeType,
            "--mime-encoding" => o.mode = Mode::MimeEncoding,
            "--no-pad" => o.no_pad = true,
            "--version" | "-v" => {
                println!("file-5.48 (Skua Bash)");
                return Err(ExitCode::SUCCESS);
            }
            "--help" => {
                println!("Usage: file [OPTION...] [FILE...]");
                println!("Determine type of FILEs.");
                println!("  -b, --brief            do not prepend filenames to output lines");
                println!("  -L, --dereference      follow symlinks");
                println!("  -h, --no-dereference   don't follow symlinks (default)");
                println!("  -i, --mime             output MIME type strings");
                println!("      --mime-type        output the MIME type");
                println!("      --mime-encoding    output the MIME encoding");
                println!("  -F, --separator STRING use string as separator instead of `:'");
                println!("  -f, --files-from FILE  read the filenames to be examined from FILE");
                println!("  -N, --no-pad           do not pad output");
                return Err(ExitCode::SUCCESS);
            }
            "--separator" => o.separator = args.next().ok_or_else(usage)?,
            "--files-from" => o.names_file = Some(args.next().ok_or_else(usage)?),
            s if s.starts_with("--") => {
                eprintln!("file: unknown option -- {}", &s[2..]);
                return Err(usage());
            }
            s => {
                let flags: Vec<char> = s[1..].chars().collect();
                for (i, c) in flags.iter().enumerate() {
                    match c {
                        'b' => o.brief = true,
                        'L' => o.follow = true,
                        'h' => o.follow = false,
                        'i' => o.mode = Mode::Mime,
                        'N' => o.no_pad = true,
                        'z' | 'k' | 'r' | 's' | 'p' | 'n' => {}
                        'F' | 'f' => {
                            let rest: String = flags[i + 1..].iter().collect();
                            let value = if rest.is_empty() {
                                args.next().ok_or_else(usage)?
                            } else {
                                rest
                            };
                            if *c == 'F' {
                                o.separator = value;
                            } else {
                                o.names_file = Some(value);
                            }
                            break;
                        }
                        _ => {
                            eprintln!("file: unknown option -- {c}");
                            return Err(usage());
                        }
                    }
                }
            }
        }
    }
    Ok(o)
}

fn main() -> ExitCode {
    let mut o = match parse_args() {
        Ok(o) => o,
        Err(code) => return code,
    };
    if let Some(list) = &o.names_file {
        let text = if list == "-" {
            let mut s = String::new();
            let _ = io::stdin().read_to_string(&mut s);
            s
        } else {
            match fs::read_to_string(list) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("file: Cannot open `{list}' ({})", skua_tools::gnu_error(&e));
                    return ExitCode::from(1);
                }
            }
        };
        o.files
            .extend(text.lines().filter(|l| !l.is_empty()).map(str::to_owned));
    }
    if o.files.is_empty() {
        return usage();
    }
    let shown: Vec<String> = o
        .files
        .iter()
        .map(|f| {
            if f == "-" {
                "/dev/stdin".to_owned()
            } else {
                f.clone()
            }
        })
        .collect();
    let width = shown.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut out = io::stdout().lock();
    for (file, name) in o.files.iter().zip(&shown) {
        let desc = describe(file, &o);
        let line = if o.brief {
            desc
        } else if o.no_pad {
            format!("{name}{} {desc}", o.separator)
        } else {
            let pad = width - name.chars().count();
            format!("{name}{}{} {desc}", o.separator, " ".repeat(pad))
        };
        if writeln!(out, "{line}").is_err() {
            return ExitCode::from(141);
        }
    }
    ExitCode::SUCCESS
}

fn describe(file: &str, o: &Options) -> String {
    let data = if file == "-" {
        let mut buf = Vec::new();
        let _ = io::stdin().lock().take(READ_LIMIT).read_to_end(&mut buf);
        buf
    } else {
        let path = Path::new(file);
        let meta = if o.follow {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        };
        let meta = match meta {
            Ok(m) => m,
            Err(e) => {
                return format!("cannot open `{file}' ({})", skua_tools::gnu_error(&e));
            }
        };
        if meta.file_type().is_symlink() {
            return match fs::read_link(path) {
                Ok(target) => match o.mode {
                    Mode::Description => format!("symbolic link to {}", target.display()),
                    Mode::MimeType => "inode/symlink".to_owned(),
                    Mode::MimeEncoding => "binary".to_owned(),
                    Mode::Mime => "inode/symlink; charset=binary".to_owned(),
                },
                Err(e) => format!("cannot open `{file}' ({})", skua_tools::gnu_error(&e)),
            };
        }
        if meta.is_dir() {
            return mime_or(o.mode, "directory", "inode/directory", "binary");
        }
        let mut buf = Vec::new();
        match fs::File::open(path) {
            Ok(f) => {
                if let Err(e) = f.take(READ_LIMIT).read_to_end(&mut buf) {
                    return format!("cannot open `{file}' ({})", skua_tools::gnu_error(&e));
                }
            }
            Err(e) => return format!("cannot open `{file}' ({})", skua_tools::gnu_error(&e)),
        }
        buf
    };
    classify(&data, o.mode)
}

fn mime_or(mode: Mode, desc: &str, mime: &str, charset: &str) -> String {
    match mode {
        Mode::Description => desc.to_owned(),
        Mode::MimeType => mime.to_owned(),
        Mode::MimeEncoding => charset.to_owned(),
        Mode::Mime => format!("{mime}; charset={charset}"),
    }
}

fn classify(data: &[u8], mode: Mode) -> String {
    if data.is_empty() {
        return mime_or(mode, "empty", "inode/x-empty", "binary");
    }
    if let Some((desc, mime)) = binary_magic(data) {
        return mime_or(mode, &desc, mime, "binary");
    }
    let Some(text) = classify_text(data) else {
        return mime_or(mode, "data", "application/octet-stream", "binary");
    };
    let charset = match text.encoding {
        Encoding::Ascii => "us-ascii",
        Encoding::Utf8 | Encoding::Utf8Bom => "utf-8",
        Encoding::Iso8859 => "iso-8859-1",
        Encoding::ExtendedAscii => "unknown-8bit",
    };
    let (desc, mime) = text_description(data, &text);
    mime_or(mode, &desc, mime, charset)
}

/// "ASCII text", "Unicode text, UTF-8 text", ... and the qualifiers libmagic appends.
fn text_description(data: &[u8], t: &TextInfo) -> (String, &'static str) {
    let enc = match t.encoding {
        Encoding::Ascii => "ASCII",
        Encoding::Utf8 => "Unicode text, UTF-8",
        Encoding::Utf8Bom => "Unicode text, UTF-8 (with BOM)",
        Encoding::Iso8859 => "ISO-8859",
        Encoding::ExtendedAscii => "Non-ISO extended-ASCII",
    };
    let mut quals = String::new();
    if t.longest_line > 300 {
        quals.push_str(&format!(", with very long lines ({})", t.longest_line));
    }
    let mut terms = Vec::new();
    if t.crlf > 0 {
        terms.push("CRLF");
    }
    if t.cr > 0 {
        terms.push("CR");
    }
    if t.lf > 0 && !terms.is_empty() {
        terms.push("LF");
    }
    if !terms.is_empty() {
        quals.push_str(&format!(", with {} line terminators", terms.join(", ")));
    } else if t.lf == 0 {
        quals.push_str(", with no line terminators");
    }
    if t.escapes {
        quals.push_str(", with escape sequences");
    }
    if t.backspaces {
        quals.push_str(", with overstriking");
    }

    let body = if t.encoding == Encoding::Utf8Bom {
        &data[3..]
    } else {
        data
    };
    if let Some(interp) = shebang(body) {
        let name = Path::new(&interp).file_name().map_or(interp.clone(), |n| {
            n.to_string_lossy().trim_end_matches(".exe").to_owned()
        });
        let name = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        let text = format!("{enc} text executable{quals}");
        let (desc, mime) = match name {
            "sh" | "dash" | "ash" => (format!("POSIX shell script, {text}"), "text/x-shellscript"),
            "bash" => (
                format!("Bourne-Again shell script, {text}"),
                "text/x-shellscript",
            ),
            "zsh" => (
                format!("Paul Falstad's zsh script, {text}"),
                "text/x-shellscript",
            ),
            "python" => (format!("Python script, {text}"), "text/x-script.python"),
            "node" => (
                format!("Node.js script executable, {enc} text{quals}"),
                "application/javascript",
            ),
            "perl" => (format!("Perl script text executable{quals}"), "text/x-perl"),
            "ruby" => (format!("Ruby script, {text}"), "text/x-ruby"),
            "awk" | "gawk" => (format!("awk or perl script, {text}"), "text/x-awk"),
            _ => (format!("a {interp} script, {text}"), "text/plain"),
        };
        return (desc, mime);
    }
    let plain = format!("{enc} text{quals}");
    let trimmed = body.trim_ascii_start();
    if trimmed.starts_with(b"<?xml version=\"1.0\"") || trimmed.starts_with(b"<?xml version='1.0'")
    {
        return (format!("XML 1.0 document, {plain}"), "text/xml");
    }
    let lower: Vec<u8> = trimmed
        .iter()
        .take(64)
        .map(u8::to_ascii_lowercase)
        .collect();
    if lower.starts_with(b"<!doctype html") || lower.starts_with(b"<html") {
        return (format!("HTML document, {plain}"), "text/html");
    }
    if (trimmed.starts_with(b"{") || trimmed.starts_with(b"["))
        && skua_tools::json::is_json(trimmed)
    {
        return ("JSON text data".to_owned(), "application/json");
    }
    (plain, "text/plain")
}

/// The interpreter named by a `#!` line: the program, or the one `env` runs.
fn shebang(data: &[u8]) -> Option<String> {
    let rest = data.strip_prefix(b"#!")?;
    let line = rest.split(|&b| b == b'\n').next()?;
    let line = String::from_utf8_lossy(line)
        .trim()
        .trim_end_matches('\r')
        .to_owned();
    let mut words = line.split_whitespace();
    let prog = words.next()?.to_owned();
    if prog.ends_with("/env") || prog == "env" {
        return words.find(|w| !w.starts_with('-')).map(str::to_owned);
    }
    Some(prog)
}

fn u16le(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32le(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn u32be(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn binary_magic(d: &[u8]) -> Option<(String, &'static str)> {
    if d.starts_with(b"MZ") {
        if let Some(pe) = pe_description(d) {
            return Some((pe, "application/vnd.microsoft.portable-executable"));
        }
        return Some(("MS-DOS executable".to_owned(), "application/x-dosexec"));
    }
    if d.starts_with(b"\x7fELF") {
        let class = if d.get(4) == Some(&2) { "64" } else { "32" };
        let order = if d.get(5) == Some(&2) { "MSB" } else { "LSB" };
        let kind = match u16le(d, 16) {
            Some(2) => "executable",
            Some(3) => "pie executable",
            Some(1) => "relocatable",
            Some(4) => "core file",
            _ => "file",
        };
        return Some((
            format!("ELF {class}-bit {order} {kind}"),
            "application/x-executable",
        ));
    }
    if d.starts_with(b"\x89PNG\r\n\x1a\n") && d.len() >= 26 && &d[12..16] == b"IHDR" {
        let (w, h) = (u32be(d, 16)?, u32be(d, 20)?);
        let color = match d[25] {
            0 => "grayscale",
            2 => "RGB",
            3 => "colormap",
            4 => "gray+alpha",
            6 => "RGBA",
            _ => "unknown",
        };
        let inter = if d.get(28) == Some(&1) {
            "interlaced"
        } else {
            "non-interlaced"
        };
        return Some((
            format!(
                "PNG image data, {w} x {h}, {}-bit/color {color}, {inter}",
                d[24]
            ),
            "image/png",
        ));
    }
    if d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a") {
        let v = String::from_utf8_lossy(&d[3..6]).into_owned();
        let (w, h) = (u16le(d, 6)?, u16le(d, 8)?);
        return Some((
            format!("GIF image data, version {v}, {w} x {h}"),
            "image/gif",
        ));
    }
    if d.starts_with(b"\xff\xd8\xff") {
        return Some(("JPEG image data".to_owned(), "image/jpeg"));
    }
    if let Some(rest) = d.strip_prefix(b"%PDF-") {
        let v: String = rest
            .iter()
            .take_while(|b| b.is_ascii_digit() || **b == b'.')
            .map(|&b| b as char)
            .collect();
        return Some((format!("PDF document, version {v}"), "application/pdf"));
    }
    if d.starts_with(b"\x1f\x8b") {
        return Some(("gzip compressed data".to_owned(), "application/gzip"));
    }
    if d.starts_with(b"PK\x03\x04") && d.len() >= 30 {
        return Some(("Zip archive data".to_owned(), "application/zip"));
    }
    if d.starts_with(b"7z\xbc\xaf\x27\x1c") {
        return Some((
            "7-zip archive data".to_owned(),
            "application/x-7z-compressed",
        ));
    }
    if d.starts_with(b"BZh") && d.len() > 4 && d[3].is_ascii_digit() {
        return Some(("bzip2 compressed data".to_owned(), "application/x-bzip2"));
    }
    if d.starts_with(b"\xfd7zXZ\x00") {
        return Some(("XZ compressed data".to_owned(), "application/x-xz"));
    }
    if d.starts_with(b"\x28\xb5\x2f\xfd") {
        return Some(("Zstandard compressed data".to_owned(), "application/zstd"));
    }
    if d.len() > 262 && &d[257..262] == b"ustar" {
        return Some(("POSIX tar archive".to_owned(), "application/x-tar"));
    }
    if d.starts_with(b"SQLite format 3\x00") {
        return Some(("SQLite 3.x database".to_owned(), "application/vnd.sqlite3"));
    }
    if d.starts_with(b"\x00asm") {
        return Some((
            "WebAssembly (wasm) binary module".to_owned(),
            "application/wasm",
        ));
    }
    None
}

/// "PE32+ executable for MS Windows 10.00 (console), x86-64, 8 sections", as file 5.4x says.
fn pe_description(d: &[u8]) -> Option<String> {
    let pe = u32le(d, 0x3c)? as usize;
    if d.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = u16le(d, pe + 4)?;
    let sections = u16le(d, pe + 6)?;
    let characteristics = u16le(d, pe + 22)?;
    let opt = pe + 24;
    let magic = u16le(d, opt)?;
    let kind = if magic == 0x20b { "PE32+" } else { "PE32" };
    // MajorSubsystemVersion / MinorSubsystemVersion, what file 5.4x prints.
    let os_major = u16le(d, opt + 48)?;
    let os_minor = u16le(d, opt + 50)?;
    let stripped = if characteristics & 0x0200 != 0 {
        " (stripped to external PDB)"
    } else {
        ""
    };
    let subsystem = u16le(d, opt + 68)?;
    let sub = if characteristics & 0x2000 != 0 {
        "DLL"
    } else {
        match subsystem {
            2 => "GUI",
            3 => "console",
            1 => "native",
            10 => "EFI application",
            _ => "Unknown subsystem",
        }
    };
    let arch = match machine {
        0x8664 => "x86-64",
        0x14c => "Intel i386",
        0xaa64 => "Aarch64",
        0x1c4 => "ARMv7 Thumb",
        _ => "Unknown processor",
    };
    Some(format!(
        "{kind} executable for MS Windows {os_major}.{os_minor:02} ({sub}), {arch}{stripped}, {sections} sections"
    ))
}

#[cfg(test)]
mod tests {
    use super::{Mode, classify};

    fn desc(d: &[u8]) -> String {
        classify(d, Mode::Description)
    }

    #[test]
    fn matches_git_bash_file() {
        // Each expectation is Git Bash's file-5.48 output for the same bytes.
        assert_eq!(desc(b""), "empty");
        assert_eq!(desc(b"a\nb\n"), "ASCII text");
        assert_eq!(
            desc(b"a\r\nb\r\n"),
            "ASCII text, with CRLF line terminators"
        );
        assert_eq!(desc(b"a\rb\r"), "ASCII text, with CR line terminators");
        assert_eq!(
            desc(b"a\r\nb\n"),
            "ASCII text, with CRLF, LF line terminators"
        );
        assert_eq!(desc(b"abc"), "ASCII text, with no line terminators");
        assert_eq!(desc("h\u{e9}\n".as_bytes()), "Unicode text, UTF-8 text");
        assert_eq!(
            desc(b"\xef\xbb\xbfhi\n"),
            "Unicode text, UTF-8 (with BOM) text"
        );
        assert_eq!(desc(b"h\xe9\n"), "ISO-8859 text");
        assert_eq!(
            desc(b"#!/bin/sh\necho\n"),
            "POSIX shell script, ASCII text executable"
        );
        assert_eq!(
            desc(b"#!/usr/bin/env bash\necho\n"),
            "Bourne-Again shell script, ASCII text executable"
        );
        assert_eq!(
            desc(b"#!/usr/bin/env python3\nprint(1)\n"),
            "Python script, ASCII text executable"
        );
        assert_eq!(
            desc(b"#!/usr/bin/env node\n1\n"),
            "Node.js script executable, ASCII text"
        );
        assert_eq!(desc(b"#!/usr/bin/perl\n1\n"), "Perl script text executable");
        assert_eq!(desc(b"{\"a\": 1}\n"), "JSON text data");
        assert_eq!(
            desc(b"<?xml version=\"1.0\"?>\n<a/>\n"),
            "XML 1.0 document, ASCII text"
        );
        assert_eq!(
            desc(b"<!DOCTYPE html>\n<html></html>\n"),
            "HTML document, ASCII text"
        );
        assert_eq!(desc(b"\x00\x01\x02"), "data");
        assert_eq!(
            desc(b"a\nb\n\x1b[31mred\n"),
            "ASCII text, with escape sequences"
        );
        assert_eq!(desc(b"%PDF-1.4\n"), "PDF document, version 1.4");
        let mut long = vec![b'a'; 3000];
        long.push(b'\n');
        assert_eq!(desc(&long), "ASCII text, with very long lines (3000)");
    }

    #[test]
    fn this_test_binary_is_a_pe_or_elf() {
        let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let d = desc(&exe);
        assert!(d.starts_with("PE32") || d.starts_with("ELF"), "{d}");
    }
}
