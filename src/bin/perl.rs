//! `perl` for Skua Bash: not an implementation. Finds Git for Windows' perl and runs it with the
//! same arguments, stdio and exit status (owner decision: Skua Bash does not reimplement Perl).
//!
//! Search order: `SKUA_PERL`; `git.exe` on `PATH` (its Git root: `cmd\`, `bin\`, `mingw64\bin\`)
//! then `usr\bin\perl.exe`; then the usual installs (Program Files, per-user, Scoop). Git's perl
//! is an MSYS program: its `usr\bin` is put in front of `PATH` for the child so its DLLs and
//! helpers resolve, which means a `system("ls")` inside a script runs Git's `ls`. Arguments are
//! quoted for the Cygwin parser (`msys_args`).

#[path = "../misc/launch.rs"]
mod launch;
#[path = "../misc/msys_args.rs"]
mod msys_args;

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const NOT_FOUND: &str = "perl: not found: Skua Bash runs Git for Windows' perl; install Git for Windows or set SKUA_PERL";

/// The Git for Windows root that contains `git.exe`'s directory.
fn git_root(git_dir: &Path) -> Option<PathBuf> {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let mut dir = git_dir.to_owned();
    if name(&dir).as_deref() == Some("shims") {
        // Scoop: `<scoop>\shims\git.exe`, the real Git is `<scoop>\apps\git\current`.
        let current = dir.parent()?.join("apps").join("git").join("current");
        return current.is_dir().then_some(current);
    }
    if name(&dir).as_deref() == Some("bin") {
        let parent = dir.parent()?.to_owned();
        if matches!(
            name(&parent).as_deref(),
            Some("mingw64" | "mingw32" | "clangarm64")
        ) {
            return parent.parent().map(Path::to_owned);
        }
        // `Git\bin\git.exe` or `Git\usr\bin`: for the latter the root is two up.
        if name(&parent).as_deref() == Some("usr") {
            return parent.parent().map(Path::to_owned);
        }
    }
    if matches!(name(&dir).as_deref(), Some("cmd" | "bin")) {
        dir.pop();
        return Some(dir);
    }
    None
}

/// Where Git for Windows is usually installed (machine-wide, per user, Scoop), as roots.
fn default_git_roots() -> Vec<PathBuf> {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let mut roots = Vec::new();
    for k in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        roots.extend(var(k).map(|p| p.join("Git")));
    }
    roots.extend(var("LOCALAPPDATA").map(|p| p.join("Programs").join("Git")));
    roots.extend(var("SCOOP").map(|p| p.join("apps").join("git").join("current")));
    roots.extend(
        var("USERPROFILE").map(|p| p.join("scoop").join("apps").join("git").join("current")),
    );
    roots.push(PathBuf::from(r"C:\Program Files\Git"));
    roots
}

fn find_perl() -> Option<PathBuf> {
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    // A candidate counts when it is a file and not this launcher itself (no recursion).
    let usable = |c: &Path| c.is_file() && c.canonicalize().ok() != me;
    if let Some(p) = std::env::var_os("SKUA_PERL").filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        if usable(&p) {
            return Some(p);
        }
        eprintln!(
            "perl: warning: SKUA_PERL={} is not a usable perl; searching for Git for Windows",
            p.display()
        );
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            if !d.join("git.exe").is_file() {
                continue;
            }
            if let Some(root) = git_root(&d) {
                let c = root.join("usr").join("bin").join("perl.exe");
                if usable(&c) {
                    return Some(c);
                }
            }
        }
    }
    default_git_roots()
        .into_iter()
        .map(|r| r.join("usr").join("bin").join("perl.exe"))
        .find(|c| usable(c))
}

fn main() -> ExitCode {
    let Some(perl) = find_perl() else {
        eprintln!("{NOT_FOUND}");
        return ExitCode::from(127);
    };
    let mut cmd = Command::new(&perl);
    for a in std::env::args_os().skip(1) {
        cmd.raw_arg(msys_args::quote(&a));
    }
    if let Some(bin) = perl.parent() {
        let mut dirs: Vec<PathBuf> = vec![bin.to_owned()];
        if let Some(p) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            cmd.env("PATH", joined);
        }
    }
    launch::run("perl", cmd)
}
