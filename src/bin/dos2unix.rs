//! `dos2unix` and `unix2dos` (one executable; the name it runs as picks the direction) for Skua
//! Bash, with the options and messages of the dos2unix 7.x that Git Bash ships: files converted
//! in place, `-n IN OUT` pairs, stdin to stdout with no file, `-q`, `-k`, `-f`, `-b`/`-r`
//! (BOM), binary files skipped.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    ToUnix,
    ToDos,
}

struct Options {
    dir: Direction,
    quiet: bool,
    keep_date: bool,
    force: bool,
    keep_bom: bool,
    add_bom: bool,
}

/// The direction, from the name the program runs as, and how many leading arguments name it.
/// Skua Bash's bundle runs `dos2unix.exe unix2dos ARGS` when it has no `bin\` hardlinks
/// (multicall form, as `coreutils.exe ls`); with links, argv[0] is `unix2dos.exe`.
fn program() -> (String, Direction, usize) {
    let args: Vec<String> = std::env::args().take(2).collect();
    let stem = |a: &str| {
        Path::new(a)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().to_lowercase())
    };
    let (name, skip) = match args.get(1).map(String::as_str) {
        Some(n @ ("dos2unix" | "unix2dos")) => (n.to_owned(), 2),
        _ => (stem(args.first().map_or("", String::as_str)), 1),
    };
    if name.contains("unix2dos") {
        ("unix2dos".to_owned(), Direction::ToDos, skip)
    } else {
        ("dos2unix".to_owned(), Direction::ToUnix, skip)
    }
}

/// Converts `data`; `None` when it is binary (a NUL or other control byte) and not forced.
fn convert(data: &[u8], o: &Options, prog: &str, name: &str) -> Option<Vec<u8>> {
    if !o.force
        && let Some((line, b)) = binary_symbol(data)
    {
        if !o.quiet {
            eprintln!("{prog}: Binary symbol 0x{b:02X} found at line {line}");
        }
        return None;
    }
    let _ = name;
    let (body, had_bom) = match data.strip_prefix(b"\xef\xbb\xbf") {
        Some(rest) => (rest, true),
        None => (data, false),
    };
    let mut out = Vec::with_capacity(data.len() + data.len() / 16);
    let bom = match o.dir {
        // dos2unix removes a UTF-8 BOM unless -b; unix2dos keeps one, adds one with -m.
        Direction::ToUnix => had_bom && o.keep_bom,
        Direction::ToDos => had_bom || o.add_bom,
    };
    if bom {
        out.extend_from_slice(b"\xef\xbb\xbf");
    }
    let mut i = 0;
    while i < body.len() {
        let b = body[i];
        match (o.dir, b) {
            (Direction::ToUnix, b'\r') if body.get(i + 1) == Some(&b'\n') => {}
            (Direction::ToDos, b'\n') if i == 0 || body[i - 1] != b'\r' => {
                out.extend_from_slice(b"\r\n");
                i += 1;
                continue;
            }
            _ => {}
        }
        if !(o.dir == Direction::ToUnix && b == b'\r' && body.get(i + 1) == Some(&b'\n')) {
            out.push(b);
        }
        i += 1;
    }
    Some(out)
}

/// The first byte dos2unix treats as binary: a control character other than
/// `\t \n \v \f \r` and ESC (BS and BEL too in dos2unix 7.x), with its line number.
fn binary_symbol(data: &[u8]) -> Option<(usize, u8)> {
    let mut line = 1;
    for &b in data {
        if b == b'\n' {
            line += 1;
        }
        if b < 0x20 && !matches!(b, 7..=13 | 0x1b) {
            return Some((line, b));
        }
    }
    None
}

fn convert_file(input: &str, output: &str, o: &Options, prog: &str) -> u8 {
    let meta = match fs::metadata(input) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{prog}: {input}: {}", skua_tools::gnu_error(&e));
            eprintln!("{prog}: Skipping {input}, not a regular file.");
            return 2;
        }
    };
    if !meta.is_file() {
        eprintln!("{prog}: Skipping {input}, not a regular file.");
        return 0;
    }
    let data = match fs::read(input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{prog}: {input}: {}", skua_tools::gnu_error(&e));
            return 2;
        }
    };
    let Some(out) = convert(&data, o, prog, input) else {
        if !o.quiet {
            eprintln!("{prog}: Skipping binary file {input}");
        }
        return 0;
    };
    let format = if o.dir == Direction::ToUnix {
        "Unix"
    } else {
        "DOS"
    };
    if !o.quiet {
        if input == output {
            eprintln!("{prog}: converting file {input} to {format} format...");
        } else {
            eprintln!("{prog}: converting file {input} to file {output} in {format} format...");
        }
    }
    if let Err(e) = fs::write(output, &out) {
        eprintln!("{prog}: {output}: {}", skua_tools::gnu_error(&e));
        return 2;
    }
    if o.keep_date
        && let Ok(modified) = meta.modified()
        && let Ok(f) = fs::File::options().write(true).open(output)
    {
        let _ = f.set_modified(modified);
    }
    0
}

