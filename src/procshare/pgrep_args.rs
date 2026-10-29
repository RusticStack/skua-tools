//! Command line of `pgrep` / `pkill` (procps-ng): options, signal names, usage errors.
//! Usage errors exit 2; `Try '<tool> --help'` names the bare program.

use std::process::ExitCode;

/// What a `pgrep`/`pkill` run was asked for.
#[derive(Default)]
pub struct Opts {
    pub pattern: Option<String>,
    pub full: bool,
    pub list_name: bool,
    pub list_full: bool,
    pub exact: bool,
    pub icase: bool,
    pub newest: bool,
    pub oldest: bool,
    pub count: bool,
    pub inverse: bool,
    pub echo: bool,
    pub delim: Option<String>,
    pub euid: Vec<String>,
    pub uid: Vec<String>,
    pub parent: Vec<u32>,
    pub pgroup: Vec<u32>,
    /// `-s`, `-t`, `-G`: Windows has no sessions, terminals or groups, so nothing matches.
    pub never: bool,
    pub older: Option<u64>,
    pub pidfile: Option<String>,
    pub signal: Option<i32>,
}

impl Opts {
    /// True when something besides the pattern selects processes.
    pub fn has_filter(&self) -> bool {
        !(self.euid.is_empty()
            && self.uid.is_empty()
            && self.parent.is_empty()
            && self.pgroup.is_empty()
            && !self.never
            && self.older.is_none()
            && self.pidfile.is_none())
    }
}

pub const SIGNALS: [(&str, i32); 31] = [
    ("HUP", 1),
    ("INT", 2),
    ("QUIT", 3),
    ("ILL", 4),
    ("TRAP", 5),
    ("ABRT", 6),
    ("BUS", 7),
    ("FPE", 8),
    ("KILL", 9),
    ("USR1", 10),
    ("SEGV", 11),
    ("USR2", 12),
    ("PIPE", 13),
    ("ALRM", 14),
    ("TERM", 15),
    ("STKFLT", 16),
    ("CHLD", 17),
    ("CONT", 18),
    ("STOP", 19),
    ("TSTP", 20),
    ("TTIN", 21),
    ("TTOU", 22),
    ("URG", 23),
    ("XCPU", 24),
    ("XFSZ", 25),
    ("VTALRM", 26),
    ("PROF", 27),
    ("WINCH", 28),
    ("IO", 29),
    ("PWR", 30),
    ("SYS", 31),
];

/// `9`, `KILL` or `SIGKILL` (any case) to the signal number.
pub fn signal_number(s: &str) -> Option<i32> {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        return s.parse::<i32>().ok().filter(|n| (0..=64).contains(n));
    }
    let up = s.to_ascii_uppercase();
    let name = up.strip_prefix("SIG").unwrap_or(&up);
    SIGNALS.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn usage_text(prog: &str, kill: bool) -> String {
    let sig = if kill {
        " -<sig>, --signal <sig>    signal to send (either number or name)\n -e, --echo               display what is killed\n"
    } else {
        " -d, --delimiter <string> specify output delimiter\n -l, --list-name         list PID and process name\n -a, --list-full         list PID and full command line\n"
    };
    format!(
        "\nUsage:\n {prog} [options] <pattern>\n\nOptions:\n{sig} -c, --count              count of matching processes\n -f, --full               use full process name to match\n -g, --pgroup <PGID,...>  match listed process group IDs\n -i, --ignore-case        match case insensitively\n -n, --newest             select most recently started\n -o, --oldest             select least recently started\n -O, --older <seconds>    select where older than seconds\n -P, --parent <PPID,...>  match only child processes of the given parent\n -u, --euid <ID,...>      match by effective IDs\n -U, --uid <ID,...>       match by real IDs\n -v, --inverse            negates the matching\n -x, --exact              match exactly with the command name\n -F, --pidfile <file>     read PIDs from file\n\n -h, --help     display this help and exit\n -V, --version  output version information and exit\n\nFor more details see {prog}(1).\n"
    )
}

pub fn fail(prog: &str, msg: &str) -> ExitCode {
    eprintln!("{prog}: {msg}");
    eprintln!("Try '{prog} --help' for more information.");
    ExitCode::from(2)
}

