//! Finding Git for Windows without assuming its internal layout.
//!
//! Git for Windows 2.56 replaced `mingw64\` with `ucrt64\`; only `cmd\git.exe` is promised to
//! stay where it is. So an install is recognised by `<root>\cmd\git.exe` alone, and a tool is
//! found by probing every directory that holds programs in one of the layouts
//! ([`TOOL_DIRS`]) for the wanted file, never by one fixed path.
//!
//! Where an install is looked for, in order ([`Probe::from_env`]):
//!
//! 1. `git.exe` on `PATH` (in `cmd\`, `bin\`, `<env>\bin\` or `usr\bin\` of an install, or a
//!    Scoop shim): the root is the nearest parent with `cmd\git.exe`;
//! 2. `%ProgramFiles%\Git` (also `ProgramW6432`, `ProgramFiles(x86)`) and
//!    `%LOCALAPPDATA%\Programs\Git`;
//! 3. `InstallPath` under `HKLM\SOFTWARE\GitForWindows`, then `HKCU\SOFTWARE\GitForWindows`
//!    (read only; skipped when `SKUA_TOOLS_IGNORE_REGISTRY` is set, for tests);
//! 4. Scoop's `apps\git\current` (`%SCOOP%`, `%USERPROFILE%\scoop`).
//!
//! A tool-specific override (`SKUA_PERL`) is the caller's, checked before all of these.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The directories of an install that hold programs, probed in this order: the MSYS base
/// (`usr\bin`, where perl, ssh and less live), then the MinGW environments of the current
/// (`ucrt64`), older (`mingw64`) and ARM64 (`clangarm64`) layouts.
pub const TOOL_DIRS: [&str; 4] = [r"usr\bin", r"ucrt64\bin", r"mingw64\bin", r"clangarm64\bin"];

/// What was looked for, for "not found" messages.
pub const LOOKED_FOR: &str = r"git.exe on PATH, %ProgramFiles%\Git\cmd\git.exe, %LOCALAPPDATA%\Programs\Git\cmd\git.exe, InstallPath under HKLM and HKCU\SOFTWARE\GitForWindows";

/// The places to look, gathered from the environment and the registry (or given by a test).
#[derive(Debug, Default, Clone)]
pub struct Probe {
    /// The `PATH` value.
    pub path: Option<OsString>,
    /// Install roots to try after `PATH`, in order (`%ProgramFiles%\Git`, ...).
    pub roots: Vec<PathBuf>,
}

impl Probe {
    /// The places this process's environment and the registry name.
    pub fn from_env() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut roots = Vec::new();
        for k in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            roots.extend(var(k).map(|p| p.join("Git")));
        }
        roots.extend(var("LOCALAPPDATA").map(|p| p.join("Programs").join("Git")));
        roots.extend(registry_install_paths());
        roots.extend(var("SCOOP").map(|p| p.join("apps").join("git").join("current")));
        roots.extend(
            var("USERPROFILE").map(|p| p.join("scoop").join("apps").join("git").join("current")),
        );
        Self {
            path: std::env::var_os("PATH"),
            roots,
        }
    }
}

/// One Git for Windows install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitForWindows {
    /// The install root (the directory that holds `cmd\git.exe`).
    pub root: PathBuf,
}

impl GitForWindows {
    /// The stable entry point, `<root>\cmd\git.exe`.
    pub fn git(&self) -> PathBuf {
        self.root.join("cmd").join("git.exe")
    }

    /// The first of [`TOOL_DIRS`] that has `file` (`perl.exe`).
    pub fn tool(&self, file: &str) -> Option<PathBuf> {
        TOOL_DIRS
            .iter()
            .map(|dir| self.root.join(dir).join(file))
            .find(|p| p.is_file())
    }
}

/// `root` is an install: it has `cmd\git.exe`.
fn is_root(root: &Path) -> bool {
    root.join("cmd").join("git.exe").is_file()
}

/// The install that a directory holding `git.exe` belongs to: the directory's parent or
/// grandparent with `cmd\git.exe` (`<root>\cmd`, `<root>\bin`, `<root>\ucrt64\bin`,
/// `<root>\usr\bin`), or for a Scoop shim directory, Scoop's `apps\git\current`.
pub fn root_of_git_dir(dir: &Path) -> Option<PathBuf> {
    if dir
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("shims"))
    {
        let current = dir.parent()?.join("apps").join("git").join("current");
        return is_root(&current).then_some(current);
    }
    dir.ancestors()
        .skip(1)
        .take(2)
        .find(|p| is_root(p))
        .map(Path::to_owned)
}

