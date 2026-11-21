//! `perl` for Skua Bash: not an implementation. Finds Git for Windows' perl and runs it with the
//! same arguments, stdio and exit status (owner decision: Skua Bash does not reimplement Perl).
//!
//! Search order: `SKUA_PERL`; then every Git for Windows install `skua_tools::gitwin` finds
//! (`git.exe` on `PATH`, Program Files, per-user, the registry, Scoop), each probed for
//! `perl.exe` in `usr\bin`, `ucrt64\bin`, `mingw64\bin`, `clangarm64\bin`. Git's perl is an
//! MSYS program: its `usr\bin` is put in front of `PATH` for the child so its DLLs and
//! helpers resolve, which means a `system("ls")` inside a script runs Git's `ls`. Arguments are
//! quoted for the Cygwin parser (`msys_args`).

//!
//! Off Windows there is no Git for Windows and every system has a perl: `SKUA_PERL`, else the
//! first other `perl` on `PATH`, run in place of this process.

#[path = "../misc/launch.rs"]
mod launch;
#[cfg(windows)]
#[path = "../misc/msys_args.rs"]
mod msys_args;

#[cfg(windows)]
use skua_tools::gitwin;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const OVERRIDE: &str = "SKUA_PERL";

/// `SKUA_PERL` when it names a usable perl, else Git for Windows' perl
/// ([`skua_tools::gitwin`]); `Err` is the message for a perl that is not there.
#[cfg(windows)]
fn find_perl() -> Result<PathBuf, String> {
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    // A candidate counts when it is a file and not this launcher itself (no recursion).
    let usable = |c: &Path| c.is_file() && c.canonicalize().ok() != me;
    if let Some(p) = std::env::var_os(OVERRIDE).filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        if usable(&p) {
            return Ok(p);
        }
        eprintln!(
            "perl: warning: {OVERRIDE}={} is not a usable perl; searching for Git for Windows",
            p.display()
        );
    }
    let probe = gitwin::Probe::from_env();
    let all = gitwin::installs(&probe);
    all.iter()
        .filter_map(|g| g.tool("perl.exe"))
        .find(|c| usable(c))
        .ok_or_else(|| gitwin::not_found_message("perl", "perl.exe", OVERRIDE, &all))
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    match std::env::var_os(OVERRIDE).filter(|p| !p.is_empty()) {
        Some(perl) => {
            let mut cmd = Command::new(perl);
            cmd.args(std::env::args_os().skip(1));
            launch::run("perl", cmd)
        }
        None => launch::system_tool("perl"),
    }
}

#[cfg(windows)]
fn main() -> ExitCode {
    let perl = match find_perl() {
        Ok(perl) => perl,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(127);
        }
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
