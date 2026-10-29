//! xxd's own option parser (not getopt): every option is matched by prefix, like the C
//! original (`-bits`, `-b` and `-bp` are all `-b`; `-cols 4`, `-c4` and `-c 4` agree).

pub const USAGE: &str = "Usage:
       xxd [options] [infile [outfile]]
    or
       xxd -r [-s [-]offset] [-c cols] [-ps] [infile [outfile]]
Options:
    -a          toggle autoskip: A single '*' replaces nul-lines. Default off.
    -b          binary digit dump (incompatible with -ps). Default hex.
    -C          capitalize variable names in C include file style (-i).
    -c cols     format <cols> octets per line. Default 16 (-i: 12, -ps: 30).
    -E          show characters in EBCDIC. Default ASCII.
    -e          little-endian dump (incompatible with -ps,-i,-r).
    -g bytes    number of octets per group in normal output. Default 2 (-e: 4).
    -h          print this summary.
    -i          output in C include file style.
    -t          append terminating zero to C include output (-i).
    -l len      stop after <len> octets.
    -n name     set the variable name used in C include output (-i).
    -o off      add <off> to the displayed file position.
    -ps         output in postscript plain hexdump style.
    -r          reverse operation: convert (or patch) hexdump into binary.
    -r -s off   revert with <off> added to file positions found in hexdump.
    -d          show offset in decimal instead of hex.
    -s [+][-]seek  start at <seek> bytes abs. (or +: rel.) infile offset.
    -u          use upper case hex letters.
    -R when     colorize the output; <when> can be 'always', 'auto' or 'never'. Default: 'auto'.
    -v          show version: \"xxd 2026-06-16 by Juergen Weigert et al.\".
";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HexType {
    Normal,
    Postscript,
    Include,
    Bits,
    Little,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug)]
pub struct Seek {
    pub negative: bool,
    pub amount: i64,
}

#[derive(Debug)]
pub struct Opts {
    pub hextype: HexType,
    /// `-b` together with `-i` (0b literals).
    pub bits_include: bool,
    /// Octets per line; always resolved to its default. 0 means unlimited (`-ps -c 0`).
    pub cols: usize,
    /// Octets per group; 0 means one group per line.
    pub group: usize,
    pub upper: bool,
    pub autoskip: bool,
    pub capitalize: bool,
    pub decimal: bool,
    pub ebcdic: bool,
    pub terminate: bool,
    pub revert: bool,
    pub length: Option<u64>,
    pub seek: Option<Seek>,
    pub display_off: i64,
    pub name: Option<String>,
    pub color: Color,
    pub infile: Option<String>,
    pub outfile: Option<String>,
}

pub enum Parsed {
    Run(Opts),
    Version,
}

pub struct Fail {
    pub code: i32,
    pub msg: String,
    pub usage: bool,
}

impl Fail {
    pub fn msg(code: i32, m: impl Into<String>) -> Fail {
        Fail {
            code,
            msg: m.into(),
            usage: false,
        }
    }
    fn usage() -> Fail {
        Fail {
            code: 1,
            msg: String::new(),
            usage: true,
        }
    }
}

/// C `strtol(s, NULL, 0)`: optional blanks and sign, `0x` hex, leading `0` octal, else decimal;
/// stops at the first character that does not fit; no digits is 0.
pub fn strtol(s: &str) -> i64 {
    let t = s.trim_start();
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let (radix, digits) = if let Some(r) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if r.starts_with(|c: char| c.is_ascii_hexdigit()) {
            (16, r)
        } else {
            (10, t)
        }
    } else if t.starts_with('0') {
        (8, t)
    } else {
        (10, t)
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else { break };
        v = v.wrapping_mul(i64::from(radix)).wrapping_add(i64::from(d));
    }
    if neg { v.wrapping_neg() } else { v }
}

struct Cursor<'a> {
    args: &'a [String],
    i: usize,
}

impl Cursor<'_> {
    /// The value of an option: the rest of `pp` after `skip` bytes unless it is empty or just
    /// the long spelling (`long_tail`), else the next argument.
    fn value(&mut self, pp: &str, skip: usize, long_tail: &str) -> Result<String, Fail> {
        let rest = pp.get(skip..).unwrap_or("");
        if !rest.is_empty() && !rest.starts_with(long_tail) {
            return Ok(rest.to_owned());
        }
        self.next_arg()
    }
    fn next_arg(&mut self) -> Result<String, Fail> {
        self.i += 1;
        self.args.get(self.i).cloned().ok_or_else(Fail::usage)
    }
}

fn conflict() -> Fail {
    Fail::msg(1, "only one of -b, -e, -u, -p, -i can be used")
}

