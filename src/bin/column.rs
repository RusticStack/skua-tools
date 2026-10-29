//! `column` for Skua Bash, as Git Bash's util-linux 2.40 `column`: fill mode and table mode
//! (`-t`), JSON output. See `src/column/`.

#[path = "../column/mod.rs"]
mod column;

fn main() -> std::process::ExitCode {
    column::main_exit()
}
