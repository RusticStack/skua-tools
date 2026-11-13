//! `id` for Skua Bash, as Git Bash's (GNU coreutils 8.32 on Cygwin): uid/gid from the process
//! token with Cygwin's SID mapping. See `src/id/`.

#[path = "../id/mod.rs"]
mod id;

fn main() -> std::process::ExitCode {
    id::main_exit()
}
