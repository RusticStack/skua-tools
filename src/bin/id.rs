//! `id` for Skua Bash, as Git Bash's (GNU coreutils 8.32 on Cygwin): uid/gid from the process
//! token with Cygwin's SID mapping. See `src/id/`.

#[cfg(windows)]
#[path = "../id/mod.rs"]
mod id;
#[cfg(not(windows))]
#[path = "../misc/launch.rs"]
mod launch;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    id::main_exit()
}

/// Off Windows there are no SIDs to map: the system's `id` is the answer.
#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    launch::system_tool("id")
}
