//! `id` against Git Bash's `id.exe` (GNU coreutils 8.32 on Cygwin): the same stdout, stderr and
//! status for each form. The values are this machine's, so the reference is run here; the
//! comparison is skipped when Git for Windows is not installed. Nothing depends on the user
//! name or its length: the current user's name is taken from the reference `id -un`.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn git_id() -> Option<PathBuf> {
    let p = PathBuf::from(r"C:\Program Files\Git\usr\bin\id.exe");
    p.is_file().then_some(p)
}

fn run(exe: &std::path::Path, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run id")
}

fn ours(args: &[&str]) -> Output {
    run(std::path::Path::new(env!("CARGO_BIN_EXE_id")), args)
}

/// The reference's stderr with its program name (Cygwin's argv[0], a full POSIX path) as `id`.
fn normalize(stderr: &[u8], argv0: &str) -> String {
    String::from_utf8_lossy(stderr)
        .replace(&format!("{argv0}:"), "id:")
        .replace(&format!("'{argv0} --help'"), "'id --help'")
}

#[test]
fn same_as_git_bash() {
    let Some(git) = git_id() else {
        eprintln!("skipped: Git for Windows' id.exe not found");
        return;
    };
    let bad = run(&git, &["-x"]);
    let text = String::from_utf8_lossy(&bad.stderr).into_owned();
    let argv0 = text
        .split_once("Try '")
        .and_then(|(_, r)| r.split_once(" --help'"))
        .map(|(a, _)| a.to_owned())
        .expect("argv0 in the reference's Try line");
    let me = String::from_utf8_lossy(&run(&git, &["-un"]).stdout)
        .trim_end()
        .to_owned();
    let uid = String::from_utf8_lossy(&run(&git, &["-u"]).stdout)
        .trim_end()
        .to_owned();
    let mut forms: Vec<Vec<&str>> = [
        &[][..],
        &["-u"],
        &["-g"],
        &["-G"],
        &["-un"],
        &["-gn"],
        &["-Gn"],
        &["-ur"],
        &["-unr"],
        &["-Gr"],
        &["-a"],
        &["--user"],
        &["--group"],
        &["--groups"],
        &["--name", "-u"],
        &["-uz"],
        &["-Gz"],
        &["-Gnz"],
        &["-n"],
        &["-r"],
        &["-z"],
        &["-ug"],
        &["-uG"],
        &["-x"],
        &["-h"],
        &["--bogus"],
        &["--g"],
        &["--user=x"],
        &["-Z"],
        &["--"],
        &["-u", ""],
        &["-u", "-"],
        &["SYSTEM"],
        &["-un", "SYSTEM"],
        &["-Gn", "SYSTEM"],
        &["18"],
        &["544"],
        &["+18"],
        &["0"],
        &["65792"],
        &["4294967295"],
        &["no-such-user-9z"],
        &["-u", "no-such-user-9z", "SYSTEM"],
    ]
    .iter()
    .map(|f| f.to_vec())
    .collect();
    forms.push(vec![&me]);
    forms.push(vec!["-un", &me]);
    forms.push(vec!["-G", &me]);
    forms.push(vec![&uid]);
    for f in &forms {
        let (r, n) = (run(&git, f), ours(f));
        assert_eq!(
            String::from_utf8_lossy(&n.stdout),
            String::from_utf8_lossy(&r.stdout),
            "stdout of id {f:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&n.stderr),
            normalize(&r.stderr, &argv0),
            "stderr of id {f:?}"
        );
        assert_eq!(n.status.code(), r.status.code(), "status of id {f:?}");
    }
}

#[test]
fn help_version_and_errors() {
    let help = ours(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).starts_with("Usage: id [OPTION]... [USER]...\n"));
    let version = ours(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("id (skua-tools) "));
    let bad = ours(&["-x"]);
    assert_eq!(bad.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&bad.stderr),
        "id: unknown option -- x\nTry 'id --help' for more information.\n"
    );
    let unknown = ours(&["no-such-user-9z"]);
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&unknown.stderr),
        "id: \u{2018}no-such-user-9z\u{2019}: no such user\n"
    );
    // Well-known accounts have fixed ids on every Windows machine.
    let system = ours(&["-u", "SYSTEM"]);
    assert_eq!(String::from_utf8_lossy(&system.stdout), "18\n");
    let g = ours(&["-gn", "18"]);
    assert_eq!(g.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&g.stdout), "18\n");
    assert_eq!(
        String::from_utf8_lossy(&g.stderr),
        "id: cannot find name for group ID 18\n"
    );
}
