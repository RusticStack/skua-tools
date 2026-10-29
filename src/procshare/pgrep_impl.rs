//! `pgrep` and `pkill` (procps-ng semantics) over Windows processes. Exit status: 0 something
//! matched, 1 nothing matched, 2 usage error, 3 fatal. Never reads stdin, so it cannot hang an
//! agent. Match target: the image name without `.exe` (`-f`: the command line); the regex is
//! case-sensitive unless `-i`; `-x` anchors it. Never matches itself or any ancestor, and
//! so never signals the shell that started it.

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;

use regex_lite::RegexBuilder;
use sysinfo::Pid;

use crate::pgrep_args::{Opts, fail, parse};
use crate::procs::{self, Proc};

/// What a signal does on Windows.
enum Effect {
    Check,
    Terminate,
    Ignored,
    Unsupported,
}

fn effect(signal: i32) -> Effect {
    match signal {
        0 => Effect::Check,
        17 | 18 | 23 | 28 => Effect::Ignored, // CHLD CONT URG WINCH: default action is ignore
        19..=22 => Effect::Unsupported,       // STOP TSTP TTIN TTOU
        _ => Effect::Terminate,
    }
}

/// UID numbers named by `-u`/`-U` values: numbers, the current user's name, `SYSTEM`.
fn user_ids(prog: &str, values: &[String], own: u32) -> Result<HashSet<u32>, ExitCode> {
    let me = std::env::var("USERNAME").unwrap_or_default();
    let mut set = HashSet::new();
    for v in values.iter().filter(|v| !v.is_empty()) {
        if let Ok(n) = v.parse::<u32>() {
            set.insert(n);
        } else if !me.is_empty() && v.eq_ignore_ascii_case(&me) {
            set.insert(own);
        } else if v.eq_ignore_ascii_case("SYSTEM") {
            set.insert(18);
        } else {
            return Err(fail(prog, &format!("invalid user name: {v}")));
        }
    }
    Ok(set)
}

fn pidfile_pids(prog: &str, path: &str) -> Result<HashSet<u32>, ExitCode> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Err(fail(prog, "pidfile not valid"));
    };
    let pids: HashSet<u32> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect();
    if pids.is_empty() {
        return Err(fail(prog, "pidfile not valid"));
    }
    Ok(pids)
}

/// Processes selected by `o`, oldest PID first (before `-n`/`-o` narrowing).
fn select<'a>(
    prog: &str,
    o: &Opts,
    all: &'a [Proc],
    skip: &HashSet<u32>,
    now: u64,
) -> Result<Vec<&'a Proc>, ExitCode> {
    let own = procs::own_uid(all);
    let euid = user_ids(prog, &o.euid, own)?;
    let ruid = user_ids(prog, &o.uid, own)?;
    let from_file = match &o.pidfile {
        Some(path) => Some(pidfile_pids(prog, path)?),
        None => None,
    };
    let re = match &o.pattern {
        Some(p) => {
            let src = if o.exact {
                format!("^(?:{p})$")
            } else {
                p.clone()
            };
            let built = RegexBuilder::new(&src).case_insensitive(o.icase).build();
            match built {
                Ok(re) => Some(re),
                Err(e) => return Err(fail(prog, &format!("regex error: {e}"))),
            }
        }
        None => None,
    };
    let mut picked: Vec<&Proc> = all
        .iter()
        .filter(|p| !skip.contains(&p.pid))
        .filter(|p| {
            let mut m = !o.never
                && (euid.is_empty() || euid.contains(&p.uid))
                && (ruid.is_empty() || ruid.contains(&p.uid))
                && (o.parent.is_empty() || o.parent.contains(&p.ppid))
                && (o.pgroup.is_empty() || o.pgroup.contains(&p.pid))
                && from_file.as_ref().is_none_or(|f| f.contains(&p.pid))
                && o.older
                    .is_none_or(|s| now.saturating_sub(p.start_secs()) > s);
            if m && let Some(re) = &re {
                m = if o.full {
                    re.is_match(&p.command_line()) || re.is_match(&p.argv.join(" "))
                } else {
                    re.is_match(&p.name)
                };
            }
            m != o.inverse
        })
        .collect();
    if o.newest {
        picked.sort_by_key(|p| (p.start_ms, p.pid));
        picked = picked.pop().into_iter().collect();
    } else if o.oldest {
        picked.sort_by_key(|p| (p.start_ms, p.pid));
        picked.truncate(1);
    }
    Ok(picked)
}

