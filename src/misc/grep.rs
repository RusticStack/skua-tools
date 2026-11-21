//! `egrep`/`fgrep` support: find the bundled grep and run it.

use super::launch::run;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// `prog` next to this executable, else the first `prog` in an absolute `PATH` directory that is
/// not this executable. A relative or empty `PATH` entry (`.`) is skipped: it names a directory
/// under the current one, where a checkout could plant a `grep.exe`.
pub fn find_tool(prog: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let me = exe.as_deref().and_then(|p| p.canonicalize().ok());
    let same = |p: &Path| me.is_some() && p.canonicalize().ok() == me;
    if let Some(dir) = exe.as_deref().and_then(Path::parent) {
        let c = dir.join(prog);
        if c.is_file() && !same(&c) {
            return Some(c);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|d| d.is_absolute())
        .map(|d| d.join(prog))
        .find(|c| c.is_file() && !same(c))
}

/// `egrep` and `fgrep`: `grep <flag> ARGS...`, bundled grep first. Git Bash's are two-line
/// `exec grep -E "$@"` scripts: no warning, same stdin, same status. The program is run by its
/// absolute path, never by a name `CreateProcess` would search for in the current directory.
pub fn grep_wrapper(name: &str, flag: &str) -> ExitCode {
    let Some(grep) = find_tool("grep.exe") else {
        eprintln!("{name}: grep: command not found");
        return ExitCode::from(127);
    };
    let mut cmd = Command::new(grep);
    cmd.arg(flag).args(std::env::args_os().skip(1));
    run(name, cmd)
}
