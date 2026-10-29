//! `perl` for Skua Bash: not an implementation. Finds Git for Windows' perl and runs it with the
//! same arguments, stdio and exit status (owner decision: Skua Bash does not reimplement Perl).
//!
//! Search order: `SKUA_PERL`; `git.exe` on `PATH` (its Git root: `cmd\`, `bin\`, `mingw64\bin\`)
//! then `usr\bin\perl.exe`; `C:\Program Files\Git\usr\bin\perl.exe`. Git's perl is an MSYS
//! program: its `usr\bin` is put on `PATH` for the child so its DLLs and helpers resolve.

#[path = "../misc/launch.rs"]
mod launch;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const NOT_FOUND: &str = "perl: not found: Skua Bash runs Git for Windows' perl; install Git for Windows or set SKUA_PERL";

/// The Git for Windows root that contains `git.exe`'s directory.
fn git_root(git_dir: &Path) -> Option<PathBuf> {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase());
    let mut dir = git_dir.to_owned();
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

fn find_perl() -> Option<PathBuf> {
    if let Some(p) =
        std::env::var_os("SKUA_PERL").filter(|p| !p.is_empty() && Path::new(p).is_file())
    {
        return Some(PathBuf::from(p));
    }
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            if !d.join("git.exe").is_file() {
                continue;
            }
            if let Some(root) = git_root(&d) {
                let c = root.join("usr").join("bin").join("perl.exe");
                if c.is_file() && c.canonicalize().ok() != me {
                    return Some(c);
                }
            }
        }
    }
    let c = PathBuf::from(r"C:\Program Files\Git\usr\bin\perl.exe");
    (c.is_file() && c.canonicalize().ok() != me).then_some(c)
}

fn main() -> ExitCode {
    let Some(perl) = find_perl() else {
        eprintln!("{NOT_FOUND}");
        return ExitCode::from(127);
    };
    let mut cmd = Command::new(&perl);
    cmd.args(std::env::args_os().skip(1));
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