/// Every install `probe` finds, in its order, each once.
pub fn installs(probe: &Probe) -> Vec<GitForWindows> {
    let mut found: Vec<GitForWindows> = Vec::new();
    let mut add = |root: PathBuf| {
        let key = root.canonicalize().unwrap_or_else(|_| root.clone());
        let seen = found
            .iter()
            .any(|g| g.root.canonicalize().unwrap_or_else(|_| g.root.clone()) == key);
        if !seen {
            found.push(GitForWindows { root });
        }
    };
    if let Some(path) = &probe.path {
        for dir in std::env::split_paths(path) {
            if dir.join("git.exe").is_file()
                && let Some(root) = root_of_git_dir(&dir)
            {
                add(root);
            }
        }
    }
    for root in &probe.roots {
        if is_root(root) {
            add(root.clone());
        }
    }
    found
}

/// The first install that has `file` in one of its [`TOOL_DIRS`], and the tool's path.
///
/// `Err` holds the installs found (empty when there is none), for the message.
pub fn find_tool(probe: &Probe, file: &str) -> Result<PathBuf, Vec<GitForWindows>> {
    let all = installs(probe);
    all.iter().find_map(|g| g.tool(file)).ok_or(all)
}

/// The "not found" message for `tool` (`perl`): what was looked for and how to point at it.
pub fn not_found_message(
    tool: &str,
    file: &str,
    override_var: &str,
    installs: &[GitForWindows],
) -> String {
    if installs.is_empty() {
        format!(
            "{tool}: not found: Skua Bash runs Git for Windows' {tool}, and no Git for Windows was found (looked for {LOOKED_FOR}); install Git for Windows, or set {override_var} to a {file}"
        )
    } else {
        let roots: Vec<String> = installs
            .iter()
            .map(|g| g.root.display().to_string())
            .collect();
        format!(
            "{tool}: not found: no {file} in Git for Windows at {} (looked in {}); MinGit has no {tool}: install the full Git for Windows, or set {override_var} to a {file}",
            roots.join(", "),
            TOOL_DIRS.join(", ")
        )
    }
}

/// `InstallPath` under `HKLM` then `HKCU` `SOFTWARE\GitForWindows`, where set.
#[cfg(windows)]
fn registry_install_paths() -> Vec<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            key: isize,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut core::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002_u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x2;

    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let subkey = wide(r"SOFTWARE\GitForWindows");
    let value = wide("InstallPath");
    let mut out = Vec::new();
    // Tests that fake "nothing installed" switch the registry off (it is the machine's).
    if std::env::var_os("SKUA_TOOLS_IGNORE_REGISTRY").is_some() {
        return out;
    }
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the key and value names are NUL-terminated; `buf` has `len` bytes.
        let rc = unsafe {
            RegGetValueW(
                hive,
                subkey.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if rc != 0 {
            continue;
        }
        let chars = (len as usize / 2).min(buf.len());
        let text = &buf[..chars];
        let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        if end > 0 {
            out.push(PathBuf::from(OsString::from_wide(&text[..end])));
        }
    }
    out
}

