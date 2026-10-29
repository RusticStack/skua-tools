//! `fgrep` for Skua Bash: `grep -F "$@"`, as Git Bash's `/usr/bin/fgrep` script does.

#[path = "../misc/grep.rs"]
mod grep;
#[path = "../misc/launch.rs"]
mod launch;

fn main() -> std::process::ExitCode {
    grep::grep_wrapper("fgrep", "-F")
}
