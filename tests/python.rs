//! `python3` / `python` launchers end to end with fake directories: a stub-only WindowsApps
//! (zero-length files, as the App Installer's look), a "Python" whose `python.exe` is a copy of
//! `cmd.exe` (so running it shows the arguments arrived), and nothing at all.
//! Windows only: off Windows these commands run the system's own (see `tests/unix.rs`).

#![cfg(windows)]

use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("skua-tools-py-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("mkdir");
        Self(p)
    }
    fn sub(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cmd_exe() -> PathBuf {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join(r"System32\cmd.exe")
}

#[test]
fn python3_runs_python_exe_past_the_store_stubs() {
    let d = Dir::new("run");
    let apps = d.sub("WindowsApps");
    std::fs::write(apps.join("python3.exe"), b"").expect("stub");
    std::fs::write(apps.join("python.exe"), b"").expect("stub");
    let real = d.sub("Python315");
    std::fs::copy(cmd_exe(), real.join("python.exe")).expect("copy");
    // Marks the install as Python 3: python3 never falls back to a Python 2 python.exe.
    std::fs::write(real.join("python315.dll"), b"").expect("dll");
    let path = std::env::join_paths([apps, real]).expect("join");
    for launcher in [env!("CARGO_BIN_EXE_python3"), env!("CARGO_BIN_EXE_python")] {
        let out = Command::new(launcher)
            .args(["/d", "/c", "echo", "from-fake-python"])
            .env("PATH", &path)
            .stdin(Stdio::null())
            .output()
            .expect("run");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "from-fake-python"
        );
    }
}

#[test]
fn the_exit_status_passes_through() {
    let d = Dir::new("status");
    let real = d.sub("Py");
    std::fs::copy(cmd_exe(), real.join("python3.exe")).expect("copy");
    let out = Command::new(env!("CARGO_BIN_EXE_python3"))
        .args(["/d", "/c", "exit", "3"])
        .env("PATH", &real)
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn nothing_installed_fails_fast_with_127() {
    let d = Dir::new("none");
    let apps = d.sub("WindowsApps");
    std::fs::write(apps.join("python3.exe"), b"").expect("stub");
    let empty = d.sub("empty");
    let start = std::time::Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_python3"))
        .arg("-c")
        .arg("print(1)")
        .env("PATH", &apps)
        .env("SystemRoot", &empty)
        .env("LOCALAPPDATA", &empty)
        .stdin(Stdio::null())
        .output()
        .expect("run");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(127), "{err}");
    assert!(
        err.starts_with("bash: python3: command not found ("),
        "{err}"
    );
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn a_launcher_copy_on_path_is_not_run_again() {
    let d = Dir::new("loop");
    let bin = d.sub("bin");
    std::fs::copy(env!("CARGO_BIN_EXE_python3"), bin.join("python3.exe")).expect("copy");
    std::fs::copy(env!("CARGO_BIN_EXE_python"), bin.join("python.exe")).expect("copy");
    let empty = d.sub("empty");
    let out = Command::new(bin.join("python3.exe"))
        .arg("-V")
        .env("PATH", &bin)
        .env("SystemRoot", &empty)
        .env("LOCALAPPDATA", &empty)
        .stdin(Stdio::null())
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(127),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