#[cfg(not(windows))]
fn registry_install_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp directory removed on drop.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "skua-tools-gitwin-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("mkdir");
            Self(p)
        }
        fn file(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(&p, b"MZ").expect("write");
            p
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A Git for Windows 2.56+ tree (UCRT64).
    fn ucrt64(t: &Tmp, root: &str) -> PathBuf {
        for f in [
            "cmd/git.exe",
            "bin/git.exe",
            "bin/bash.exe",
            "ucrt64/bin/git.exe",
            "usr/bin/perl.exe",
            "usr/bin/sh.exe",
            "usr/bin/less.exe",
            "usr/bin/ssh.exe",
        ] {
            t.file(&format!("{root}/{f}"));
        }
        t.0.join(root)
    }

    /// A Git for Windows tree before 2.56 (MINGW64).
    fn mingw64(t: &Tmp, root: &str) -> PathBuf {
        for f in [
            "cmd/git.exe",
            "mingw64/bin/git.exe",
            "mingw64/bin/curl.exe",
            "usr/bin/perl.exe",
        ] {
            t.file(&format!("{root}/{f}"));
        }
        t.0.join(root)
    }

    fn path_of(dirs: &[PathBuf]) -> Option<OsString> {
        std::env::join_paths(dirs).ok()
    }

    #[test]
    fn ucrt64_layout_with_only_cmd_on_path() {
        let t = Tmp::new("ucrt");
        let root = ucrt64(&t, "Git");
        let probe = Probe {
            path: path_of(&[t.0.join("elsewhere"), root.join("cmd")]),
            roots: vec![],
        };
        assert_eq!(installs(&probe), vec![GitForWindows { root: root.clone() }]);
        assert_eq!(
            find_tool(&probe, "perl.exe"),
            Ok(root.join(r"usr\bin\perl.exe"))
        );
        assert_eq!(
            find_tool(&probe, "less.exe"),
            Ok(root.join(r"usr\bin\less.exe"))
        );
        assert_eq!(
            find_tool(&probe, "git.exe"),
            Ok(root.join(r"ucrt64\bin\git.exe"))
        );
        assert_eq!(installs(&probe)[0].git(), root.join(r"cmd\git.exe"));
    }

    #[test]
    fn mingw64_layout_tools_are_probed_not_assumed() {
        let t = Tmp::new("mingw");
        let root = mingw64(&t, "Git");
        let probe = Probe {
            path: path_of(&[root.join(r"mingw64\bin")]),
            roots: vec![],
        };
        assert_eq!(
            find_tool(&probe, "curl.exe"),
            Ok(root.join(r"mingw64\bin\curl.exe"))
        );
        assert_eq!(
            find_tool(&probe, "perl.exe"),
            Ok(root.join(r"usr\bin\perl.exe"))
        );
        assert!(find_tool(&probe, "ssh.exe").is_err());
    }

    #[test]
    fn root_from_each_kind_of_git_dir() {
        let t = Tmp::new("dirs");
        let root = ucrt64(&t, "G");
        t.file("G/usr/bin/git.exe");
        for dir in ["cmd", "bin", r"ucrt64\bin", r"usr\bin"] {
            assert_eq!(
                root_of_git_dir(&root.join(dir)),
                Some(root.clone()),
                "{dir}"
            );
        }
        // A git.exe that is not part of a Git for Windows tree.
        t.file("other/bin/git.exe");
        assert_eq!(root_of_git_dir(&t.0.join(r"other\bin")), None);
    }

    #[test]
    fn scoop_shims_point_at_apps_git_current() {
        let t = Tmp::new("scoop");
        t.file("scoop/shims/git.exe");
        let root = ucrt64(&t, "scoop/apps/git/current");
        let probe = Probe {
            path: path_of(&[t.0.join(r"scoop\shims")]),
            roots: vec![],
        };
        assert_eq!(
            find_tool(&probe, "perl.exe"),
            Ok(root.join(r"usr\bin\perl.exe"))
        );
    }

    #[test]
    fn install_roots_after_path_and_first_with_the_tool_wins() {
        let t = Tmp::new("roots");
        // On PATH: a MinGit-like tree without perl; then a full install among the roots.
        for f in [
            "MinGit/cmd/git.exe",
            "MinGit/ucrt64/bin/git.exe",
            "MinGit/usr/bin/sh.exe",
        ] {
            t.file(f);
        }
        let full = ucrt64(&t, "PF/Git");
        let probe = Probe {
            path: path_of(&[t.0.join(r"MinGit\cmd")]),
            roots: vec![t.0.join(r"nothing\Git"), full.clone(), full.clone()],
        };
        assert_eq!(installs(&probe).len(), 2);
        assert_eq!(
            find_tool(&probe, "perl.exe"),
            Ok(full.join(r"usr\bin\perl.exe"))
        );
        assert_eq!(
            find_tool(&probe, "sh.exe"),
            Ok(t.0.join(r"MinGit\usr\bin\sh.exe"))
        );
    }

    #[test]
    fn nothing_installed() {
        let t = Tmp::new("none");
        let probe = Probe {
            path: path_of(&[t.0.join("a"), t.0.join("b")]),
            roots: vec![t.0.join(r"PF\Git"), t.0.join(r"LocalAppData\Programs\Git")],
        };
        let err = find_tool(&probe, "perl.exe").expect_err("nothing there");
        assert!(err.is_empty());
        let msg = not_found_message("perl", "perl.exe", "SKUA_PERL", &err);
        assert!(msg.starts_with("perl: not found: "), "{msg}");
        assert!(
            msg.contains(LOOKED_FOR) && msg.contains("set SKUA_PERL"),
            "{msg}"
        );
        assert!(!msg.contains('\n'));
    }

    #[test]
    fn found_install_without_the_tool_names_the_dirs_probed() {
        let t = Tmp::new("noperl");
        t.file("Git/cmd/git.exe");
        let probe = Probe {
            path: None,
            roots: vec![t.0.join("Git")],
        };
        let err = find_tool(&probe, "perl.exe").expect_err("no perl");
        let msg = not_found_message("perl", "perl.exe", "SKUA_PERL", &err);
        assert!(
            msg.contains(r"usr\bin, ucrt64\bin, mingw64\bin, clangarm64\bin"),
            "{msg}"
        );
    }

    /// The real machine, read only: whatever is installed is found through `cmd\git.exe`.
    #[test]
    fn this_machine_if_installed() {
        let probe = Probe::from_env();
        for g in installs(&probe) {
            assert!(g.git().is_file(), "{}", g.root.display());
        }
    }
}