fn numbers(prog: &str, what: &str, list: &str) -> Result<Vec<u32>, ExitCode> {
    list.split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| fail(prog, &format!("Invalid {what} ID: {s}")))
        })
        .collect()
}

/// (short, long, takes an argument)
const OPTIONS: [(char, &str, bool); 24] = [
    ('c', "count", false),
    ('d', "delimiter", true),
    ('e', "echo", false),
    ('f', "full", false),
    ('g', "pgroup", true),
    ('G', "group", true),
    ('i', "ignore-case", false),
    ('l', "list-name", false),
    ('a', "list-full", false),
    ('n', "newest", false),
    ('o', "oldest", false),
    ('O', "older", true),
    ('P', "parent", true),
    ('s', "session", true),
    ('t', "terminal", true),
    ('u', "euid", true),
    ('U', "uid", true),
    ('v', "inverse", false),
    ('w', "lightweight", false),
    ('x', "exact", false),
    ('F', "pidfile", true),
    ('L', "logpidfile", false),
    ('q', "queue", true),
    ('S', "signal", true),
];

fn apply(prog: &str, o: &mut Opts, key: char, value: &str) -> Result<(), ExitCode> {
    match key {
        'c' => o.count = true,
        'd' => o.delim = Some(value.to_owned()),
        'e' => o.echo = true,
        'f' => o.full = true,
        'g' => o.pgroup.extend(numbers(prog, "process group", value)?),
        'G' | 's' | 't' => o.never = true,
        'i' => o.icase = true,
        'l' => o.list_name = true,
        'a' => o.list_full = true,
        'n' => o.newest = true,
        'o' => o.oldest = true,
        'O' => {
            o.older = Some(
                value
                    .parse::<u64>()
                    .map_err(|_| fail(prog, &format!("Invalid age: {value}")))?,
            );
        }
        'P' => o.parent.extend(numbers(prog, "parent", value)?),
        'u' => o.euid.extend(value.split(',').map(str::to_owned)),
        'U' => o.uid.extend(value.split(',').map(str::to_owned)),
        'v' => o.inverse = true,
        'x' => o.exact = true,
        'F' => o.pidfile = Some(value.to_owned()),
        'S' => {
            o.signal = Some(
                signal_number(value)
                    .ok_or_else(|| fail(prog, &format!("Unknown signal: {value}")))?,
            );
        }
        _ => {} // w, L, q: accepted, no effect
    }
    Ok(())
}

