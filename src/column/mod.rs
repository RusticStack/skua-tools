//! column: columnate lists, as util-linux 2.40's `column` (the one in Git for Windows).
//!
//! Fill mode (`-x`, `-c`) and table mode (`-t -s -o -N -C -O -H -R -d -l -L -J -n`) follow it
//! byte for byte. Accepted and ignored: `-E -T -W -m -e -r -i -p` (no truncation, wrapping,
//! maxout, repeated headers or tree output).

mod args;
mod console;
mod fill;
mod table;
#[cfg(test)]
mod tests;
mod width;

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use skua_tools::gnu_error;
use table::Settings;

pub fn main_exit() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut err = io::stderr().lock();
    let width = terminal_width();
    let code = run(&args, &mut io::stdin().lock(), &mut out, &mut err, width);
    let _ = out.flush();
    ExitCode::from(code as u8)
}

/// The width fill mode lays out for: the console, else `$COLUMNS`, else 80.
fn terminal_width() -> usize {
    if io::stdout().is_terminal()
        && let Some(w) = console::console_width()
    {
        return w;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|w| *w > 0)
        .unwrap_or(80)
}

fn usage_error(err: &mut dyn Write, msg: &str) -> i32 {
    let _ = writeln!(err, "column: {msg}");
    let _ = writeln!(err, "Try 'column --help' for more information.");
    1
}

/// A column named by `-O`/`-H`/`-R`: 1-based number, negative from the end, or a name.
fn resolve(spec: &str, names: &[String], ncols: usize) -> Result<Vec<usize>, String> {
    let mut v = Vec::new();
    for tok in spec.split(',').filter(|t| !t.is_empty()) {
        let undefined = || format!("undefined column name '{tok}'");
        if let Ok(n) = tok.parse::<i64>() {
            if n == 0 {
                continue;
            }
            let idx = if n > 0 { n - 1 } else { ncols as i64 + n };
            if idx < 0 || idx >= ncols as i64 {
                return Err(undefined());
            }
            v.push(idx as usize);
        } else {
            match names.iter().position(|n| n == tok) {
                Some(i) => v.push(i),
                None => return Err(undefined()),
            }
        }
    }
    Ok(v)
}

#[derive(Default)]
struct Config {
    table: bool,
    json: bool,
    name: Option<String>,
    order: Option<String>,
    cols_def: Vec<(String, bool, bool)>,
    names: Option<String>,
    limit: Option<usize>,
    noheadings: bool,
    hide: Option<String>,
    right: Option<String>,
    keep_empty: bool,
    width: Option<usize>,
    osep: Option<String>,
    sep: Option<String>,
    fillrows: bool,
    maxout: bool,
}

