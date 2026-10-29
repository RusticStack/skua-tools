//! Table mode (`-t`): split the lines into cells, size the columns, print (or JSON).

use std::io::{self, Write};

use super::width::width;

pub struct Settings {
    pub osep: Vec<u8>,
    pub names: Vec<String>,
    pub right: Vec<bool>,
    pub hidden: Vec<bool>,
    pub order: Vec<usize>,
    pub header: bool,
    pub json: bool,
    pub json_name: String,
    /// `-m`: widen the columns to fill this many cells.
    pub maxout: Option<usize>,
}

/// Splits one line into cells.
pub fn split(line: &[u8], sep: &Option<Vec<u8>>, limit: usize) -> Vec<Vec<u8>> {
    let mut cells: Vec<Vec<u8>> = Vec::new();
    let blank = |b: u8| b == b' ' || b == b'\t';
    let mut i = 0;
    match sep {
        None => {
            while i < line.len() {
                while i < line.len() && blank(line[i]) {
                    i += 1;
                }
                if i >= line.len() {
                    break;
                }
                if cells.len() + 1 >= limit {
                    cells.push(line[i..].to_vec());
                    break;
                }
                let s = i;
                while i < line.len() && !blank(line[i]) {
                    i += 1;
                }
                cells.push(line[s..i].to_vec());
            }
        }
        Some(seps) => {
            let mut s = 0;
            loop {
                if cells.len() + 1 >= limit {
                    cells.push(line[s..].to_vec());
                    break;
                }
                match line[s..].iter().position(|b| seps.contains(b)) {
                    Some(p) => {
                        cells.push(line[s..s + p].to_vec());
                        s += p + 1;
                    }
                    None => {
                        cells.push(line[s..].to_vec());
                        break;
                    }
                }
            }
        }
    }
    cells
}

fn json_str(out: &mut dyn Write, s: &[u8]) -> io::Result<()> {
    out.write_all(b"\"")?;
    for &b in s {
        match b {
            b'"' => out.write_all(b"\\\"")?,
            b'\\' => out.write_all(b"\\\\")?,
            8 => out.write_all(b"\\b")?,
            12 => out.write_all(b"\\f")?,
            b'\n' => out.write_all(b"\\n")?,
            b'\r' => out.write_all(b"\\r")?,
            b'\t' => out.write_all(b"\\t")?,
            0..=0x1f => write!(out, "\\u{b:04x}")?,
            _ => out.write_all(&[b])?,
        }
    }
    out.write_all(b"\"")
}

/// Prints the table. `ncols` is the number of columns; `rows` holds the cells per line.
pub fn print(
    st: &Settings,
    rows: &[Vec<Vec<u8>>],
    ncols: usize,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), io::Error> {
    let shown: Vec<usize> = st
        .order
        .iter()
        .copied()
        .filter(|&c| !st.hidden[c])
        .collect();
    if st.json {
        return print_json(st, rows, ncols, &shown, out, err);
    }
    let mut w = vec![0usize; ncols];
    let named = st.header && st.names.iter().any(|n| !n.is_empty());
    if named {
        for (c, n) in st.names.iter().enumerate().take(ncols) {
            w[c] = w[c].max(width(n.as_bytes()));
        }
    }
    for r in rows {
        for (c, cell) in r.iter().enumerate() {
            w[c] = w[c].max(width(cell));
        }
    }
    let mut pad_last = false;
    if let Some(tw) = st.maxout {
        let total: usize = shown.iter().map(|&c| w[c]).sum::<usize>()
            + width(&st.osep) * shown.len().saturating_sub(1);
        if tw > total && !shown.is_empty() {
            let extra = tw - total;
            for &c in &shown {
                w[c] += extra / shown.len();
            }
            let rem = extra % shown.len();
            for &c in shown.iter().skip(shown.len() - rem) {
                w[c] += 1;
            }
            pad_last = true;
        }
    }
    let line = |out: &mut dyn Write, cell: &dyn Fn(usize) -> Vec<u8>| -> io::Result<()> {
        for (k, &c) in shown.iter().enumerate() {
            let text = cell(c);
            let pad = w[c].saturating_sub(width(&text));
            if k > 0 {
                out.write_all(&st.osep)?;
            }
            if st.right[c] {
                out.write_all(&vec![b' '; pad])?;
                out.write_all(&text)?;
            } else {
                out.write_all(&text)?;
                if k + 1 < shown.len() || pad_last {
                    out.write_all(&vec![b' '; pad])?;
                }
            }
        }
        out.write_all(b"\n")
    };
    if named {
        line(out, &|c| {
            st.names
                .get(c)
                .map(|n| n.as_bytes().to_vec())
                .unwrap_or_default()
        })?;
    }
    for r in rows {
        line(out, &|c| r.get(c).cloned().unwrap_or_default())?;
    }
    Ok(())
}

fn print_json(
    st: &Settings,
    rows: &[Vec<Vec<u8>>],
    ncols: usize,
    shown: &[usize],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), io::Error> {
    for (i, r) in rows.iter().enumerate() {
        for c in 0..r.len().min(ncols) {
            if st.names.get(c).is_none_or(|n| n.is_empty()) {
                writeln!(
                    err,
                    "column: line {}: for JSON the name of the column {} is required",
                    i + 2,
                    c + 1
                )?;
                return Err(io::Error::other("json"));
            }
        }
    }
    write!(out, "{{\n   ")?;
    json_str(out, st.json_name.as_bytes())?;
    out.write_all(b": [\n")?;
    for (i, r) in rows.iter().enumerate() {
        out.write_all(if i == 0 { b"      {\n" } else { b",{\n" })?;
        for (k, &c) in shown.iter().enumerate() {
            out.write_all(b"         ")?;
            json_str(out, st.names.get(c).map_or(b"", |n| n.as_bytes()))?;
            out.write_all(b": ")?;
            match r.get(c) {
                Some(cell) => json_str(out, cell)?,
                None => out.write_all(b"null")?,
            }
            out.write_all(if k + 1 < shown.len() { b",\n" } else { b"\n" })?;
        }
        out.write_all(b"      }")?;
    }
    if !rows.is_empty() {
        out.write_all(b"\n")?;
    }
    out.write_all(b"   ]\n}\n")
}
