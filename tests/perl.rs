//! `perl` launcher: arguments must reach Git for Windows' perl unchanged. Skipped when Git for
//! Windows is not installed.

use std::process::{Command, Output};

fn have_git_perl() -> bool {
    std::path::Path::new(r"C:\Program Files\Git\usr\bin\perl.exe").is_file()
        || std::env::var_os("SKUA_PERL").is_some()
}

fn perl(args: &[&str], stdin: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_perl"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run perl launcher");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

fn args_seen(args: &[&str]) -> String {
    let mut all = vec!["-e", r#"print join("|", map { "<$_>" } @ARGV)"#];
    all.extend_from_slice(args);
    let out = perl(&all, b"");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn arguments_with_quotes_arrive_unchanged() {
    if !have_git_perl() {
        return;
    }
    assert_eq!(args_seen(&["c\"d"]), "<c\"d>");
    assert_eq!(args_seen(&["it's"]), "<it's>");
    assert_eq!(args_seen(&["x\"y", "~", "a b", ""]), "<x\"y>|<~>|<a b>|<>");
    assert_eq!(
        args_seen(&["*.nomatch", "a\\", "b c\\"]),
        "<*.nomatch>|<a\\>|<b c\\>"
    );
}

#[test]
fn one_liners_with_quotes_run() {
    if !have_git_perl() {
        return;
    }
    let out = perl(&["-pe", "s/\"//g"], b"a\"b\n");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ab\n");
    let out = perl(&["-pe", "s/it's/IT/"], b"it's a \"q\"\n");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "IT a \"q\"\n");
}

#[test]
fn a_signalled_perl_is_not_reported_as_success() {
    if !have_git_perl() {
        return;
    }
    let out = perl(&["-e", "kill 9, $$"], b"");
    assert_eq!(out.status.code(), Some(137));
}
