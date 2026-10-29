//! Character sets: names (the aliases GNU libiconv 1.19 accepts), decoding to Unicode
//! scalar values and encoding back.

use encoding_rs::Encoding;

const TABLE: &str = include_str!("../data/iconv_charsets.dat");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Ascii,
    Utf8,
    Latin1,
    Iso9,
    /// 16-bit units: (big endian by default, BOM allowed on input, BOM written, surrogate pairs)
    Utf16 {
        le: bool,
        bom_in: bool,
        bom_out: bool,
        pairs: bool,
    },
    Utf32 {
        le: bool,
        bom_in: bool,
        bom_out: bool,
    },
    /// Everything `encoding_rs` knows: the index of its encoding in [`rs_encoding`].
    Rs(&'static str),
}

/// A set of names for one character set, as `iconv -l` prints them.
pub struct Entry {
    pub key: &'static str,
    pub names: &'static str,
}

pub fn entries() -> impl Iterator<Item = Entry> {
    TABLE.lines().filter_map(|l| {
        let (key, names) = l.split_once('\t')?;
        Some(Entry { key, names })
    })
}

fn kind_of(key: &'static str) -> Option<Kind> {
    Some(match key {
        "ascii" => Kind::Ascii,
        "utf8" => Kind::Utf8,
        "latin1" => Kind::Latin1,
        "iso9" => Kind::Iso9,
        "ucs2" => Kind::Utf16 {
            le: false,
            bom_in: true,
            bom_out: false,
            pairs: false,
        },
        "ucs2be" => Kind::Utf16 {
            le: false,
            bom_in: false,
            bom_out: false,
            pairs: false,
        },
        "ucs2le" => Kind::Utf16 {
            le: true,
            bom_in: false,
            bom_out: false,
            pairs: false,
        },
        "utf16" => Kind::Utf16 {
            le: false,
            bom_in: true,
            bom_out: true,
            pairs: true,
        },
        "utf16be" => Kind::Utf16 {
            le: false,
            bom_in: false,
            bom_out: false,
            pairs: true,
        },
        "utf16le" => Kind::Utf16 {
            le: true,
            bom_in: false,
            bom_out: false,
            pairs: true,
        },
        "ucs4" | "ucs4be" | "utf32be" => Kind::Utf32 {
            le: false,
            bom_in: false,
            bom_out: false,
        },
        "ucs4le" | "utf32le" => Kind::Utf32 {
            le: true,
            bom_in: false,
            bom_out: false,
        },
        "utf32" => Kind::Utf32 {
            le: false,
            bom_in: true,
            bom_out: true,
        },
        other => Kind::Rs(other),
    })
}

pub fn rs_encoding(key: &str) -> Option<&'static Encoding> {
    use encoding_rs as e;
    Some(match key {
        "iso2" => e::ISO_8859_2,
        "iso3" => e::ISO_8859_3,
        "iso4" => e::ISO_8859_4,
        "iso5" => e::ISO_8859_5,
        "iso6" => e::ISO_8859_6,
        "iso7" => e::ISO_8859_7,
        "iso8" => e::ISO_8859_8,
        "iso10" => e::ISO_8859_10,
        "iso13" => e::ISO_8859_13,
        "iso14" => e::ISO_8859_14,
        "iso15" => e::ISO_8859_15,
        "iso16" => e::ISO_8859_16,
        "koi8r" => e::KOI8_R,
        "koi8u" => e::KOI8_U,
        "cp1250" => e::WINDOWS_1250,
        "cp1251" => e::WINDOWS_1251,
        "cp1252" => e::WINDOWS_1252,
        "cp1253" => e::WINDOWS_1253,
        "cp1254" => e::WINDOWS_1254,
        "cp1255" => e::WINDOWS_1255,
        "cp1256" => e::WINDOWS_1256,
        "cp1257" => e::WINDOWS_1257,
        "cp1258" => e::WINDOWS_1258,
        "cp874" => e::WINDOWS_874,
        "mac" => e::MACINTOSH,
        "eucjp" => e::EUC_JP,
        "sjis" => e::SHIFT_JIS,
        "gbk" => e::GBK,
        "gb18030" => e::GB18030,
        "big5" => e::BIG5,
        "euckr" => e::EUC_KR,
        _ => return None,
    })
}

