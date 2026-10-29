//! xxd: option parsing (`opts`), dumping (`dump`), reverting (`revert`).
//!
//! Matches the vim xxd that ships with Git for Windows (version string `2026-06-16`): same
//! bytes on stdout, the same `xxd: ...` messages and the same exit codes (1 usage and option
//! errors, 2 cannot read the input, 3 cannot write the output, 4 cannot seek, 5 cannot seek
//! backwards in `-r`, 255 cannot revert `-i`/`-e`). Known deviations: with `-R`, colours are
//! produced for the normal and `-b` layouts only; `-r` to a redirected stdout cannot seek.

mod dump;
mod opts;
mod revert;
#[cfg(test)]
mod tests;

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, IsTerminal, Read, Seek, SeekFrom, Write};
use std::process::ExitCode;

use opts::{Color, Fail, HexType, Opts, Parsed};
use skua_tools::gnu_error;

const VERSION: &str = "xxd 2026-06-16 by Juergen Weigert et al.";

pub fn main_exit() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let code = run(&args);
    ExitCode::from(code as u8)
}

fn fail(f: &Fail) -> i32 {
    let mut err = io::stderr().lock();
    if f.usage {
        let _ = err.write_all(opts::USAGE.as_bytes());
    } else {
        let _ = writeln!(err, "xxd: {}", f.msg);
    }
    f.code
}

/// Runs xxd with `args` (including the program name); returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let o = match opts::parse(args) {
        Ok(Parsed::Run(o)) => o,
        Ok(Parsed::Version) => {
            let _ = writeln!(io::stderr(), "{VERSION}");
            return 0;
        }
        Err(f) => return fail(&f),
    };
    match execute(&o) {
        Ok(()) => 0,
        Err(f) => fail(&f),
    }
}

fn execute(o: &Opts) -> Result<(), Fail> {
    if o.revert && matches!(o.hextype, HexType::Include | HexType::Little) {
        return Err(Fail::msg(255, "Sorry, cannot revert this type of hexdump"));
    }
    let infile = o.infile.as_deref().filter(|n| *n != "-");
    let mut input: Box<dyn Read> = match infile {
        Some(name) => Box::new(
            File::open(name).map_err(|e| Fail::msg(2, format!("{name}: {}", gnu_error(&e))))?,
        ),
        None => Box::new(io::stdin().lock()),
    };
    let outfile = o.outfile.as_deref().filter(|n| *n != "-");
    if o.revert {
        return match outfile {
            Some(name) => {
                let f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(name)
                    .map_err(|e| Fail::msg(3, format!("{name}: {}", gnu_error(&e))))?;
                revert::revert(o, &mut input, &mut revert::FileSink::new(f))
            }
            None if revert::stdout_file_sink().is_some() => {
                let mut sink = revert::stdout_file_sink().ok_or_else(|| Fail::msg(3, "stdout"))?;
                revert::revert(o, &mut input, &mut sink)
            }
            None => {
                let mut out = BufWriter::new(io::stdout().lock());
                let mut sink = revert::PipeSink::new(&mut out);
                let r = revert::revert(o, &mut input, &mut sink);
                let _ = out.flush();
                r
            }
        };
    }
    let start = seek_input(o, infile, &mut input)?;
    let color = match o.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => outfile.is_none() && io::stdout().is_terminal() && no_color_unset(),
    };
    let name = include_name(o, infile);
    let result = match outfile {
        Some(path) => {
            let f = File::create(path)
                .map_err(|e| Fail::msg(3, format!("{path}: {}", gnu_error(&e))))?;
            let mut w = BufWriter::new(f);
            let r = dump::dump(o, name.as_deref(), &mut input, &mut w, start, color);
            r.and_then(|()| w.flush())
        }
        None => {
            let mut w = BufWriter::new(io::stdout().lock());
            let r = dump::dump(o, name.as_deref(), &mut input, &mut w, start, color);
            r.and_then(|()| w.flush())
        }
    };
    match result {
        Ok(()) => Ok(()),
        // `xxd file | head`: the reader went away.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Fail::msg(3, gnu_error(&e))),
    }
}

fn no_color_unset() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

/// The C variable name for `-i`: `-n NAME`, else the file name with non-alphanumerics as `_`
/// (and `__` first when it starts with a digit); none for an unnamed stdin.
fn include_name(o: &Opts, infile: Option<&str>) -> Option<String> {
    if let Some(n) = &o.name {
        return Some(n.clone());
    }
    let f = infile?;
    let mut s = String::new();
    if f.starts_with(|c: char| c.is_ascii_digit()) {
        s.push_str("__");
    }
    s.extend(
        f.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }),
    );
    Some(s)
}

/// Applies `-s`; returns the absolute position the dump starts at.
fn seek_input(o: &Opts, file: Option<&str>, input: &mut Box<dyn Read>) -> Result<u64, Fail> {
    let cannot = || Fail::msg(4, "Sorry, cannot seek.");
    let Some(seek) = o.seek else { return Ok(0) };
    match file {
        Some(name) => {
            let mut f =
                File::open(name).map_err(|e| Fail::msg(2, format!("{name}: {}", gnu_error(&e))))?;
            let pos = if seek.negative {
                f.seek(SeekFrom::End(-seek.amount)).map_err(|_| cannot())?
            } else {
                f.seek(SeekFrom::Start(seek.amount as u64))
                    .map_err(|_| cannot())?
            };
            *input = Box::new(f);
            Ok(pos)
        }
        None => {
            if seek.negative {
                return Err(cannot());
            }
            let n = seek.amount as u64;
            let got =
                io::copy(&mut input.by_ref().take(n), &mut io::sink()).map_err(|_| cannot())?;
            if got < n {
                return Err(cannot());
            }
            Ok(n)
        }
    }
}
