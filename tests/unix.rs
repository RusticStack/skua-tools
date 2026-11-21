//! Off Windows: `id`, `ps`, `pgrep`, `pkill` and `perl` run the system's own tool, `python` and
//! `python3` find an interpreter by its Unix name, and `egrep` runs `grep -E`. Each test builds
//! a `PATH` of fake tools (shell scripts that print their arguments), so nothing depends on what
//! the machine has installed beyond `/bin/sh`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("skua-tools-unix-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("mkdir");
        Self(p)
    }

    /// An executable script `name` that prints `label`, then its arguments one per line, and
    /// exits with `status`.
    fn tool(&self, name: &str, label: &str, status: u8) -> PathBuf {
        let p = self.0.join(name);
        fs::write(
            &p,
            format!(
                "#!/bin/sh\necho {label}\nfor a in \"$@\"; do echo \"[$a]\"; done\nexit {status}\n"
            ),
        )
        .expect("write");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod");
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(exe: &str, path: &Path, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .env("PATH", path)
        .env_remove("SKUA_PERL")
        .output()
        .expect("run")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn system_commands_run_the_systems_own_with_arguments_and_status() {
    for (name, exe) in [
        ("id", env!("CARGO_BIN_EXE_id")),
        ("ps", env!("CARGO_BIN_EXE_ps")),
        ("pgrep", env!("CARGO_BIN_EXE_pgrep")),
        ("pkill", env!("CARGO_BIN_EXE_pkill")),
        ("perl", env!("CARGO_BIN_EXE_perl")),
    ] {
        let d = Dir::new(name);
        d.tool(name, "system", 7);
        let o = run(exe, &d.0, &["-a", "two words", ""]);
        assert_eq!(out(&o), "system\n[-a]\n[two words]\n[]\n", "{name}");
        assert_eq!(o.status.code(), Some(7), "{name}: {}", err(&o));
    }
}

#[test]
fn without_a_system_tool_it_is_command_not_found() {
    let d = Dir::new("none");
    let o = run(env!("CARGO_BIN_EXE_ps"), &d.0, &[]);
    assert_eq!(o.status.code(), Some(127));
    assert!(err(&o).starts_with("ps: command not found"), "{}", err(&o));
}

/// A copy of the launcher first on `PATH` (Skua's own bin directory) is not what it runs.
#[test]
fn a_copy_of_itself_on_path_is_skipped() {
    let ours = Dir::new("self-ours");
    let system = Dir::new("self-system");
    fs::copy(env!("CARGO_BIN_EXE_id"), ours.0.join("id")).expect("copy");
    system.tool("id", "system", 0);
    let path = std::env::join_paths([&ours.0, &system.0]).expect("path");
    let o = Command::new(ours.0.join("id"))
        .env("PATH", path)
        .output()
        .expect("run");
    assert_eq!(out(&o), "system\n", "{}", err(&o));
}

#[test]
fn skua_perl_names_the_perl() {
    let d = Dir::new("perl-override");
    let named = d.tool("myperl", "named", 3);
    let o = Command::new(env!("CARGO_BIN_EXE_perl"))
        .args(["-e", "print \"a b\""])
        .env("PATH", &d.0)
        .env("SKUA_PERL", &named)
        .output()
        .expect("run");
    assert_eq!(out(&o), "named\n[-e]\n[print \"a b\"]\n");
    assert_eq!(o.status.code(), Some(3));
}

#[test]
fn python_and_python3_stand_in_for_each_other() {
    // Only `python3` (most Linux systems, macOS): `python` runs it.
    let d = Dir::new("py3-only");
    d.tool("python3", "three", 5);
    let o = run(env!("CARGO_BIN_EXE_python"), &d.0, &["-c", "x y"]);
    assert_eq!(out(&o), "three\n[-c]\n[x y]\n", "{}", err(&o));
    assert_eq!(o.status.code(), Some(5));

    // Only a venv's `python`: `python3` runs it when the venv is a Python 3.
    let v = Dir::new("venv");
    fs::create_dir_all(v.0.join("bin")).expect("mkdir");
    fs::write(v.0.join("pyvenv.cfg"), "version = 3.13.1\n").expect("write");
    let bin = Dir(v.0.join("bin"));
    bin.tool("python", "venv", 0);
    let o = run(env!("CARGO_BIN_EXE_python3"), &bin.0, &[]);
    assert_eq!(out(&o), "venv\n", "{}", err(&o));
    std::mem::forget(bin);

    // A `python` that is not known to be a Python 3 is not what `python3` runs.
    let two = Dir::new("py2");
    two.tool("python", "two", 0);
    let o = run(env!("CARGO_BIN_EXE_python3"), &two.0, &[]);
    assert_eq!(o.status.code(), Some(127));
    assert_eq!(
        err(&o),
        "bash: python3: command not found (no python3 or python on PATH)\n"
    );
}

#[test]
fn python_skips_files_that_cannot_run_and_follows_links() {
    let d = Dir::new("py-links");
    let real = Dir::new("py-real");
    let target = real.tool("python3.13", "linked", 0);
    // Not executable: skipped.
    let dead = Dir::new("py-dead");
    fs::write(dead.0.join("python3"), "#!/bin/sh\necho dead\n").expect("write");
    std::os::unix::fs::symlink(&target, d.0.join("python3")).expect("symlink");
    let path = std::env::join_paths([&dead.0, &d.0]).expect("path");
    let o = Command::new(env!("CARGO_BIN_EXE_python3"))
        .env("PATH", path)
        .output()
        .expect("run");
    assert_eq!(out(&o), "linked\n", "{}", err(&o));
}

#[test]
fn egrep_and_fgrep_run_grep_with_the_flag() {
    let d = Dir::new("grep");
    d.tool("grep", "grep", 1);
    let o = run(env!("CARGO_BIN_EXE_egrep"), &d.0, &["a|b", "f"]);
    assert_eq!(out(&o), "grep\n[-E]\n[a|b]\n[f]\n", "{}", err(&o));
    assert_eq!(o.status.code(), Some(1));
    let o = run(env!("CARGO_BIN_EXE_fgrep"), &d.0, &["x"]);
    assert_eq!(out(&o), "grep\n[-F]\n[x]\n", "{}", err(&o));
}

#[test]
fn wget_without_curl_says_what_it_looked_for() {
    let d = Dir::new("nocurl");
    let o = run(env!("CARGO_BIN_EXE_wget"), &d.0, &["http://127.0.0.1:9/x"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        err(&o),
        "wget: curl not found (looked for curl on PATH); this wget runs curl\n"
    );
}
