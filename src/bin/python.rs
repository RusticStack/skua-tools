//! `python` for Skua Bash: a launcher that finds a real Python and runs it (see
//! `skua_tools::python`): App Installer stubs in WindowsApps are skipped, `python.exe` stands in
//! for `python3.exe` and the other way round, and the `py` launcher is the last resort.

#[path = "../misc/launch.rs"]
mod launch;

fn main() -> std::process::ExitCode {
    skua_tools::python::launch("python", |cmd| launch::run("python", cmd))
}
