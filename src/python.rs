//! `python` / `python3` for Skua Bash: find a real interpreter and run it.
//!
//! On Windows, `python.exe` and `python3.exe` in `%LOCALAPPDATA%\Microsoft\WindowsApps` are
//! usually App Installer stubs (zero-length app-execution-alias reparse points of
//! `Microsoft.DesktopAppInstaller` that point at `AppInstallerPythonRedirector.exe`): with
//! arguments they print an install hint and exit 9009, without arguments they open the Microsoft
//! Store. A python.org install has `python.exe` but no `python3.exe`, a venv's `Scripts\` neither
//! `python3.exe`, and the `py` launcher may be all there is. Git Bash users expect `python3` and
//! `python` to run the interpreter in each of those cases.
//!
//! Lookup for a name (`python3`; `python` is the same with the names swapped), in order:
//!
//! 1. each `PATH` directory in turn: `python3.exe`, then `python.exe`, skipping App Installer
//!    stubs, zero-length files and other copies of this launcher (so a venv's `python.exe` that
//!    comes first on `PATH` wins over an install's `python3.exe` that comes later);
//! 2. the `py` launcher (`py.exe` on `PATH`, `%SystemRoot%\py.exe`,
//!    `%LOCALAPPDATA%\Programs\Python\Launcher\py.exe`), as `py -3` for `python3`.
//!
//! Nothing found: one line on stderr and status 127, without ever starting a stub.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Bytes only the launcher's own executable holds, so another copy of it on `PATH` (Skua's
/// `bin\` and a standalone install both on `PATH`) is skipped instead of run in a loop.
pub const LAUNCHER_MARKER: &[u8] = b"skua-tools python launcher (do not run me as python) v1";

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The interpreter (or `py.exe`).
    pub program: PathBuf,
    /// Arguments before the user's (`-3` for `py` as `python3`).
    pub leading: Vec<&'static str>,
}

/// Where to look.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// The `PATH` directories, in order.
    pub path: Vec<PathBuf>,
    /// Places of `py.exe` after `PATH`.
    pub py_launchers: Vec<PathBuf>,
}

impl Search {
    /// From this process's environment.
    pub fn from_env() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let mut py_launchers = Vec::new();
        py_launchers.extend(var("SystemRoot").map(|r| r.join("py.exe")));
        py_launchers.extend(var("LOCALAPPDATA").map(|p| {
            p.join("Programs")
                .join("Python")
                .join("Launcher")
                .join("py.exe")
        }));
        Self { path, py_launchers }
    }
}

/// `python3.exe` for `python3`, `python.exe` for `python`, with the other name second.
fn names(wanted: &str) -> [&'static str; 2] {
    if wanted.eq_ignore_ascii_case("python3") {
        ["python3.exe", "python.exe"]
    } else {
        ["python.exe", "python3.exe"]
    }
}

/// Finds what `wanted` (`python` or `python3`) runs. `is_launcher` tells a copy of this
/// launcher (by default [`is_launcher_file`]).
pub fn resolve(
    wanted: &str,
    search: &Search,
    is_launcher: &dyn Fn(&Path) -> bool,
) -> Option<Found> {
    let python3 = wanted.eq_ignore_ascii_case("python3");
    // Only absolute PATH entries: an empty or relative one (`.`) would let the current
    // directory pick the interpreter.
    let dirs = || search.path.iter().filter(|d| d.is_absolute());
    for dir in dirs() {
        for name in names(wanted) {
            let c = dir.join(name);
            // `python3` never falls back to a Python 2 `python.exe`.
            let right_version = !python3 || name == "python3.exe" || is_python3(&c);
            if right_version && runnable(&c) && !is_launcher(&c) {
                return Some(Found {
                    program: c,
                    leading: vec![],
                });
            }
        }
    }
    let leading = if python3 { vec!["-3"] } else { vec![] };
    dirs()
        .map(|d| d.join("py.exe"))
        .chain(search.py_launchers.iter().cloned())
        .find(|c| runnable(c) && !is_launcher(c))
        .map(|program| Found { program, leading })
}

