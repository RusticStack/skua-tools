//! `ps` for Skua Bash, in the layout of Git Bash's (Cygwin 3.6) `ps`, measured: the default
//! (long) table, `-f`, `-s`, `-p PID`, `-u UID`, and `aux`/`-ef`/`-e`/`-a`/`-W` accepted.
//!
//! Skua Bash has no MSYS layer, so every Windows process is listed (as Git Bash's `-W` does),
//! PID = WINPID, PPID = the real parent, PGID = PID, TTY `?`. COMMAND is the image path (`-f`:
//! the command line). Without a matching `-p` PID the table has its header only and the exit
//! status is 1; other filters leave it 0.

#[path = "../procshare/procs.rs"]
mod procs;
#[path = "../procshare/stime.rs"]
mod stime;

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use procs::Proc;
use stime::stime;

const USAGE: &str = "Usage: ps [-aefls] [-u UID] [-p PID]\n\nReport process status\n\n -a, --all       show processes of all users\n -e, --everyone  show processes of all users\n -f, --full      show process uids, ppids and command line\n -h, --help      output usage information and exit\n -l, --long      show process uids, ppids, pgids, winpids\n -p, --process   show information for specified PID\n -s, --summary   show process summary\n -u, --user      list processes owned by UID\n -V, --version   output version information and exit\n -W, --windows   show windows as well as cygwin processes\n\nWith no options, ps outputs the long format by default\n";

const HEAD_LONG: &str = "      PID    PPID    PGID     WINPID   TTY         UID    STIME COMMAND";
const HEAD_FULL: &str = "     UID     PID    PPID  TTY        STIME COMMAND";
const HEAD_SHORT: &str = "    PID  TTY        STIME COMMAND";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Long,
    Full,
    Short,
}

struct Options {
    format: Format,
    pids: Vec<u32>,
    pid_filter: bool,
    uid: Option<u32>,
}

enum Parsed {
    Run(Options),
    Done(u8),
}

fn bad_usage(msg: &str) -> Parsed {
    eprintln!("ps: {msg}");
    eprintln!("Try `ps --help' for more information.");
    Parsed::Done(1)
}

/// `atoi`: Cygwin's `ps` reads `-p`/`-u` values with it, so junk is 0.
fn atoi(s: &str) -> u32 {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0)
}

fn parse(args: impl Iterator<Item = String>) -> Parsed {
    let mut args = args.peekable();
    let (mut long, mut full, mut short) = (false, false, false);
    let mut o = Options {
        format: Format::Long,
        pids: Vec::new(),
        pid_filter: false,
        uid: None,
    };
    let set_pid = |o: &mut Options, v: &str| {
        let pid = atoi(v);
        if pid != 0 {
            o.pids.push(pid);
            o.pid_filter = true;
        }
    };
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if let Some(name) = arg.strip_prefix("--") {
            let (key, value) = match name.split_once('=') {
                Some((k, v)) => (k, Some(v.to_owned())),
                None => (name, None),
            };
            match key {
                "all" | "everyone" | "windows" => {}
                "full" => full = true,
                "long" => long = true,
                "summary" => short = true,
                "help" => {
                    print!("{USAGE}");
                    return Parsed::Done(0);
                }
                "version" => {
                    println!("ps (skua-tools) {}", env!("CARGO_PKG_VERSION"));
                    return Parsed::Done(0);
                }
                "process" | "user" => {
                    let Some(v) = value.or_else(|| args.next()) else {
                        return bad_usage(&format!("option '--{key}' requires an argument"));
                    };
                    if key == "process" {
                        set_pid(&mut o, &v);
                    } else {
                        o.uid = Some(atoi(&v));
                    }
                }
                other => return bad_usage(&format!("unknown option -- {other}")),
            }
            continue;
        }
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            continue; // `aux` and other BSD words: ignored, as Git Bash's ps does
        };
        for (i, c) in flags.char_indices() {
            match c {
                'a' | 'e' | 'W' => {}
                'f' => full = true,
                'l' => long = true,
                's' => short = true,
                'h' => {
                    print!("{USAGE}");
                    return Parsed::Done(0);
                }
                'V' => {
                    println!("ps (skua-tools) {}", env!("CARGO_PKG_VERSION"));
                    return Parsed::Done(0);
                }
                'p' | 'u' => {
                    let rest = &flags[i + 1..];
                    let v = if rest.is_empty() {
                        match args.next() {
                            Some(v) => v,
                            None => {
                                return bad_usage(&format!("option requires an argument -- {c}"));
                            }
                        }
                    } else {
                        rest.to_owned()
                    };
                    if c == 'p' {
                        set_pid(&mut o, &v);
                    } else {
                        o.uid = Some(atoi(&v));
                    }
                    break;
                }
                other => return bad_usage(&format!("unknown option -- {other}")),
            }
        }
    }
    // Measured: -l wins over -f, which wins over -s; no option at all is the long table.
    o.format = if long || (!full && !short) {
        Format::Long
    } else if full {
        Format::Full
    } else {
        Format::Short
    };
    Parsed::Run(o)
}