/// Windows code pages: libiconv leaves some bytes undefined where WHATWG fills in C1 controls.
pub fn undefined_c1(key: &str) -> bool {
    key.starts_with("cp125") || key == "cp874"
}

/// Looks `name` up (case-insensitive) among the supported character sets.
pub fn lookup(name: &str) -> Option<Kind> {
    let up = name.to_ascii_uppercase();
    for e in entries() {
        if e.names.split(' ').any(|n| n == up) {
            return kind_of(e.key);
        }
    }
    None
}

/// Encoder state: BOM written?, plus the `encoding_rs` encoder for table-driven sets.
pub struct Encoder {
    kind: Kind,
    bom_done: bool,
    rs: Option<encoding_rs::Encoder>,
    key: &'static str,
    /// UCS-2 turns characters beyond the BMP into U+FFFD unless a substitution is set.
    pub strict_bmp: bool,
}

impl Encoder {
    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn new(kind: Kind) -> Encoder {
        let (rs, key) = match kind {
            Kind::Rs(k) => (rs_encoding(k).map(|e| e.new_encoder()), k),
            _ => (None, ""),
        };
        Encoder {
            kind,
            bom_done: false,
            rs,
            key,
            strict_bmp: false,
        }
    }

    /// The bytes for `cp`, or `None` when the character set cannot represent it.
    pub fn encode(&mut self, cp: u32) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let c = char::from_u32(cp)?;
        Some(match self.kind {
            Kind::Ascii => (cp < 0x80).then(|| vec![cp as u8])?,
            Kind::Latin1 => u8::try_from(cp).ok().map(|b| vec![b])?,
            Kind::Iso9 => {
                if let Some((b, _)) = ISO9.iter().find(|(_, x)| *x == cp) {
                    vec![*b]
                } else if cp < 0x100 && !ISO9.iter().any(|(b, _)| u32::from(*b) == cp) {
                    vec![cp as u8]
                } else {
                    return None;
                }
            }
            Kind::Utf8 => c.to_string().into_bytes(),
            Kind::Utf16 {
                le,
                bom_in,
                bom_out,
                pairs,
            } => {
                self.bom(&mut out, bom_out, le, false);
                let cp = if bom_in && cp == 0xfffe { 0xfffd } else { cp };
                let c = char::from_u32(cp)?;
                let mut units = [0u16; 2];
                let us: &[u16] = if cp > 0xffff {
                    if !pairs {
                        if self.strict_bmp {
                            return None;
                        }
                        units[0] = 0xfffd;
                        &units[..1]
                    } else {
                        c.encode_utf16(&mut units)
                    }
                } else {
                    c.encode_utf16(&mut units)
                };
                for u in us {
                    out.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
                }
                out
            }
            Kind::Utf32 { le, bom_out, .. } => {
                self.bom(&mut out, bom_out, le, true);
                out.extend_from_slice(&if le {
                    cp.to_le_bytes()
                } else {
                    cp.to_be_bytes()
                });
                out
            }
            Kind::Rs(_) => {
                if undefined_c1(self.key) && (0x80..=0x9f).contains(&cp) {
                    return None;
                }
                let enc = self.rs.as_mut()?;
                let mut s = [0u8; 4];
                let mut tmp = [0u8; 16];
                let (res, _, n) = enc.encode_from_utf8_without_replacement(
                    c.encode_utf8(&mut s),
                    &mut tmp,
                    false,
                );
                match res {
                    encoding_rs::EncoderResult::InputEmpty => tmp[..n].to_vec(),
                    _ => return None,
                }
            }
        })
    }

    fn bom(&mut self, out: &mut Vec<u8>, want: bool, le: bool, wide: bool) {
        if want && !self.bom_done {
            // libiconv writes the byte order mark in big endian for the unmarked forms.
            let _ = le;
            out.extend_from_slice(if wide {
                &[0, 0, 0xfe, 0xff]
            } else {
                &[0xfe, 0xff]
            });
        }
        self.bom_done = true;
    }
}

/// ISO-8859-9 differs from ISO-8859-1 in these six positions: (byte, code point).
pub const ISO9: [(u8, u32); 6] = [
    (0xd0, 0x11e),
    (0xdd, 0x130),
    (0xde, 0x15e),
    (0xf0, 0x11f),
    (0xfd, 0x131),
    (0xfe, 0x15f),
];
