//! The iconv command line (libiconv style: options before the files, exact spellings).

#[derive(Default)]
pub struct Opts {
    pub from: Option<String>,
    pub to: Option<String>,
    pub discard: bool,
    pub silent: bool,
    pub list: bool,
    pub help: bool,
    pub version: bool,
    pub unicode_subst: Option<String>,
    pub byte_subst: Option<String>,
    pub widechar_subst: Option<String>,
    pub files: Vec<String>,
}

pub fn parse_args(args: &[String]) -> Result<Opts, ()> {
    let mut o = Opts::default();
    if args.len() == 2 && (args[1] == "-l" || args[1] == "--list") {
        o.list = true;
        return Ok(o);
    }
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a == "-" {
            break;
        }
        i += 1;
        if let Some(long) = a.strip_prefix("--") {
            let (name, val) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_owned())),
                None => (long, None),
            };
            let takes = matches!(
                name,
                "from-code" | "to-code" | "unicode-subst" | "byte-subst" | "widechar-subst"
            );
            let val = if takes && val.is_none() {
                let v = args.get(i).cloned().ok_or(())?;
                i += 1;
                Some(v)
            } else {
                val
            };
            match (name, val) {
                ("from-code", Some(v)) => o.from = Some(v),
                ("to-code", Some(v)) => o.to = Some(v),
                ("unicode-subst", Some(v)) => o.unicode_subst = Some(v),
                ("byte-subst", Some(v)) => o.byte_subst = Some(v),
                ("widechar-subst", Some(v)) => o.widechar_subst = Some(v),
                ("silent", None) => o.silent = true,
                ("help", None) => o.help = true,
                ("version", None) => o.version = true,
                _ => return Err(()),
            }
            if o.help || o.version {
                // Sequential: the first of --help / --version wins; later options are not read.
                return Ok(o);
            }
            continue;
        }
        match a {
            "-f" | "-t" => {
                let v = args.get(i).cloned().ok_or(())?;
                i += 1;
                if a == "-f" {
                    o.from = Some(v)
                } else {
                    o.to = Some(v)
                }
            }
            _ if a[1..].chars().all(|c| c == 'c' || c == 's') => {
                o.discard |= a.contains('c');
                o.silent |= a.contains('s');
            }
            _ => return Err(()),
        }
    }
    o.files = args[i..].to_vec();
    Ok(o)
}