pub fn parse(args: &[String]) -> Result<Parsed, Fail> {
    let mut hextype = HexType::Normal;
    let mut cols: Option<i64> = None;
    let mut group: Option<i64> = None;
    let mut o = Opts {
        hextype,
        bits_include: false,
        cols: 0,
        group: 0,
        upper: false,
        autoskip: false,
        capitalize: false,
        decimal: false,
        ebcdic: false,
        terminate: false,
        revert: false,
        length: None,
        seek: None,
        display_off: 0,
        name: None,
        color: Color::Auto,
        infile: None,
        outfile: None,
    };
    let mut cur = Cursor { args, i: 1 };
    while cur.i < args.len() {
        let arg = args[cur.i].as_str();
        let pp = if arg.starts_with("--") && arg.len() > 2 {
            &arg[1..]
        } else {
            arg
        };
        if pp.starts_with("-a") {
            o.autoskip = !o.autoskip;
        } else if pp.starts_with("-b") {
            match hextype {
                HexType::Normal | HexType::Bits => hextype = HexType::Bits,
                HexType::Include => o.bits_include = true,
                _ => return Err(conflict()),
            }
        } else if pp.starts_with("-E") {
            o.ebcdic = true;
        } else if pp.starts_with("-e") {
            match hextype {
                HexType::Normal | HexType::Little => hextype = HexType::Little,
                _ => return Err(conflict()),
            }
        } else if pp.starts_with("-u") {
            o.upper = true;
        } else if pp.starts_with("-p") {
            match hextype {
                HexType::Normal | HexType::Postscript => hextype = HexType::Postscript,
                _ => return Err(conflict()),
            }
        } else if pp.starts_with("-i") {
            match hextype {
                HexType::Normal | HexType::Include => hextype = HexType::Include,
                HexType::Bits => {
                    hextype = HexType::Include;
                    o.bits_include = true;
                }
                _ => return Err(conflict()),
            }
        } else if pp.starts_with("-C") {
            o.capitalize = true;
        } else if pp.starts_with("-d") {
            o.decimal = true;
        } else if pp.starts_with("-r") {
            o.revert = true;
        } else if pp.starts_with("-v") {
            return Ok(Parsed::Version);
        } else if pp.starts_with("-c") {
            if pp.get(2..).unwrap_or("").starts_with("apitalize") {
                o.capitalize = true;
            } else {
                cols = Some(strtol(&cur.value(pp, 2, "ols")?));
            }
        } else if pp.starts_with("-g") {
            group = Some(strtol(&cur.value(pp, 2, "roupsize")?));
        } else if pp.starts_with("-h") {
            return Err(Fail::usage());
        } else if pp.starts_with("-o") {
            o.display_off = strtol(&cur.value(pp, 2, "ffset")?);
        } else if pp.starts_with("-l") {
            let n = strtol(&cur.value(pp, 2, "en")?);
            o.length = u64::try_from(n).ok();
        } else if pp.starts_with("-n") {
            o.name = Some(cur.value(pp, 2, "ame")?);
        } else if pp.starts_with("-s") {
            let tail = pp.get(2..).unwrap_or("");
            let v = if tail.is_empty() || tail.starts_with("eek") || tail.starts_with("kip") {
                cur.next_arg()?
            } else {
                tail.to_owned()
            };
            let v = v.strip_prefix('+').unwrap_or(&v);
            let (negative, num) = match v.strip_prefix('-') {
                Some(r) => (true, r),
                None => (false, v),
            };
            o.seek = Some(Seek {
                negative,
                amount: strtol(num),
            });
        } else if pp.starts_with("-t") {
            o.terminate = true;
        } else if pp.starts_with("-R") {
            let when = if pp.len() > 2 {
                pp.get(2..).unwrap_or("").to_owned()
            } else {
                cur.next_arg()?
            };
            o.color = if when.starts_with("always") {
                Color::Always
            } else if when.starts_with("never") {
                Color::Never
            } else if when.starts_with("auto") {
                Color::Auto
            } else {
                return Err(Fail::usage());
            };
        } else if pp == "--" {
            cur.i += 1;
            break;
        } else if pp.starts_with('-') && pp.len() > 1 {
            return Err(Fail::usage());
        } else {
            break;
        }
        cur.i += 1;
    }
    let rest = &args[cur.i.min(args.len())..];
    if rest.len() > 2 {
        return Err(Fail::usage());
    }
    o.infile = rest.first().cloned();
    o.outfile = rest.get(1).cloned();
    o.hextype = hextype;
    resolve(&mut o, cols, group)?;
    Ok(Parsed::Run(o))
}

/// Fills in the per-layout defaults and checks the ranges.
fn resolve(o: &mut Opts, cols: Option<i64>, group: Option<i64>) -> Result<(), Fail> {
    let default_cols = match o.hextype {
        HexType::Normal | HexType::Little => 16,
        HexType::Bits => 6,
        HexType::Postscript => 30,
        HexType::Include if o.bits_include => 6,
        HexType::Include => 12,
    };
    o.cols = match (o.hextype, cols) {
        (_, None) => default_cols,
        (HexType::Postscript, Some(c)) => usize::try_from(c).unwrap_or(0),
        (HexType::Include, Some(c)) => usize::try_from(c)
            .ok()
            .filter(|c| *c > 0)
            .unwrap_or(default_cols),
        (_, Some(0)) => default_cols,
        (_, Some(c)) if (1..=256).contains(&c) => c as usize,
        _ => return Err(Fail::msg(1, "invalid number of columns (max. 256).")),
    };
    let default_group = match o.hextype {
        HexType::Little => 4,
        HexType::Bits => 1,
        _ => 2,
    };
    o.group = match group {
        None => default_group,
        Some(g) if g < 0 => default_group,
        Some(g) => g as usize,
    };
    if o.hextype == HexType::Little && o.group != 0 && !o.group.is_power_of_two() {
        return Err(Fail::msg(
            1,
            "number of octets per group must be a power of 2 with -e.",
        ));
    }
    Ok(())
}
