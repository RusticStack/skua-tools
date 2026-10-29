//! `start` for Skua Bash: Git Bash's `/usr/bin/start` runs `cmd //c start "${@//&/^&}"`. This is
//! the same with complete escaping: `cmd.exe /c start "" <args>`; `/c/x/y` becomes `C:\x\y` (as
//! the MSYS argument conversion did); never waits for the opened program.
//!
//! `SKUA_START_DRY_RUN=1` prints the command line instead of running it. No arguments: usage on
//! stderr, exit 1 (Git Bash's would open an empty console window). A first argument `""` is
//! taken as the window title (not doubled). cmd.exe's own status is passed through.

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, ExitCode};

const USAGE: &str = "Usage: start <url|file|dir|program> [args]\n";

/// MSYS drive path (`/c/x`, `/c/`) to a Windows path; drive paths with `/` get `\`.
/// URLs and everything else are unchanged.
fn convert(arg: &str) -> String {
    let b = arg.as_bytes();
    let drive = b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/';
    let win = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'/' || b[2] == b'\\');
    if drive {
        let mut s = format!("{}:", (b[1] as char).to_ascii_uppercase());
        s.push_str(&arg[2..].replace('/', "\\"));
        s
    } else if win && !arg.contains("://") {
        arg.replace('/', "\\")
    } else {
        arg.to_owned()
    }
}

/// One argument as cmd.exe must see it. Plain words stay bare; an argument with a space or any
/// cmd metacharacter is double-quoted (the metacharacters are literal inside quotes; an embedded
/// `"` is doubled). `%VAR%` is still expanded by cmd.exe, as it is for Git Bash's `start`.
fn quote(arg: &str) -> String {
    let special = |c: char| {
        matches!(
            c,
            ' ' | '\t' | ',' | ';' | '=' | '&' | '|' | '<' | '>' | '^' | '(' | ')' | '!' | '"'
        )
    };
    if arg.is_empty() || arg.chars().any(special) {
        format!("\"{}\"", arg.replace('"', "\"\""))
    } else {
        arg.to_owned()
    }
}

/// The text after `cmd.exe /c `.
fn command_line(args: &[String]) -> String {
    let mut line = String::from("start");
    if args.first().map(String::as_str) != Some("") {
        line.push_str(" \"\"");
    }
    for a in args {
        line.push(' ');
        line.push_str(&quote(&convert(a)));
    }
    line
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.is_empty() {
        let _ = std::io::stderr().write_all(USAGE.as_bytes());
        return ExitCode::from(1);
    }
    let line = command_line(&args);
    if std::env::var_os("SKUA_START_DRY_RUN").is_some_and(|v| !v.is_empty() && v != "0") {
        let _ = writeln!(std::io::stdout(), "cmd.exe /c {line}");
        return ExitCode::SUCCESS;
    }
    let comspec = std::env::var_os("COMSPEC")
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "cmd.exe".into());
    let mut cmd = Command::new(comspec);
    cmd.arg("/c").raw_arg(&line);
    match cmd.status() {
        Ok(s) => ExitCode::from((s.code().unwrap_or(1) & 0xFF) as u8),
        Err(e) => {
            eprintln!("start: cannot run cmd.exe: {e}");
            ExitCode::from(126)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: &[&str]) -> String {
        command_line(&a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn url_with_ampersand() {
        assert_eq!(
            line(&["http://x/?a=1&b=2"]),
            r#"start "" "http://x/?a=1&b=2""#
        );
        assert_eq!(
            line(&["https://x/p?q=a b"]),
            r#"start "" "https://x/p?q=a b""#
        );
    }

    #[test]
    fn msys_paths() {
        assert_eq!(convert("/c/Users/x/a.html"), r"C:\Users\x\a.html");
        assert_eq!(convert("c:/x/y"), r"c:\x\y");
        assert_eq!(convert("https://a/b"), "https://a/b");
        assert_eq!(convert("/tmp/x"), "/tmp/x");
    }

    #[test]
    fn spaces_and_title() {
        assert_eq!(
            line(&["/c/My Dir/a b.txt"]),
            r#"start "" "C:\My Dir\a b.txt""#
        );
        assert_eq!(line(&["", "notepad"]), "start \"\" notepad");
        assert_eq!(line(&["a|b<c>d^e"]), r#"start "" "a|b<c>d^e""#);
        assert_eq!(line(&["x\"y", ""]), r#"start "" "x""y" """#);
    }
}
