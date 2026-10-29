//! The conversion loop: encode each decoded character, apply //TRANSLIT, -c and substitutions,
//! and report errors with libiconv's `file:line:column` positions.

use std::io::Write;

use super::charset::{Encoder, Kind};
use super::decode::{Dec, decode};
use super::subst::{Format, translit, translit_for};

pub struct Conv {
    pub enc: Encoder,
    pub translit: bool,
    pub ignore: bool,
    pub discard: bool,
    pub silent: bool,
    pub unicode_subst: Option<Format>,
    pub byte_subst: Option<Format>,
    pub omitted: bool,
}

impl Conv {
    /// Encodes every character of `text`, or none if one does not fit.
    pub fn encode_str(&mut self, text: &str) -> Option<Vec<u8>> {
        let mut v = Vec::new();
        for c in text.chars() {
            v.extend(self.enc.encode(c as u32)?);
        }
        Some(v)
    }

    /// The bytes standing in for the unconvertible `cp`: translit, substitution; `None`: none.
    pub fn replacement(&mut self, cp: u32) -> Option<(Vec<u8>, bool)> {
        if self.translit {
            if let Some(b) = translit_for(self.enc.kind(), cp) {
                return Some((b, false));
            }
            if let Some(b) = translit(cp).and_then(|t| self.encode_str(t)) {
                return Some((b, false));
            }
        }
        if self.discard {
            return None;
        }
        let f = self.unicode_subst.clone()?;
        self.encode_str(&f.apply(cp)).map(|b| (b, true))
    }

    /// `--byte-subst` text for invalid input `bytes` (and the columns it counts as).
    pub fn byte_replacement(&mut self, bytes: &[u8]) -> Option<(Vec<u8>, u64)> {
        let f = self.byte_subst.clone().filter(|_| !bytes.is_empty())?;
        let (mut v, mut n) = (Vec::new(), 0u64);
        for b in bytes {
            let text = f.apply(u32::from(*b));
            n += text.chars().count() as u64;
            v.extend(self.encode_str(&text)?);
        }
        Some((v, n))
    }

    /// Converts one input; false when it stopped on an error (already reported).
    pub fn convert(
        &mut self,
        name: &str,
        data: &[u8],
        kind: Kind,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> bool {
        let (mut line, mut col) = (1u64, 0u64);
        let mut buf: Vec<u8> = Vec::with_capacity(16384);
        let mut ok = true;
        let silent = self.silent;
        let report = |buf: &mut Vec<u8>,
                      out: &mut dyn Write,
                      err: &mut dyn Write,
                      what: &str,
                      line: u64,
                      col: u64| {
            let _ = out.write_all(buf);
            buf.clear();
            let _ = out.flush();
            if !silent {
                if col > 0 {
                    let _ = err.write_all(b"\n");
                }
                let _ = writeln!(err, "iconv: {name}:{line}:{col}: {what}");
            }
        };
        decode(kind, data, &mut |it| {
            match it.dec {
                Dec::Ch(cp) => {
                    match self.enc.encode(cp) {
                        Some(b) => buf.extend(b),
                        None => match self.replacement(cp) {
                            Some((b, subst)) => {
                                buf.extend(b);
                                if subst {
                                    col += 1;
                                }
                            }
                            None if self.ignore => col += 1,
                            None if self.discard => {
                                self.omitted = true;
                                col += 1;
                            }
                            None => {
                                report(&mut buf, out, err, "cannot convert", line, col);
                                ok = false;
                                return false;
                            }
                        },
                    }
                    if cp == 0x0a {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                }
                Dec::Bad => {
                    let bytes = data.get(it.start..it.start + it.len).unwrap_or(&[]);
                    if let Some((v, n)) = self.byte_replacement(bytes) {
                        buf.extend(v);
                        col += n;
                        return true;
                    }
                    if self.ignore {
                    } else if self.discard {
                        self.omitted = true;
                    } else {
                        report(&mut buf, out, err, "cannot convert", line, col);
                        ok = false;
                        return false;
                    }
                }
                Dec::Incomplete => {
                    let bytes = data.get(it.start..it.start + it.len).unwrap_or(&[]);
                    if let Some((v, _)) = self.byte_replacement(bytes) {
                        buf.extend(v);
                    }
                    report(
                        &mut buf,
                        out,
                        err,
                        "incomplete character or shift sequence",
                        line,
                        col,
                    );
                    ok = false;
                    return false;
                }
            }
            if buf.len() >= 8192 {
                let _ = out.write_all(&buf);
                buf.clear();
            }
            true
        });
        if ok {
            let _ = out.write_all(&buf);
        }
        ok
    }
}
