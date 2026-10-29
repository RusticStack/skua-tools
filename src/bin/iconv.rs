//! `iconv` for Skua Bash, as Git Bash's GNU libiconv 1.19 `iconv`. See `src/iconv/`.

#[path = "../iconv/mod.rs"]
mod iconv;

fn main() -> std::process::ExitCode {
    iconv::main_exit()
}
