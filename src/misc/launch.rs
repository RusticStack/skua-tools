//! Shared by `egrep`, `fgrep` and `perl`: run another program with our stdio, wait, and return
//! its exit status. Not part of the library (`#[path]` include) so the crate root stays shared.

use std::process::{Command, ExitCode};

unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

/// Swallow Ctrl+C and Ctrl+Break in the launcher: the child shares the console, gets the event
/// too, and we report how it ended. (Not "ignore": that flag would be inherited by the child.)
unsafe extern "system" fn swallow(kind: u32) -> i32 {
    i32::from(kind == 0 || kind == 1)
}

fn install_ctrl_handler() {
    // SAFETY: registers a handler that only returns a constant.
    unsafe {
        SetConsoleCtrlHandler(Some(swallow), 1);
    }
}

/// An exit status as a shell sees it: Ctrl+C status (0xC000013A) becomes 130, other values wrap
/// to 8 bits.
pub fn exit_code(code: Option<i32>) -> ExitCode {
    match code {
        Some(c) if c as u32 == 0xC000_013A => ExitCode::from(130),
        // An MSYS/Cygwin program killed by signal N exits with N << 8: report 128 + N as a shell does.
        Some(c)
            if (c as u32) & 0xFF == 0
                && (1..0x80).contains(&(c as u32 >> 8))
                && (c as u32) <= 0x7F00 =>
        {
            ExitCode::from(128 + (c as u32 >> 8) as u8)
        }
        Some(c) => ExitCode::from((c & 0xFF) as u8),
        None => ExitCode::from(1),
    }
}

/// Run `cmd` with inherited stdio and return its status; `name` prefixes the error on failure.
pub fn run(name: &str, mut cmd: Command) -> ExitCode {
    install_ctrl_handler();
    match cmd.status() {
        Ok(status) => exit_code(status.code()),
        Err(e) => {
            eprintln!(
                "{name}: cannot run {}: {e}",
                cmd.get_program().to_string_lossy()
            );
            ExitCode::from(126)
        }
    }
}