/// Name shown for a UID: the current user's name, `SYSTEM` for 18, else the number.
fn user_name(uid: u32, own: u32) -> String {
    if uid == own
        && own != 0
        && let Ok(n) = std::env::var("USERNAME")
        && !n.is_empty()
    {
        return n;
    }
    if uid == 18 {
        return "SYSTEM".to_owned();
    }
    uid.to_string()
}

fn row(p: &Proc, format: Format, own: u32, now: u64) -> String {
    let st = stime(p, now);
    match format {
        Format::Long => format!(
            "  {:>7} {:>7} {:>7} {:>10}  {:<9} {:>6} {:>8} {}",
            p.pid,
            p.ppid,
            p.pid,
            p.pid,
            "?",
            p.uid,
            st,
            p.image_path()
        ),
        Format::Full => format!(
            "{:>8} {:>7} {:>7} {:<8} {:>8} {}",
            user_name(p.uid, own),
            p.pid,
            p.ppid,
            "?",
            st,
            p.command_line()
        ),
        Format::Short => format!("{:>7} {:<8} {:>8} {}", p.pid, "?", st, p.image_path()),
    }
}

fn main() -> ExitCode {
    let o = match parse(std::env::args().skip(1)) {
        Parsed::Run(o) => o,
        Parsed::Done(code) => return ExitCode::from(code),
    };
    let sys = procs::system();
    let all = procs::collect(&sys);
    let own = procs::own_uid(&all);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut out = String::new();
    out.push_str(match o.format {
        Format::Long => HEAD_LONG,
        Format::Full => HEAD_FULL,
        Format::Short => HEAD_SHORT,
    });
    out.push('\n');
    let mut rows = 0usize;
    for p in &all {
        if o.pid_filter && !o.pids.contains(&p.pid) {
            continue;
        }
        if o.uid.is_some_and(|u| u != p.uid) {
            continue;
        }
        out.push_str(&row(p, o.format, own, now));
        out.push('\n');
        rows += 1;
    }
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(out.as_bytes()).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::from(u8::from(o.pid_filter && rows == 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, uid: u32) -> Proc {
        Proc {
            pid,
            ppid: 7,
            name: "sleep".to_owned(),
            image: Some("C:\\x\\sleep.exe".to_owned()),
            argv: vec!["C:\\x\\sleep.exe".to_owned(), "20".to_owned()],
            start_ms: 0,
            uid,
        }
    }

    fn opts(args: &[&str]) -> Option<Options> {
        match parse(args.iter().map(|s| (*s).to_owned())) {
            Parsed::Run(o) => Some(o),
            Parsed::Done(_) => None,
        }
    }

    #[test]
    fn long_row_columns_as_git_bash() {
        // Measured: "    44136   26454   44136      16544  ?         197609 23:17:28 /usr/bin/sleep"
        let mut q = p(44136, 197_609);
        q.ppid = 26454;
        let r = row(&q, Format::Long, 197_609, 0);
        let prefix = "    44136   26454   44136      44136  ?         197609 ";
        assert!(r.starts_with(prefix), "{r:?}");
        assert_eq!(
            r.find("C:\\x"),
            Some(HEAD_LONG.find("COMMAND").unwrap_or(0))
        );
    }

    #[test]
    fn short_and_full_rows() {
        let q = p(46123, 5);
        let s = row(&q, Format::Short, 5, 0);
        assert_eq!(
            s.find("C:\\x"),
            Some(HEAD_SHORT.find("COMMAND").unwrap_or(0))
        );
        assert!(s.starts_with("  46123 ?        "), "{s:?}");
        let f = row(&q, Format::Full, 99, 0);
        assert_eq!(
            f.find("sleep 20"),
            Some(HEAD_FULL.find("COMMAND").unwrap_or(0))
        );
    }

    #[test]
    fn option_forms() {
        assert!(opts(&["aux"]).is_some_and(|o| o.format == Format::Long));
        assert!(opts(&["-ef"]).is_some_and(|o| o.format == Format::Full));
        assert!(opts(&["-fl"]).is_some_and(|o| o.format == Format::Long));
        assert!(opts(&["-s"]).is_some_and(|o| o.format == Format::Short));
        assert!(opts(&["-W", "-e", "-a"]).is_some_and(|o| o.format == Format::Long));
        assert!(opts(&["-p", "12"]).is_some_and(|o| o.pids == [12] && o.pid_filter));
        assert!(opts(&["-p12", "--process=13"]).is_some_and(|o| o.pids == [12, 13]));
        assert!(opts(&["-p", "abc"]).is_some_and(|o| !o.pid_filter));
        assert!(opts(&["-u", "5"]).is_some_and(|o| o.uid == Some(5)));
        assert!(opts(&["-x"]).is_none());
        assert!(opts(&["--bogus"]).is_none());
        assert!(opts(&["-p"]).is_none());
        assert!(opts(&["--help"]).is_none());
    }
}