/// Whether the `python.exe` at `exe` is a Python 3: a `python3*.dll` beside it (an install), or a
/// `pyvenv.cfg` one level up whose version is 3 (a venv's `Scripts`). Python 2 has neither.
pub fn is_python3(exe: &Path) -> bool {
    let Some(dir) = exe.parent() else {
        return false;
    };
    let has_dll = std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("python3") && name.ends_with(".dll")
        })
    });
    has_dll
        || dir.parent().is_some_and(|venv| {
            std::fs::read_to_string(venv.join("pyvenv.cfg")).is_ok_and(|cfg| {
                cfg.lines().any(|l| {
                    let (key, value) = l.split_once('=').unwrap_or((l, ""));
                    matches!(key.trim(), "version" | "version_info")
                        && value.trim().starts_with('3')
                })
            })
        })
}

/// The one-line message for nothing found: Git Bash's "command not found", then what was looked
/// for.
pub fn not_found_message(wanted: &str) -> String {
    let [a, b] = names(wanted);
    format!(
        "bash: {wanted}: command not found (no {a} or {b} on PATH, Microsoft Store aliases skipped, and no py launcher; install Python: python.org, or winget install Python.Python.3)"
    )
}

/// A file that can run: exists, is not empty, and is not an App Installer stub. An app execution
/// alias of a real Store package (zero length too) can run.
pub fn runnable(p: &Path) -> bool {
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return false;
    };
    if is_reparse_point(&md) {
        return match alias_target(p) {
            Some(alias) => !alias.is_app_installer_stub(),
            // Another kind of reparse point (a symbolic link): what it points at.
            None => std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0),
        };
    }
    md.is_file() && md.len() > 0
}

/// Whether `p` is a copy of this launcher (it holds [`LAUNCHER_MARKER`]).
pub fn is_launcher_file(p: &Path) -> bool {
    use std::io::Read as _;
    let Ok(f) = std::fs::File::open(p) else {
        return false;
    };
    let mut data = Vec::new();
    if f.take(16 << 20).read_to_end(&mut data).is_err() {
        return false;
    }
    data.windows(LAUNCHER_MARKER.len())
        .any(|w| w == LAUNCHER_MARKER)
}

#[cfg(windows)]
fn is_reparse_point(md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_md: &std::fs::Metadata) -> bool {
    false
}

/// An app execution alias (`IO_REPARSE_TAG_APPEXECLINK`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// The package family name (`Microsoft.DesktopAppInstaller_8wekyb3d8bbwe`).
    pub package: String,
    /// The program it starts.
    pub target: String,
}

impl Alias {
    /// The App Installer's "install Python from the Store" redirector.
    pub fn is_app_installer_stub(&self) -> bool {
        let target = self.target.rsplit(['\\', '/']).next().unwrap_or("");
        self.package
            .to_ascii_lowercase()
            .starts_with("microsoft.desktopappinstaller")
            || target.eq_ignore_ascii_case("AppInstallerPythonRedirector.exe")
    }
}

/// `IO_REPARSE_TAG_APPEXECLINK`.
pub const APPEXECLINK: u32 = 0x8000_001B;

/// Parses a reparse buffer (`REPARSE_DATA_BUFFER`): for an app execution alias, its version
/// (3) and NUL-terminated UTF-16 strings: package, application id, target, ...
pub fn parse_alias(buf: &[u8]) -> Option<Alias> {
    let tag = u32::from_le_bytes(buf.get(0..4)?.try_into().ok()?);
    if tag != APPEXECLINK {
        return None;
    }
    let len = usize::from(u16::from_le_bytes(buf.get(4..6)?.try_into().ok()?));
    let data = buf.get(8..8 + len)?;
    let words: Vec<u16> = data
        .get(4..)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let mut strings = words.split(|&w| w == 0).map(String::from_utf16_lossy);
    let package = strings.next()?;
    let _app_id = strings.next()?;
    let target = strings.next()?;
    Some(Alias { package, target })
}

