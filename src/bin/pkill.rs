//! `pkill` for Skua Bash (procps-ng semantics over Windows processes); the code is shared with
//! its sibling in `src/procshare/`.

#[cfg(not(windows))]
#[path = "../misc/launch.rs"]
mod launch;
#[cfg(windows)]
#[path = "../procshare/pgrep_args.rs"]
mod pgrep_args;
#[cfg(windows)]
#[path = "../procshare/pgrep_impl.rs"]
mod pgrep_impl;
#[cfg(windows)]
#[path = "../procshare/procs.rs"]
mod procs;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    pgrep_impl::run(true)
}

/// Off Windows the system has procps (or BSD) `pkill`: run that one.
#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    launch::system_tool("pkill")
}
