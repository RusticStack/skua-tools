//! `//TRANSLIT` (approximations for characters the target lacks) and the `--*-subst`
//! format strings.

use std::sync::OnceLock;

const TABLE: &str = include_str!("../data/iconv_translit.dat");

fn table() -> &'static Vec<(u32, &'static str)> {
    static T: OnceLock<Vec<(u32, &'static str)>> = OnceLock::new();
    T.get_or_init(|| {
        let mut v: Vec<(u32, &'static str)> = TABLE
            .lines()
            .filter_map(|l| {
                let (hex, text) = l.split_once('=')?;
                Some((u32::from_str_radix(hex, 16).ok()?, text))
            })
            .collect();
        v.sort_by_key(|e| e.0);
        v
    })
}

/// The ASCII approximation of `cp` as libiconv's `//TRANSLIT` writes it.
pub fn translit(cp: u32) -> Option<&'static str> {
    let t = table();
    t.binary_search_by_key(&cp, |e| e.0).ok().map(|i| t[i].1)
}

/// A parsed `--unicode-subst` / `--byte-subst` / `--widechar-subst` format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Lit(String),
    Num {
        alt: bool,
        zero: bool,
        left: bool,
        width: usize,
        conv: char,
    },
}

impl Format {
    /// Parses `s`; the error is the message tail after `--NAME argument: `.
    pub fn parse(s: &str) -> Result<Format, String> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            if c != '%' {
                lit.push(c);
                continue;
            }
            if it.peek() == Some(&'%') {
                it.next();
                lit.push('%');
                continue;
            }
            let (mut alt, mut zero, mut left) = (false, false, false);
            while let Some(&f) = it.peek() {
                match f {
                    '#' => alt = true,
                    '0' => zero = true,
                    '-' => left = true,
                    '+' | ' ' => {}
                    _ => break,
                }
                it.next();
            }
            let mut width = 0usize;
            while let Some(d) = it.peek().and_then(|c| c.to_digit(10)) {
                width = width * 10 + d as usize;
                it.next();
            }
            match it.next() {
                Some(conv @ ('x' | 'X' | 'o' | 'u')) => {
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(Part::Num {
                        alt,
                        zero,
                        left,
                        width,
                        conv,
                    });
                }
                Some(other) if "hlLjzqt".contains(other) => {
                    return Err("A format directive with a size is not allowed here.".to_owned());
                }
                Some(other) => {
                    return Err(format!(
                        "The character '{other}' is not a valid conversion specifier."
                    ));
                }
                None => return Err("The string ends in the middle of a directive.".to_owned()),
            }
        }
        if !lit.is_empty() {
            parts.push(Part::Lit(lit));
        }
        Ok(Format { parts })
    }

    /// The text for the number `n`.
    pub fn apply(&self, n: u32) -> String {
        let mut out = String::new();
        for p in &self.parts {
            match p {
                Part::Lit(s) => out.push_str(s),
                Part::Num {
                    alt,
                    zero,
                    left,
                    width,
                    conv,
                } => {
                    let mut digits = match conv {
                        'x' => format!("{n:x}"),
                        'X' => format!("{n:X}"),
                        'o' => format!("{n:o}"),
                        _ => format!("{n}"),
                    };
                    let prefix = match (alt, conv) {
                        (true, 'x') if n != 0 => "0x",
                        (true, 'X') if n != 0 => "0X",
                        (true, 'o') if !digits.starts_with('0') => "0",
                        _ => "",
                    };
                    let used = prefix.len() + digits.len();
                    if *zero && !*left && *width > used {
                        digits = format!("{}{digits}", "0".repeat(width - used));
                    }
                    let mut s = format!("{prefix}{digits}");
                    if s.len() < *width {
                        let pad = " ".repeat(width - s.len());
                        s = if *left {
                            format!("{s}{pad}")
                        } else {
                            format!("{pad}{s}")
                        };
                    }
                    out.push_str(&s);
                }
            }
        }
        out
    }
}

/// Target-specific `//TRANSLIT` results (as bytes of that character set) where libiconv prefers
/// a candidate that is not ASCII, e.g. acute accent plus letter for ISO-8859-1.
pub fn translit_for(target: super::charset::Kind, cp: u32) -> Option<Vec<u8>> {
    use super::charset::Kind;
    fn load(text: &'static str) -> Vec<(u32, Vec<u8>)> {
        let mut v: Vec<(u32, Vec<u8>)> = text
            .lines()
            .filter_map(|l| {
                let (c, h) = l.split_once('=')?;
                let bytes = (0..h.len() / 2)
                    .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok())
                    .collect::<Option<Vec<u8>>>()?;
                Some((u32::from_str_radix(c, 16).ok()?, bytes))
            })
            .collect();
        v.sort_by_key(|e| e.0);
        v
    }
    static LATIN1: OnceLock<Vec<(u32, Vec<u8>)>> = OnceLock::new();
    static CP1252: OnceLock<Vec<(u32, Vec<u8>)>> = OnceLock::new();
    static ISO15: OnceLock<Vec<(u32, Vec<u8>)>> = OnceLock::new();
    let t = match target {
        Kind::Latin1 => {
            LATIN1.get_or_init(|| load(include_str!("../data/iconv_translit_latin1.dat")))
        }
        Kind::Rs("cp1252") => {
            CP1252.get_or_init(|| load(include_str!("../data/iconv_translit_cp1252.dat")))
        }
        Kind::Rs("iso15") => {
            ISO15.get_or_init(|| load(include_str!("../data/iconv_translit_iso15.dat")))
        }
        _ => return None,
    };
    t.binary_search_by_key(&cp, |e| e.0)
        .ok()
        .map(|i| t[i].1.clone())
}
