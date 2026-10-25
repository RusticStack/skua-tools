//! `clear` for Skua Bash, as Git Bash's ncurses `clear` behaves (measured with Git for Windows'
//! ncurses 6.6): home the cursor, erase the screen and the scrollback.
//!
//! - `xterm*` (also `xterm-direct`, `xterm-256color`), `ms-terminal` and any other name: `\e[H\e[2J\e[3J`
//!   (`-x`: no `\e[3J`). Git Bash's small terminfo database says "unknown terminal type" for
//!   names it lacks (`tmux-256color`, `alacritty`, `linux`, ...); Skua's clears them like xterm,
//!   because every terminal Skua runs in understands those sequences.
//! - `xterm-color`: `\e[H\e[2J` (no scrollback capability in its terminfo entry).
//! - `cygwin`, `screen*`: `\e[H\e[J`.
//! - `dumb`: nothing, exit 1. `TERM` unset: `TERM environment variable not set.`; empty: `'':
//!   unknown terminal type.`; both exit 1.
//! - `-T TERM` overrides `$TERM`; `-V` prints the version; any other argument prints the usage
//!   and exits 1.

use std::io::Write;
use std::process::ExitCode;

const USAGE: &str = "Usage: clear [options]\n\nOptions:\n  -T TERM     use this instead of $TERM\n  -V          print curses-version\n  -x          do not try to clear scrollback\n";

/// The bytes that clear the screen for `term`, or `None` for a terminal that cannot clear.
fn sequence(term: &str, scrollback: bool) -> Option<&'static [u8]> {
    if term == "dumb" {
        return None;
    }
    if term == "cygwin" || term.starts_with("screen") {
        return Some(b"\x1b[H\x1b[J");
    }
    if term == "xterm-color" || !scrollback {
        return Some(b"\x1b[H\x1b[2J");
    }
    Some(b"\x1b[H\x1b[2J\x1b[3J")
}

fn usage_error(msg: Option<String>) -> ExitCode {
    let mut err = std::io::stderr();
    if let Some(msg) = msg {
        let _ = writeln!(err, "clear: {msg}");
    }
    let _ = err.write_all(USAGE.as_bytes());
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let mut term: Option<String> = None;
    let mut scrollback = true;
    let mut args = std::env::args().skip(1);
    // Multicall form (`skua-tools.exe clear ARGS`) is not needed: the bundle links `clear.exe`.
    while let Some(arg) = args.next() {
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            return usage_error(None);
        };
        let chars = flags.char_indices();
        for (i, c) in chars {
            match c {
                'x' => scrollback = false,
                'V' => {
                    println!("skua-tools clear {}", env!("CARGO_PKG_VERSION"));
                    return ExitCode::SUCCESS;
                }
                'T' => {
                    let rest = &flags[i + 1..];
                    if rest.is_empty() {
                        match args.next() {
                            Some(t) => term = Some(t),
                            None => {
                                return usage_error(Some(
                                    "option requires an argument -- T".to_owned(),
                                ));
                            }
                        }
                    } else {
                        term = Some(rest.to_owned());
                    }
                    break;
                }
                other => return usage_error(Some(format!("unknown option -- {other}"))),
            }
        }
    }
    let term = match term.or_else(|| std::env::var("TERM").ok()) {
        Some(t) => t,
        None => {
            eprintln!("TERM environment variable not set.");
            return ExitCode::from(1);
        }
    };
    if term.is_empty() {
        eprintln!("'': unknown terminal type.");
        return ExitCode::from(1);
    }
    let Some(seq) = sequence(&term, scrollback) else {
        return ExitCode::from(1);
    };
    let mut out = std::io::stdout().lock();
    if out.write_all(seq).and_then(|()| out.flush()).is_err() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::sequence;

    #[test]
    fn sequences_as_git_bash() {
        let full: &[u8] = b"\x1b[H\x1b[2J\x1b[3J";
        assert_eq!(sequence("xterm-256color", true), Some(full));
        assert_eq!(sequence("xterm", true), Some(full));
        assert_eq!(sequence("ms-terminal", true), Some(full));
        assert_eq!(
            sequence("xterm-256color", false),
            Some(&b"\x1b[H\x1b[2J"[..])
        );
        assert_eq!(sequence("xterm-color", true), Some(&b"\x1b[H\x1b[2J"[..]));
        assert_eq!(sequence("cygwin", true), Some(&b"\x1b[H\x1b[J"[..]));
        assert_eq!(
            sequence("screen-256color", true),
            Some(&b"\x1b[H\x1b[J"[..])
        );
        assert_eq!(sequence("dumb", true), None);
    }
}
