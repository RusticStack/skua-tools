//! `xxd` for Skua Bash, as Git Bash's vim xxd (2026-06-16): hex dumps (normal, binary, little
//! endian, plain, C include) and `-r` reverts. See `src/xxd/` for the pieces.

#[path = "../xxd/mod.rs"]
mod xxd;

fn main() -> std::process::ExitCode {
    xxd::main_exit()
}