/// The alias `p` is, read without following it.
#[cfg(windows)]
pub fn alias_target(p: &Path) -> Option<Alias> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::os::windows::io::AsRawHandle as _;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut core::ffi::c_void,
            code: u32,
            input: *const core::ffi::c_void,
            input_len: u32,
            output: *mut core::ffi::c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }
    const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;

    let f = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(p)
        .ok()?;
    let mut buf = vec![0u8; 16 * 1024];
    let mut returned = 0u32;
    // SAFETY: `f` is an open handle; `buf` has the length passed.
    let ok = unsafe {
        DeviceIoControl(
            f.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    buf.truncate(returned as usize);
    parse_alias(&buf)
}

#[cfg(not(windows))]
pub fn alias_target(_p: &Path) -> Option<Alias> {
    None
}

/// The launcher's `main`: resolve `wanted` and run it with this process's arguments and stdio.
pub fn launch(
    wanted: &str,
    run: impl FnOnce(std::process::Command) -> std::process::ExitCode,
) -> std::process::ExitCode {
    // Keep the marker in the executable (it is what tells copies of the launcher apart).
    std::hint::black_box(LAUNCHER_MARKER);
    let me = std::env::current_exe().ok();
    let is_launcher =
        |c: &Path| me.as_deref().is_some_and(|m| same_file(m, c)) || is_launcher_file(c);
    let Some(found) = resolve(wanted, &Search::from_env(), &is_launcher) else {
        eprintln!("{}", not_found_message(wanted));
        return std::process::ExitCode::from(127);
    };
    let mut cmd = std::process::Command::new(&found.program);
    cmd.args(found.leading);
    cmd.args(std::env::args_os().skip(1).collect::<Vec<OsString>>());
    run(cmd)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("skua-tools-python-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("mkdir");
            Self(p)
        }
        fn file(&self, rel: &str, data: &[u8]) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(&p, data).expect("write");
            p
        }
        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(&p).expect("mkdir");
            p
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const EXE: &[u8] = b"MZ fake interpreter";
    fn not_launcher(_: &Path) -> bool {
        false
    }
    fn found(program: PathBuf, leading: &[&'static str]) -> Option<Found> {
        Some(Found {
            program,
            leading: leading.to_vec(),
        })
    }

    #[test]
    fn python3_falls_back_to_python_exe_skipping_stubs() {
        let t = Tmp::new("fallback");
        // WindowsApps first on PATH, with zero-length stubs (as the App Installer's look).
        t.file(r"WindowsApps\python.exe", b"");
        t.file(r"WindowsApps\python3.exe", b"");
        let real = t.file(r"Python315\python.exe", EXE);
        t.file(r"Python315\python315.dll", EXE);
        let s = Search {
            path: vec![t.0.join("WindowsApps"), t.0.join("Python315")],
            py_launchers: vec![],
        };
        assert_eq!(
            resolve("python3", &s, &not_launcher),
            found(real.clone(), &[])
        );
        assert_eq!(resolve("python", &s, &not_launcher), found(real, &[]));
    }

    #[test]
    fn the_first_directory_with_either_name_wins() {
        let t = Tmp::new("order");
        let venv = t.file(r"venv\Scripts\python.exe", EXE);
        t.file(
            r"venv\pyvenv.cfg",
            b"home = C:\\Python315\nversion = 3.15.0\n",
        );
        t.file(r"Py\python3.exe", EXE);
        let s = Search {
            path: vec![t.0.join(r"venv\Scripts"), t.0.join("Py")],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), found(venv, &[]));
        // Within one directory the asked-for name comes first.
        let both = t.file(r"Both\python3.exe", EXE);
        t.file(r"Both\python.exe", EXE);
        let s = Search {
            path: vec![t.0.join("Both")],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), found(both, &[]));
        assert_eq!(
            resolve("python", &s, &not_launcher),
            found(t.0.join(r"Both\python.exe"), &[])
        );
    }

    #[test]
    fn py_launcher_when_there_is_no_interpreter_on_path() {
        let t = Tmp::new("py");
        t.file(r"WindowsApps\python3.exe", b"");
        let py = t.file(r"Windows\py.exe", EXE);
        let s = Search {
            path: vec![t.0.join("WindowsApps"), t.dir("empty")],
            py_launchers: vec![t.0.join(r"missing\py.exe"), py.clone()],
        };
        assert_eq!(
            resolve("python3", &s, &not_launcher),
            found(py.clone(), &["-3"])
        );
        assert_eq!(resolve("python", &s, &not_launcher), found(py, &[]));
    }

    #[test]
    fn copies_of_the_launcher_are_skipped() {
        let t = Tmp::new("self");
        let mut me = b"MZ ".to_vec();
        me.extend_from_slice(LAUNCHER_MARKER);
        t.file(r"skua\bin\python3.exe", &me);
        t.file(r"skua\bin\python.exe", &me);
        let real = t.file(r"Python\python.exe", EXE);
        t.file(r"Python\python3.dll", EXE);
        let s = Search {
            path: vec![t.0.join(r"skua\bin"), t.0.join("Python")],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &is_launcher_file), found(real, &[]));
    }

    #[test]
    fn nothing_found() {
        let t = Tmp::new("none");
        t.file(r"WindowsApps\python.exe", b"");
        t.file(r"WindowsApps\python3.exe", b"");
        let s = Search {
            path: vec![t.0.join("WindowsApps")],
            py_launchers: vec![t.0.join("py.exe")],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), None);
        let msg = not_found_message("python3");
        assert!(
            msg.starts_with(
                "bash: python3: command not found (no python3.exe or python.exe on PATH"
            )
        );
        assert!(!msg.contains('\n'));
    }

    #[test]
    fn python3_never_falls_back_to_python_2() {
        let t = Tmp::new("py2");
        t.file(r"Python27\python.exe", EXE);
        t.file(r"Python27\python27.dll", EXE);
        let s = Search {
            path: vec![t.0.join("Python27")],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), None);
        // `python` still runs it, as Git Bash would.
        assert_eq!(
            resolve("python", &s, &not_launcher),
            found(t.0.join(r"Python27\python.exe"), &[])
        );
        // A venv of Python 2 is not a Python 3 either.
        t.file(r"venv2\Scripts\python.exe", EXE);
        t.file(r"venv2\pyvenv.cfg", b"version = 2.7.18\n");
        let s = Search {
            path: vec![t.0.join(r"venv2\Scripts")],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), None);
    }

    #[test]
    fn relative_and_empty_path_entries_are_skipped() {
        let t = Tmp::new("relative");
        // A python3.exe in the current directory must not win through `.` or an empty entry.
        t.file(r"here\python3.exe", EXE);
        let real = t.file(r"Py\python3.exe", EXE);
        let s = Search {
            path: vec![
                PathBuf::new(),
                PathBuf::from("."),
                PathBuf::from(r"here"),
                t.0.join("Py"),
            ],
            py_launchers: vec![],
        };
        assert_eq!(resolve("python3", &s, &not_launcher), found(real, &[]));
    }

    fn alias_buffer(package: &str, app: &str, target: &str) -> Vec<u8> {
        let mut data = 3u32.to_le_bytes().to_vec();
        for s in [package, app, target, "0"] {
            for w in s.encode_utf16().chain(std::iter::once(0)) {
                data.extend_from_slice(&w.to_le_bytes());
            }
        }
        let mut buf = APPEXECLINK.to_le_bytes().to_vec();
        buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&data);
        buf
    }

    #[test]
    fn alias_buffers_tell_the_stub_from_a_store_python() {
        let stub = parse_alias(&alias_buffer(
            "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe",
            "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe!PythonRedirector",
            r"C:\Program Files\WindowsApps\Microsoft.DesktopAppInstaller_1.0_x64__8wekyb3d8bbwe\AppInstallerPythonRedirector.exe",
        ))
        .expect("parse");
        assert!(stub.is_app_installer_stub());
        let store = parse_alias(&alias_buffer(
            "PythonSoftwareFoundation.Python.3.13_qbz5n2kfra8p0",
            "PythonSoftwareFoundation.Python.3.13_qbz5n2kfra8p0!Python",
            r"C:\Program Files\WindowsApps\PythonSoftwareFoundation.Python.3.13_3.13.1_x64__qbz5n2kfra8p0\python3.13.exe",
        ))
        .expect("parse");
        assert!(!store.is_app_installer_stub());
        assert_eq!(parse_alias(&[0xA0, 0, 0, 0xA0, 0, 0, 0, 0]), None);
    }

    /// This machine, read only: an App Installer stub in WindowsApps (if there is one) is found
    /// to be a stub without being run.
    #[test]
    fn this_machines_windows_apps_stubs() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let dir = PathBuf::from(local).join(r"Microsoft\WindowsApps");
        for name in ["python.exe", "python3.exe"] {
            let p = dir.join(name);
            if let Some(alias) = alias_target(&p) {
                assert_eq!(runnable(&p), !alias.is_app_installer_stub(), "{alias:?}");
            }
        }
    }
}
