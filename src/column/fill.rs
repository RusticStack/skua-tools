//! Fill mode (no `-t`): the entries are laid out in tab-separated columns that fit the width,
//! as util-linux's `column` does (columns first, or rows first with `-x`).

use std::io::{self, Write};

use super::width::width;

const TAB: usize = 8;

fn tabs(out: &mut dyn Write, chcnt: &mut usize, endcol: usize) -> io::Result<()> {
    loop {
        let cnt = (*chcnt + TAB) & !(TAB - 1);
        if cnt > endcol {
            return Ok(());
        }
        out.write_all(b"\t")?;
        *chcnt = cnt;
    }
}

pub fn fill(
    entries: &[Vec<u8>],
    termwidth: usize,
    fillrows: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let widths: Vec<usize> = entries.iter().map(|e| width(e)).collect();
    let maxlen = (widths.iter().copied().max().unwrap_or(0) + TAB) & !(TAB - 1);
    let numcols = (termwidth / maxlen).max(1);
    let n = entries.len();
    if fillrows {
        let (mut chcnt, mut col, mut endcol) = (0usize, 0usize, maxlen);
        for i in 0..n {
            out.write_all(&entries[i])?;
            chcnt += widths[i];
            if i + 1 == n {
                break;
            }
            col += 1;
            if col == numcols {
                chcnt = 0;
                col = 0;
                endcol = maxlen;
                out.write_all(b"\n")?;
            } else {
                tabs(out, &mut chcnt, endcol)?;
                endcol += maxlen;
            }
        }
        return out.write_all(b"\n");
    }
    let numrows = n.div_ceil(numcols);
    for row in 0..numrows {
        let mut endcol = maxlen;
        let mut base = row;
        let mut chcnt = 0usize;
        for _ in 0..numcols {
            out.write_all(&entries[base])?;
            chcnt += widths[base];
            base += numrows;
            if base >= n {
                break;
            }
            tabs(out, &mut chcnt, endcol)?;
            endcol += maxlen;
        }
        out.write_all(b"\n")?;
    }
    Ok(())
}
