//! Option parsing for `id` as GNU coreutils 8.32's `id` built for Git for Windows: Cygwin's
//! getopt_long (options may follow the users, long options may be abbreviated, `--` ends the
//! options) with its error texts.

pub const HELP: &str = "Usage: id [OPTION]... [USER]...
Print user and group information for each specified USER,
or (when USER omitted) for the current user.

  -a             ignore, for compatibility with other versions
  -Z, --context  print only the security context of the process
  -g, --group    print only the effective group ID
  -G, --groups   print all group IDs
  -n, --name     print a name instead of a number, for -ugG
  -r, --real     print the real ID instead of the effective ID, with -ugG
  -u, --user     print only the effective user ID
  -z, --zero     delimit entries with NUL characters, not whitespace;
                   not permitted in default format
      --help     display this help and exit
      --version  output version information and exit

Without any OPTION, print some useful set of identified information.
";

/// The options given.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    pub user: bool,
    pub group: bool,
    pub groups: bool,
    pub name: bool,
    pub real: bool,
    pub zero: bool,
    pub users: Vec<String>,
}

/// The result of parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Options),
    Help,
    Version,
    /// A getopt error (`id: <text>` then the Try line, status 1).
    Usage(String),
    /// An `id` error (`id: <text>`, status 1).
    Fatal(String),
}

const LONG: &[(&str, char)] = &[
    ("context", 'Z'),
    ("group", 'g'),
    ("groups", 'G'),
    ("name", 'n'),
    ("real", 'r'),
    ("user", 'u'),
    ("zero", 'z'),
    ("help", 'h'),
    ("version", 'V'),
];

/// Apply one option; `Some` ends parsing.
fn apply(o: &mut Options, c: char) -> Option<Parsed> {
    match c {
        'a' => {}
        'Z' => {
            return Some(Parsed::Fatal(
                "--context (-Z) works only on an SELinux-enabled kernel".to_owned(),
            ));
        }
        'g' => o.group = true,
        'G' => o.groups = true,
        'n' => o.name = true,
        'r' => o.real = true,
        'u' => o.user = true,
        'z' => o.zero = true,
        'h' => return Some(Parsed::Help),
        'V' => return Some(Parsed::Version),
        _ => return Some(Parsed::Usage(format!("unknown option -- {c}"))),
    }
    None
}

/// Parse `args` (without the program name).
pub fn parse(args: &[String]) -> Parsed {
    let mut o = Options::default();
    let mut only_operands = false;
    for a in args {
        if only_operands || a == "-" || !a.starts_with('-') {
            o.users.push(a.clone());
        } else if a == "--" {
            only_operands = true;
        } else if let Some(long) = a.strip_prefix("--") {
            let (typed, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (long, None),
            };
            let exact = LONG.iter().find(|(n, _)| *n == typed);
            let matches: Vec<_> = LONG.iter().filter(|(n, _)| n.starts_with(typed)).collect();
            let c = match (exact, matches.as_slice()) {
                (Some((_, c)), _) => *c,
                (None, [(_, c)]) => *c,
                (None, []) => return Parsed::Usage(format!("unknown option -- {typed}")),
                (None, _) => return Parsed::Usage(format!("ambiguous option -- {typed}")),
            };
            if value.is_some() {
                return Parsed::Usage(format!("option doesn't take an argument -- {typed}"));
            }
            if let Some(p) = apply(&mut o, c) {
                return p;
            }
        } else {
            for c in a.chars().skip(1) {
                // `--help`/`--version` have no short form in GNU id.
                if matches!(c, 'h' | 'V') {
                    return Parsed::Usage(format!("unknown option -- {c}"));
                }
                if let Some(p) = apply(&mut o, c) {
                    return p;
                }
            }
        }
    }
    let only = [o.user, o.group, o.groups].iter().filter(|b| **b).count();
    if only > 1 {
        return Parsed::Fatal("cannot print \"only\" of more than one choice".to_owned());
    }
    if only == 0 && (o.real || o.name) {
        return Parsed::Fatal("cannot print only names or real IDs in default format".to_owned());
    }
    if only == 0 && o.zero {
        return Parsed::Fatal("option --zero not permitted in default format".to_owned());
    }
    Parsed::Run(o)
}