pub fn run(
    argv: &[String],
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
    term_width: usize,
) -> i32 {
    let parsed = match args::parse(argv) {
        Ok(p) => p,
        Err(m) => return usage_error(err, &m),
    };
    let mut cfg = Config::default();
    for (opt, val) in parsed.opts {
        let v = val.unwrap_or_default();
        match opt {
            't' => cfg.table = true,
            'J' => {
                cfg.table = true;
                cfg.json = true;
            }
            'n' => cfg.name = Some(v),
            'O' => cfg.order = Some(v),
            'C' => {
                let mut name = String::new();
                let (mut right, mut hidden) = (false, false);
                for p in v.split(',') {
                    match p.split_once('=') {
                        Some(("name", n)) => name = n.to_owned(),
                        None if p == "right" => right = true,
                        None if p == "hidden" => hidden = true,
                        _ => {}
                    }
                }
                cfg.cols_def.push((name, right, hidden));
            }
            'N' => cfg.names = Some(v),
            'l' => match v.parse::<usize>() {
                Ok(n) if n > 0 => cfg.limit = Some(n),
                _ => {
                    let _ = writeln!(err, "column: invalid columns limit argument: '{v}'");
                    return 1;
                }
            },
            'd' => cfg.noheadings = true,
            'H' => cfg.hide = Some(v),
            'R' => cfg.right = Some(v),
            'L' => cfg.keep_empty = true,
            'c' => match v.parse::<usize>() {
                Ok(n) => cfg.width = Some(n),
                Err(_) => {
                    let _ = writeln!(err, "column: invalid columns argument: '{v}'");
                    return 1;
                }
            },
            'o' => cfg.osep = Some(v),
            's' => cfg.sep = Some(v),
            'x' => cfg.fillrows = true,
            'm' => cfg.maxout = true,
            'h' => {
                let _ = out.write_all(args::HELP.as_bytes());
                return 0;
            }
            'V' => {
                let _ = writeln!(out, "{}", args::VERSION);
                return 0;
            }
            _ => {}
        }
    }
    if cfg.names.is_some() && !cfg.cols_def.is_empty() {
        let _ = writeln!(
            err,
            "column: mutually exclusive arguments: --table-column --table-columns"
        );
        return 1;
    }
    if cfg.json && cfg.names.is_none() && cfg.cols_def.is_empty() {
        let _ = writeln!(
            err,
            "column: option --table-columns or --table-column required for --json"
        );
        return 1;
    }
    let mut failed = false;
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let read_one = |r: &mut dyn Read, entries: &mut Vec<Vec<u8>>| {
        let mut data = Vec::new();
        let _ = r.read_to_end(&mut data);
        let mut parts: Vec<&[u8]> = data.split(|b| *b == b'\n').collect();
        if parts.last().is_some_and(|l| l.is_empty()) {
            parts.pop();
        }
        for mut l in parts {
            if let Some(p) = l.iter().position(|b| *b == 0) {
                l = &l[..p];
            }
            let blank = l.iter().all(|b| *b == b' ' || *b == b'\t');
            if blank {
                l = &[];
            }
            if !l.is_empty() || cfg.keep_empty {
                entries.push(l.to_vec());
            }
        }
    };
    if parsed.files.is_empty() {
        read_one(stdin, &mut entries);
    } else {
        for f in &parsed.files {
            match File::open(f) {
                Ok(mut fh) => read_one(&mut fh, &mut entries),
                Err(e) => {
                    let _ = writeln!(err, "column: {f}: {}", gnu_error(&e));
                    failed = true;
                }
            }
        }
    }
    if !cfg.table {
        let w = cfg.width.unwrap_or(term_width);
        let r = fill::fill(&entries, w, cfg.fillrows, out);
        return i32::from(failed || r.is_err());
    }
    match run_table(&cfg, entries, term_width, out, err) {
        Ok(had_rows) => i32::from(failed && !had_rows),
        Err(code) => code,
    }
}

fn run_table(
    cfg: &Config,
    entries: Vec<Vec<u8>>,
    term_width: usize,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<bool, i32> {
    let sep = cfg.sep.as_ref().map(|s| s.as_bytes().to_vec());
    let limit = cfg.limit.unwrap_or(usize::MAX);
    let rows: Vec<Vec<Vec<u8>>> = entries
        .iter()
        .map(|l| table::split(l, &sep, limit))
        .collect();
    let mut names: Vec<String> = match &cfg.names {
        Some(n) => n.split(',').map(str::to_owned).collect(),
        None => cfg.cols_def.iter().map(|c| c.0.clone()).collect(),
    };
    let ncols = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(names.len());
    names.resize(names.len().max(ncols), String::new());
    let names_given = cfg.names.is_some() || cfg.cols_def.iter().any(|c| !c.0.is_empty());
    let fail = |m: String, err: &mut dyn Write| {
        let _ = writeln!(err, "column: {m}");
        1
    };
    let mut right = vec![false; ncols];
    let mut hidden = vec![false; ncols];
    for (i, c) in cfg.cols_def.iter().enumerate() {
        right[i] = c.1;
        hidden[i] = c.2;
    }
    if let Some(r) = &cfg.right {
        right.iter_mut().for_each(|x| *x = false);
        for c in resolve(r, &names, ncols).map_err(|m| fail(m, err))? {
            right[c] = true;
        }
    }
    if let Some(h) = &cfg.hide {
        for c in resolve(h, &names, ncols).map_err(|m| fail(m, err))? {
            hidden[c] = true;
        }
    }
    let mut order: Vec<usize> = Vec::new();
    if let Some(o) = &cfg.order {
        for c in resolve(o, &names, ncols).map_err(|m| fail(m, err))? {
            if !order.contains(&c) {
                order.push(c);
            }
        }
    }
    for c in 0..ncols {
        if !order.contains(&c) {
            order.push(c);
        }
    }
    let st = Settings {
        osep: cfg
            .osep
            .as_ref()
            .map_or_else(|| b"  ".to_vec(), |s| s.as_bytes().to_vec()),
        names: if names_given { names } else { Vec::new() },
        right,
        hidden,
        order,
        header: !cfg.noheadings,
        json: cfg.json,
        json_name: cfg.name.clone().unwrap_or_else(|| "table".to_owned()),
        maxout: cfg.maxout.then_some(cfg.width.unwrap_or(term_width)),
    };
    let had = !rows.is_empty();
    match table::print(&st, &rows, ncols, out, err) {
        Ok(()) => Ok(had),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(had),
        Err(_) => Err(1),
    }
}
