//! `unzip -l`, `-v` and `-Z` listings (a child module of `bin/unzip.rs`).

use super::*;
use ziptime::{Civil, MONTHS, civil_from_dos, local_from_unix};

/// Modification time of an entry as printed: the UTC extra field in local time, else the DOS time.
pub fn entry_time(e: &Entry) -> Civil {
    e.mtime_utc
        .map_or_else(|| civil_from_dos(e.dos_date, e.dos_time), local_from_unix)
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Info-ZIP's per-mille ratio `(uc - c) / uc`, rounded half away from zero.
fn permille(uc: u64, c: u64) -> i64 {
    if uc == 0 {
        return 0;
    }
    let (uc, c) = (uc as i128, c as i128);
    let r = ((uc - c).abs() * 1000 + uc / 2) / uc;
    (if uc >= c { r } else { -r }) as i64
}

fn pct(uc: u64, c: u64) -> String {
    let p = permille(uc, c);
    let v = (p.abs() + 5) / 10;
    format!("{:>3}%", if p < 0 { -v } else { v })
}

fn method_name(e: &Entry) -> String {
    match e.method {
        0 => "Stored".to_owned(),
        8 => format!(
            "Defl:{}",
            ['N', 'X', 'F', 'S'][usize::from((e.flags >> 1) & 3)]
        ),
        1 => "Shrunk".to_owned(),
        6 => "Implode".to_owned(),
        9 => "Defl64".to_owned(),
        12 => "BZip2".to_owned(),
        14 => "LZMA".to_owned(),
        m => format!("Unk:{m:03}"),
    }
}

pub fn list(o: &Opts, name: &str, arch: &Archive) -> u8 {
    let verbose = o.verbose && !o.list;
    if o.quiet == 0 {
        println!("Archive:  {name}");
        print_comment(arch);
    }
    if o.quiet < 2 {
        if verbose {
            println!(" Length   Method    Size  Cmpr    Date    Time   CRC-32   Name");
            println!("--------  ------  ------- ---- ---------- ----- --------  ----");
        } else {
            println!("  Length      Date    Time    Name");
            println!("---------  ---------- -----   ----");
        }
    }
    let (mut n, mut tu, mut tc) = (0usize, 0u64, 0u64);
    for e in selected(o, arch) {
        let t = entry_time(e);
        let date = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            t.year, t.month, t.day, t.hour, t.min
        );
        if verbose {
            println!(
                "{:>8}  {:<6} {:>8} {} {date} {:08x}  {}",
                e.usize,
                method_name(e),
                e.csize,
                pct(e.usize, e.csize),
                e.crc,
                e.name
            );
        } else {
            println!("{:>9}  {date}   {}", e.usize, e.name);
        }
        n += 1;
        tu += e.usize;
        tc += e.csize;
    }
    if o.quiet < 2 {
        if verbose {
            println!("--------          -------  ---                            -------");
            println!(
                "{tu:>8}         {tc:>8} {}{}{n} file{}",
                pct(tu, tc),
                " ".repeat(28),
                plural(n)
            );
        } else {
            println!("---------                     -------");
            println!("{tu:>9}                     {n} file{}", plural(n));
        }
    }
    if n == 0 { 11 } else { 0 }
}

fn host_name(h: u8) -> &'static str {
    const NAMES: [&str; 20] = [
        "fat", "ami", "vms", "unx", "cms", "atr", "hpf", "mac", "zzz", "cpm", "ntf", "mvs", "vfa",
        "acn", "vfs", "mac", "tan", "omv", "mac", "ibm",
    ];
    NAMES.get(usize::from(h)).copied().unwrap_or("???")
}

fn perms(e: &Entry) -> String {
    let mode = e.ext_attr >> 16;
    let dir = e.is_dir();
    if mode != 0 && (e.version_made >> 8 == 3 || mode & 0o170000 != 0 || mode & 0o777 != 0) {
        let kind = match mode & 0o170000 {
            0o040000 => 'd',
            0o120000 => 'l',
            _ if dir => 'd',
            _ => '-',
        };
        let mut s = String::from(kind);
        for (i, ch) in "rwxrwxrwx".chars().enumerate() {
            s.push(if mode & (0o400 >> i) != 0 { ch } else { '-' });
        }
        return s;
    }
    let a = e.ext_attr as u8;
    let exe = [".exe", ".com", ".bat", ".cmd"]
        .iter()
        .any(|x| e.name.to_lowercase().ends_with(x));
    format!(
        "{}r{}{}{}{}{}",
        if dir || a & 0x10 != 0 { 'd' } else { '-' },
        if a & 1 != 0 { '-' } else { 'w' },
        if dir || exe { 'x' } else { '-' },
        if a & 0x20 != 0 { 'a' } else { '-' },
        if a & 2 != 0 { 'h' } else { '-' },
        if a & 4 != 0 { 's' } else { '-' },
    )
}

pub fn zipinfo(o: &Opts, name: &str, arch: &Archive) -> u8 {
    let names_only = o.zipinfo == Some(true);
    if !names_only {
        println!("Archive:  {name}");
        println!(
            "Zip file size: {} bytes, number of entries: {}",
            arch.size,
            arch.entries.len()
        );
    }
    let (mut n, mut tu, mut tc) = (0usize, 0u64, 0u64);
    for e in selected(o, arch) {
        n += 1;
        tu += e.usize;
        tc += e.csize;
        if names_only {
            println!("{}", e.name);
            continue;
        }
        let ver = e.version_made & 0xff;
        let m = match e.method {
            0 => "stor".to_owned(),
            8 => format!(
                "def{}",
                ['N', 'X', 'F', 'S'][usize::from((e.flags >> 1) & 3)]
            ),
            m => format!("u{m:03}"),
        };
        let kind = if e.int_attr & 1 != 0 { 't' } else { 'b' };
        let kind = if e.flags & 1 != 0 {
            kind.to_ascii_uppercase()
        } else {
            kind
        };
        let has_extra = e.central.len() > 46 + e.name.len();
        let second = match (has_extra, e.flags & 8 != 0) {
            (true, true) => 'X',
            (true, false) => 'x',
            (false, true) => 'l',
            (false, false) => '-',
        };
        let t = entry_time(e);
        println!(
            "{}  {}.{} {} {:>8} {kind}{second} {m} {:02}-{}-{:02} {:02}:{:02} {}",
            perms(e),
            ver / 10,
            ver % 10,
            host_name((e.version_made >> 8) as u8),
            e.usize,
            t.year % 100,
            MONTHS[(t.month as usize).clamp(1, 12) - 1],
            t.day,
            t.hour,
            t.min,
            e.name
        );
    }
    for p in unmatched(o, arch) {
        eprintln!("caution: filename not matched:  {p}");
    }
    if !names_only {
        let p = permille(tu, tc);
        println!(
            "{n} file{}, {tu} bytes uncompressed, {tc} bytes compressed:  {}.{}%",
            plural(n),
            p / 10,
            p % 10
        );
    }
    if n == 0 { 11 } else { 0 }
}