fn main() -> ExitCode {
    let (prog, dir, skip) = program();
    let mut o = Options {
        dir,
        quiet: false,
        keep_date: false,
        force: false,
        keep_bom: false,
        add_bom: false,
    };
    let mut files: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut args = std::env::args().skip(skip).peekable();
    let mut new_file_mode = false;
    let mut only_files = false;
    while let Some(arg) = args.next() {
        if !only_files && arg.starts_with('-') && arg.len() > 1 {
            match arg.as_str() {
                "--" => only_files = true,
                "-q" | "--quiet" => o.quiet = true,
                "-k" | "--keepdate" => o.keep_date = true,
                "-f" | "--force" => o.force = true,
                "-b" | "--keep-bom" => o.keep_bom = true,
                "-r" | "--remove-bom" => o.keep_bom = false,
                "-m" | "--add-bom" => o.add_bom = true,
                "-o" | "--oldfile" => new_file_mode = false,
                "-n" | "--newfile" => new_file_mode = true,
                "-s" | "--safe" | "-a" | "--ascii" | "-u" | "--keep-utf16" | "-v" | "--verbose" => {
                }
                "-V" | "--version" => {
                    println!("{prog} 7.5.6 (Skua Bash)");
                    return ExitCode::SUCCESS;
                }
                "-h" | "--help" => {
                    println!("Usage: {prog} [options] [file ...] [-n infile outfile ...]");
                    println!(" -f, --force      force conversion of binary files");
                    println!(" -k, --keepdate   keep output file date");
                    println!(" -n, --newfile    write to new file");
                    println!(" -o, --oldfile    write to old file (default)");
                    println!(" -q, --quiet      quiet mode, suppress all warnings");
                    return ExitCode::SUCCESS;
                }
                other => {
                    eprintln!("{prog}: option '{other}' unknown.");
                    return ExitCode::from(1);
                }
            }
            continue;
        }
        if new_file_mode {
            let Some(out) = args.next() else {
                eprintln!("{prog}: target of file {arg} not specified in new-file mode");
                return ExitCode::from(1);
            };
            pairs.push((arg, out));
        } else {
            files.push(arg);
        }
    }
    if files.is_empty() && pairs.is_empty() {
        let mut data = Vec::new();
        if let Err(e) = io::stdin().lock().read_to_end(&mut data) {
            eprintln!("{prog}: {}", skua_tools::gnu_error(&e));
            return ExitCode::from(2);
        }
        let Some(out) = convert(&data, &o, &prog, "stdin") else {
            if !o.quiet {
                eprintln!("{prog}: Skipping binary file stdin");
            }
            return ExitCode::SUCCESS;
        };
        return match io::stdout().lock().write_all(&out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(141),
        };
    }
    let mut status = 0u8;
    for f in &files {
        status = status.max(convert_file(f, f, &o, &prog));
    }
    for (i, out) in &pairs {
        status = status.max(convert_file(i, out, &o, &prog));
    }
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::{Direction, Options, convert};

    fn opts(dir: Direction) -> Options {
        Options {
            dir,
            quiet: true,
            keep_date: false,
            force: false,
            keep_bom: false,
            add_bom: false,
        }
    }

    #[test]
    fn converts_like_git_bash() {
        let u = opts(Direction::ToUnix);
        let d = opts(Direction::ToDos);
        assert_eq!(
            convert(b"a\r\nb\r\n", &u, "dos2unix", "x").unwrap(),
            b"a\nb\n"
        );
        assert_eq!(convert(b"a\rb\n", &u, "dos2unix", "x").unwrap(), b"a\rb\n");
        assert_eq!(
            convert(b"\xef\xbb\xbfhi\r\n", &u, "dos2unix", "x").unwrap(),
            b"hi\n"
        );
        assert_eq!(
            convert(b"a\nb\r\n", &d, "unix2dos", "x").unwrap(),
            b"a\r\nb\r\n"
        );
        assert_eq!(
            convert(b"\xef\xbb\xbfhi\n", &d, "unix2dos", "x").unwrap(),
            b"\xef\xbb\xbfhi\r\n"
        );
        assert!(convert(b"a\x00b", &u, "dos2unix", "x").is_none());
    }
}
