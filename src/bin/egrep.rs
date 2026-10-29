//! `egrep` for Skua Bash: `grep -E "$@"`, as Git Bash's `/usr/bin/egrep` script does.

#[path = "../misc/grep.rs"]
mod grep;
#[path = "../misc/launch.rs"]
mod launch;

fn main() -> std::process::ExitCode {
    grep::grep_wrapper("egrep", "-E")
}
