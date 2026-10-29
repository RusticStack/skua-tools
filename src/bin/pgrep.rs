//! `pgrep` for Skua Bash (procps-ng semantics over Windows processes); the code is shared with
//! its sibling in `src/procshare/`.

#[path = "../procshare/pgrep_args.rs"]
mod pgrep_args;
#[path = "../procshare/pgrep_impl.rs"]
mod pgrep_impl;
#[path = "../procshare/procs.rs"]
mod procs;

fn main() -> std::process::ExitCode {
    pgrep_impl::run(false)
}
