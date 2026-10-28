//! `egrep`/`fgrep` support: find the bundled grep and run it.

use super::launch::run;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// `prog` next to this executable, else the first `prog` on `PATH` that is not this executable.
pub fn find_tool(prog: &str) -> Option<PathBuf> {
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let same = |p: &Path| me.is_some() && p.canonicalize().ok() == me;
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_owned))
    {
        let c = dir.join(prog);
        if c.is_file() && !same(&c) {
            return Some(c);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(prog))
        .find(|c| c.is_file() && !same(c))
}

/// `egrep` and `fgrep`: `grep <flag> ARGS...`, bundled grep first. Git Bash's are two-line
/// `exec grep -E "$@"` scripts: no warning, same stdin, same status.
pub fn grep_wrapper(name: &str, flag: &str) -> ExitCode {
    let sibling = find_tool("grep.exe").filter(|p| {
        p.parent()
            == std::env::current_exe()
                .ok()
                .as_deref()
                .and_then(Path::parent)
    });
    let on_path = find_tool("grep.exe").is_some();
    // From PATH, run it by name so it sees argv[0] = `grep` (as `exec grep` does), not a full path.
    let mut cmd = match (sibling, on_path) {
        (Some(p), _) => Command::new(p),
        (None, true) => Command::new("grep"),
        (None, false) => {
            eprintln!("{name}: grep: command not found");
            return ExitCode::from(127);
        }
    };
    cmd.arg(flag).args(std::env::args_os().skip(1));
    run(name, cmd)
}