/// Parses `args` (after argv[0]) for `pgrep` (`kill` false) or `pkill`. `Ok(None)`: help or
/// version was printed.
pub fn parse(prog: &str, kill: bool, args: Vec<String>) -> Result<Option<Opts>, ExitCode> {
    let mut o = Opts::default();
    let mut it = args.into_iter().enumerate().peekable();
    let mut only_words = false;
    while let Some((idx, arg)) = it.next() {
        if only_words || arg == "-" || !arg.starts_with('-') {
            if o.pattern.is_some() {
                return Err(fail(prog, "only one pattern can be provided"));
            }
            o.pattern = Some(arg);
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        // pkill: a leading `-9` / `-TERM` is the signal (procps reads argv[1] that way).
        if kill
            && idx == 0
            && let Some(sig) = signal_number(&arg[1..])
        {
            o.signal = Some(sig);
            continue;
        }
        if let Some(name) = arg.strip_prefix("--") {
            let (key, inline) = match name.split_once('=') {
                Some((k, v)) => (k, Some(v.to_owned())),
                None => (name, None),
            };
            match key {
                "help" => {
                    print!("{}", usage_text(prog, kill));
                    return Ok(None);
                }
                "version" => {
                    println!("{prog} (skua-tools) {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                _ => {}
            }
            let Some(&(short, _, takes)) = OPTIONS.iter().find(|(_, l, _)| *l == key) else {
                return Err(fail(prog, &format!("unrecognized option '--{key}'")));
            };
            if !kill && matches!(short, 'S' | 'e' | 'q') {
                return Err(fail(prog, &format!("unrecognized option '--{key}'")));
            }
            let value = if takes {
                match inline.or_else(|| it.next().map(|(_, v)| v)) {
                    Some(v) => v,
                    None => {
                        return Err(fail(
                            prog,
                            &format!("option '--{key}' requires an argument"),
                        ));
                    }
                }
            } else {
                String::new()
            };
            apply(prog, &mut o, short, &value)?;
            continue;
        }
        let flags = &arg[1..];
        for (i, c) in flags.char_indices() {
            match c {
                'h' | '?' => {
                    print!("{}", usage_text(prog, kill));
                    return Ok(None);
                }
                'V' => {
                    println!("{prog} (skua-tools) {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                _ => {}
            }
            let known = OPTIONS
                .iter()
                .find(|(s, _, _)| *s == c && c != 'S')
                .filter(|(s, _, _)| kill || !matches!(s, 'e' | 'q'));
            let Some(&(short, _, takes)) = known else {
                return Err(fail(prog, &format!("invalid option -- '{c}'")));
            };
            if takes {
                let rest = &flags[i + c.len_utf8()..];
                let value = if rest.is_empty() {
                    match it.next() {
                        Some((_, v)) => v,
                        None => {
                            return Err(fail(
                                prog,
                                &format!("option requires an argument -- '{c}'"),
                            ));
                        }
                    }
                } else {
                    rest.to_owned()
                };
                apply(prog, &mut o, short, &value)?;
                break;
            }
            apply(prog, &mut o, short, "")?;
        }
    }
    if o.pattern.is_none() && !o.has_filter() {
        return Err(fail(prog, "no matching criteria specified"));
    }
    Ok(Some(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kill: bool, args: &[&str]) -> Option<Opts> {
        parse(
            if kill { "pkill" } else { "pgrep" },
            kill,
            args.iter().map(|s| (*s).to_owned()).collect(),
        )
        .ok()
        .flatten()
    }

    #[test]
    fn signal_names_and_numbers() {
        assert_eq!(signal_number("9"), Some(9));
        assert_eq!(signal_number("KILL"), Some(9));
        assert_eq!(signal_number("sigterm"), Some(15));
        assert_eq!(signal_number("0"), Some(0));
        assert_eq!(signal_number("99"), None);
        assert_eq!(signal_number("FOO"), None);
        assert_eq!(signal_number(""), None);
    }

    #[test]
    fn pgrep_options() {
        let o = run(false, &["-fl", "node.*srv"]);
        assert!(
            o.is_some_and(|o| o.full && o.list_name && o.pattern.as_deref() == Some("node.*srv"))
        );
        let o = run(false, &["-d", ",", "-x", "bash"]).unwrap_or_default();
        assert_eq!(o.delim.as_deref(), Some(","));
        assert!(o.exact);
        let o = run(false, &["x", "-u", "root,5"]).unwrap_or_default();
        assert_eq!(o.euid, ["root", "5"]);
        let o = run(false, &["--parent=1,2", "--ignore-case"]).unwrap_or_default();
        assert_eq!(o.parent, [1, 2]);
        assert!(o.icase);
        assert!(run(false, &[]).is_none());
        assert!(run(false, &["a", "b"]).is_none());
        assert!(run(false, &["-z", "a"]).is_none());
        assert!(run(false, &["-e", "a"]).is_none());
        assert!(run(false, &["-d"]).is_none());
        assert!(run(false, &["-P", "x"]).is_none());
        assert!(run(false, &["-h"]).is_none());
    }

    #[test]
    fn pkill_signals() {
        assert_eq!(run(true, &["-9", "x"]).and_then(|o| o.signal), Some(9));
        assert_eq!(run(true, &["-KILL", "x"]).and_then(|o| o.signal), Some(9));
        assert_eq!(
            run(true, &["--signal", "HUP", "x"]).and_then(|o| o.signal),
            Some(1)
        );
        assert_eq!(
            run(true, &["-f", "-TERM", "x"]).and_then(|o| o.signal),
            None
        );
        assert!(run(true, &["-e", "x"]).is_some_and(|o| o.echo));
        assert!(run(true, &["--signal", "FOO", "x"]).is_none());
    }
}
