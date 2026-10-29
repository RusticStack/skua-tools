//! Process listing shared by `ps`, `pgrep` and `pkill` (included with `#[path]`, not a library
//! module, so the three binaries stay independent of the other tools).
//!
//! Skua Bash has no MSYS layer, so every process is a Windows process: PID is the Windows PID
//! (Git Bash's WINPID column). The UID is what Cygwin would give the owner: `0x30000 + RID` for
//! a `S-1-5-21-...-RID` account, the last sub-authority for the well-known ones (SYSTEM = 18).

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    /// Image name without `.exe`, lowercase: Windows image names are case-insensitive, and
    /// Git Bash's tools show lowercase (what `pgrep` matches without `-f`).
    pub name: String,
    /// Full image path when readable.
    pub image: Option<String>,
    /// The command line arguments as Windows has them (`argv[0]` as started, often a full
    /// path); empty when unreadable. `command_line()` shows them as a Unix tool would.
    pub argv: Vec<String>,
    /// Start time, milliseconds since the Unix epoch.
    pub start_ms: u64,
    pub uid: u32,
}

impl Proc {
    /// The command line, or the image when it cannot be read (what `ps -f` / `pgrep -a` show).
    pub fn command_line(&self) -> String {
        if self.argv.is_empty() {
            self.image_path().to_owned()
        } else {
            unix_command_line(&self.argv)
        }
    }

    /// The full image path (what `ps` prints), else the image name.
    pub fn image_path(&self) -> &str {
        self.image.as_deref().unwrap_or(&self.name)
    }

    /// Start time in whole seconds since the Unix epoch.
    pub fn start_secs(&self) -> u64 {
        self.start_ms / 1000
    }
}

/// Process creation time in milliseconds since the Unix epoch, to the 100 ns the kernel keeps
/// (sysinfo's start time is whole seconds, too coarse to tell `pgrep -n` which is newest).
#[cfg(windows)]
fn creation_ms(pid: u32) -> Option<u64> {
    use core::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut u64,
            exit: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    // SAFETY: the handle is checked for null, used only for GetProcessTimes with four valid
    // out-pointers (FILETIME is two u32, read here as one u64), and closed once.
    unsafe {
        let h = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let (mut created, mut a, mut b, mut c) = (0u64, 0u64, 0u64, 0u64);
        let ok = GetProcessTimes(h, &mut created, &mut a, &mut b, &mut c) != 0;
        CloseHandle(h);
        let ms = (created / 10_000).checked_sub(11_644_473_600_000)?;
        (ok && ms > 0).then_some(ms)
    }
}

#[cfg(not(windows))]
fn creation_ms(_pid: u32) -> Option<u64> {
    None
}

/// A freshly refreshed view of all processes.
pub fn system() -> System {
    let mut sys = System::new();
    let kind = ProcessRefreshKind::nothing()
        .with_cmd(UpdateKind::Always)
        .with_exe(UpdateKind::Always)
        .with_user(UpdateKind::Always);
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    sys
}

/// Drops `.exe` (any case) from an image name.
pub fn strip_exe(name: &str) -> &str {
    let n = name.len();
    if n > 4 && name.is_char_boundary(n - 4) && name[n - 4..].eq_ignore_ascii_case(".exe") {
        &name[..n - 4]
    } else {
        name
    }
}

/// Cygwin's UID for an account SID (`S-1-5-21-a-b-c-RID` -> 0x30000 + RID).
pub fn uid_from_sid(sid: &str) -> u32 {
    let rid = sid.rsplit('-').next().and_then(|r| r.parse::<u32>().ok());
    match rid {
        Some(rid) if sid.starts_with("S-1-5-21-") => 0x30000 + rid,
        Some(rid) => rid,
        None => 0,
    }
}

/// `argv` joined with spaces, `argv[0]` cut to its file name without `.exe`.
pub fn unix_command_line(argv: &[String]) -> String {
    let Some((first, rest)) = argv.split_first() else {
        return String::new();
    };
    let base = first.rsplit(['\\', '/']).next().unwrap_or(first);
    let mut out = strip_exe(base).to_owned();
    for a in rest {
        out.push(' ');
        out.push_str(a);
    }
    out
}

/// Every process except the idle process, ordered by PID.
pub fn collect(sys: &System) -> Vec<Proc> {
    let boot = System::boot_time();
    let mut out: Vec<Proc> = sys
        .processes()
        .values()
        .filter(|p| p.pid().as_u32() != 0)
        .map(|p| {
            let full = p.name().to_string_lossy().into_owned();
            let argv: Vec<String> = p
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            Proc {
                pid: p.pid().as_u32(),
                ppid: p.parent().map_or(0, Pid::as_u32),
                name: strip_exe(&full).to_ascii_lowercase(),
                image: p.exe().map(|e| e.to_string_lossy().into_owned()),
                argv,
                start_ms: creation_ms(p.pid().as_u32()).unwrap_or(match p.start_time() {
                    0 => boot * 1000,
                    t => t * 1000,
                }),
                uid: p.user_id().map_or(0, |u| uid_from_sid(&u.to_string())),
            }
        })
        .collect();
    out.sort_by_key(|p| p.pid);
    out
}

/// The current user's Cygwin UID (taken from this process).
pub fn own_uid(procs: &[Proc]) -> u32 {
    let me = std::process::id();
    procs.iter().find(|p| p.pid == me).map_or(0, |p| p.uid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_exe() {
        assert_eq!(strip_exe("sleep.exe"), "sleep");
        assert_eq!(strip_exe("SLEEP.EXE"), "SLEEP");
        assert_eq!(strip_exe("System"), "System");
        assert_eq!(strip_exe(".exe"), ".exe");
        assert_eq!(strip_exe("é.exe"), "é");
    }

    #[test]
    fn unix_command_lines() {
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let node = v(&[
            "C:\\Program Files\\nodejs\\node.exe",
            "server.js",
            "-p",
            "1",
        ]);
        assert_eq!(unix_command_line(&node), "node server.js -p 1");
        assert_eq!(unix_command_line(&v(&["/usr/bin/sleep", "5"])), "sleep 5");
        assert_eq!(unix_command_line(&v(&["ping"])), "ping");
        assert_eq!(unix_command_line(&[]), "");
    }

    #[test]
    fn uid_as_cygwin() {
        assert_eq!(uid_from_sid("S-1-5-21-1-2-3-1001"), 197609);
        assert_eq!(uid_from_sid("S-1-5-18"), 18);
        assert_eq!(uid_from_sid("garbage"), 0);
    }

    #[test]
    fn lists_itself() {
        let sys = system();
        let procs = collect(&sys);
        assert!(procs.iter().any(|p| p.pid == std::process::id()));
        assert!(procs.windows(2).all(|w| w[0].pid <= w[1].pid));
    }
}