/// This process and every ancestor (the shell whose command line carries the pattern, the agent
/// that started it): never reported, never signalled.
fn excluded(all: &[Proc]) -> HashSet<u32> {
    let me = std::process::id();
    let mut set = HashSet::from([me]);
    let mut cur = me;
    while let Some(p) = all.iter().find(|p| p.pid == cur) {
        if p.ppid == 0 || !set.insert(p.ppid) {
            break;
        }
        cur = p.ppid;
    }
    set
}

pub fn run(kill: bool) -> ExitCode {
    let prog = if kill { "pkill" } else { "pgrep" };
    let o = match parse(prog, kill, std::env::args().skip(1).collect()) {
        Ok(Some(o)) => o,
        Ok(None) => return ExitCode::SUCCESS,
        Err(code) => return code,
    };
    let sys = procs::system();
    let all = procs::collect(&sys);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let skip = excluded(&all);
    let picked = match select(prog, &o, &all, &skip, now) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let mut out = String::new();
    let mut status = u8::from(picked.is_empty());
    if kill {
        let signal = o.signal.unwrap_or(15);
        let mut killed = 0usize;
        for p in &picked {
            let ok = match effect(signal) {
                Effect::Check | Effect::Ignored => true,
                Effect::Terminate => sys.process(Pid::from_u32(p.pid)).is_some_and(|q| q.kill()),
                Effect::Unsupported => {
                    eprintln!("{prog}: signal {signal} is not supported on Windows");
                    return ExitCode::from(3);
                }
            };
            if ok {
                killed += 1;
                if o.echo {
                    out.push_str(&format!("{} killed (pid {})\n", p.name, p.pid));
                }
            } else {
                eprintln!(
                    "{prog}: killing pid {} failed: Operation not permitted",
                    p.pid
                );
            }
        }
        if o.count {
            out.push_str(&format!("{}\n", picked.len()));
        }
        if !picked.is_empty() && killed == 0 {
            status = 1;
        }
    } else if o.count {
        out.push_str(&format!("{}\n", picked.len()));
    } else if !picked.is_empty() {
        let delim = o.delim.as_deref().unwrap_or("\n");
        let items: Vec<String> = picked
            .iter()
            .map(|p| {
                if o.list_full {
                    format!("{} {}", p.pid, p.command_line())
                } else if o.list_name {
                    format!("{} {}", p.pid, p.name)
                } else {
                    p.pid.to_string()
                }
            })
            .collect();
        out.push_str(&items.join(delim));
        out.push('\n');
    }
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(out.as_bytes())
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::from(status);
    }
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, name: &str, cmd: &str, start: u64) -> Proc {
        Proc {
            pid,
            ppid,
            name: name.to_owned(),
            image: Some(format!("C:\\bin\\{name}.exe")),
            argv: cmd
                .split(' ')
                .enumerate()
                .map(|(i, a)| {
                    if i == 0 {
                        format!("C:\\bin\\{a}.exe")
                    } else {
                        a.to_owned()
                    }
                })
                .collect(),
            start_ms: start * 1000,
            uid: 5,
        }
    }

    fn opts(pattern: &str) -> Opts {
        Opts {
            pattern: Some(pattern.to_owned()),
            ..Opts::default()
        }
    }

    fn ids(v: &[&Proc]) -> Vec<u32> {
        v.iter().map(|p| p.pid).collect()
    }

    fn sample() -> Vec<Proc> {
        vec![
            p(10, 1, "node", "node server.js", 100),
            p(11, 1, "nodemon", "nodemon -x server.js", 200),
            p(12, 10, "Node", "node worker.js", 300),
            p(13, 1, "bash", "bash -c pgrep node", 400),
        ]
    }

    #[test]
    fn name_regex_substring_and_exact() {
        let all = sample();
        let none = HashSet::new();
        assert_eq!(
            ids(&select("pgrep", &opts("node"), &all, &none, 1000).unwrap_or_default()),
            [10, 11]
        );
        let mut x = opts("node");
        x.exact = true;
        assert_eq!(
            ids(&select("pgrep", &x, &all, &none, 1000).unwrap_or_default()),
            [10]
        );
        let mut i = opts("^node$");
        i.icase = true;
        assert_eq!(
            ids(&select("pgrep", &i, &all, &none, 1000).unwrap_or_default()),
            [10, 12]
        );
        assert_eq!(
            ids(&select("pgrep", &opts("node|bash"), &all, &none, 1000).unwrap_or_default()),
            [10, 11, 13]
        );
    }

    #[test]
    fn full_matches_command_line() {
        let all = sample();
        let none = HashSet::new();
        let mut f = opts("server\\.js");
        f.full = true;
        assert_eq!(
            ids(&select("pgrep", &f, &all, &none, 1000).unwrap_or_default()),
            [10, 11]
        );
        assert!(
            select("pgrep", &opts("server"), &all, &none, 1000)
                .unwrap_or_default()
                .is_empty()
        );
    }

    #[test]
    fn newest_oldest_inverse_skip_parent() {
        let all = sample();
        let skip = HashSet::from([13u32]);
        let mut n = opts("^n");
        n.icase = true;
        n.newest = true;
        assert_eq!(
            ids(&select("pgrep", &n, &all, &skip, 1000).unwrap_or_default()),
            [12]
        );
        let mut o = opts("^n");
        o.oldest = true;
        o.icase = true;
        assert_eq!(
            ids(&select("pgrep", &o, &all, &skip, 1000).unwrap_or_default()),
            [10]
        );
        let mut v = opts("node");
        v.inverse = true;
        assert_eq!(
            ids(&select("pgrep", &v, &all, &skip, 1000).unwrap_or_default()),
            [12]
        );
    }

    #[test]
    fn filters() {
        let all = sample();
        let none = HashSet::new();
        let mut par = opts("node");
        par.parent = vec![10];
        par.icase = true;
        assert_eq!(
            ids(&select("pgrep", &par, &all, &none, 1000).unwrap_or_default()),
            [12]
        );
        let older = Opts {
            older: Some(750),
            ..Opts::default()
        };
        assert_eq!(
            ids(&select("pgrep", &older, &all, &none, 1000).unwrap_or_default()),
            [10, 11]
        );
        let mut u = opts("node");
        u.euid = vec!["6".to_owned()];
        assert!(
            select("pgrep", &u, &all, &none, 1000)
                .unwrap_or_default()
                .is_empty()
        );
        u.euid = vec!["5".to_owned()];
        assert_eq!(
            ids(&select("pgrep", &u, &all, &none, 1000).unwrap_or_default()),
            [10, 11]
        );
    }

    #[test]
    fn bad_regex_is_usage_error() {
        let all = sample();
        assert!(select("pgrep", &opts("("), &all, &HashSet::new(), 0).is_err());
    }

    #[test]
    fn ancestors_excluded() {
        let me = std::process::id();
        let all = vec![
            p(me, 50, "pgrep", "pgrep x", 1),
            p(50, 60, "bash", "bash -c x", 1),
            p(60, 0, "term", "term", 1),
        ];
        assert_eq!(excluded(&all), HashSet::from([me, 50, 60]));
    }
}
