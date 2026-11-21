//! `wget` for Skua Bash: GNU Wget's common options on top of Windows' `curl.exe` (see
//! `skua_tools::wget` for the subset and the exit statuses). curl is
//! `%SystemRoot%\System32\curl.exe`, else `curl.exe` on `PATH`; off Windows, `curl` on `PATH`.

use skua_tools::wget::{self, Opts, Parsed};
use std::fs::{File, OpenOptions};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// `curl.exe`, or `curl` where programs have no extension.
const CURL: &str = if cfg!(windows) { "curl.exe" } else { "curl" };
/// Where curl is looked for, for the messages.
const CURL_PLACES: &str = if cfg!(windows) {
    "%SystemRoot%\\System32\\curl.exe and curl.exe on PATH"
} else {
    "curl on PATH"
};

fn find_curl() -> Option<PathBuf> {
    let system = std::env::var_os("SystemRoot")
        .filter(|v| cfg!(windows) && !v.is_empty())
        .map(|r| PathBuf::from(r).join("System32").join(CURL));
    let on_path = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(CURL))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    system.into_iter().chain(on_path).find(|c| c.is_file())
}

fn curl_version(curl: &Path) -> String {
    Command::new(curl)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "version unknown".to_owned())
}

/// Where a URL's body goes.
enum Target {
    Stdout,
    /// A file, the bytes already in it (for `-c`), and whether this run created it.
    File(PathBuf, u64, bool),
}

/// `name`, or `name.1`, `name.2`, ... for the first that does not exist (GNU Wget's default).
fn unused_name(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_owned();
    }
    (1u32..)
        .map(|n| {
            let mut s = path.as_os_str().to_owned();
            s.push(format!(".{n}"));
            PathBuf::from(s)
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_owned())
}

fn open_target(o: &Opts, url: &str, first: bool) -> std::io::Result<(Target, Option<File>)> {
    let prefix = o.prefix.as_deref().map(PathBuf::from);
    let under = |p: PathBuf| match &prefix {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p,
    };
    if o.output.as_deref() == Some("-") {
        return Ok((Target::Stdout, None));
    }
    if let Some(out) = &o.output {
        // `-O FILE`: one file for every URL, truncated once (appended to with `-c`). As in GNU
        // Wget, `-O` works like shell redirection: FILE is used as given, `-P` does not apply.
        let path = PathBuf::from(out);
        let existed = path.exists();
        let mut opts = OpenOptions::new();
        if first && !o.cont {
            opts.write(true).create(true).truncate(true);
        } else {
            opts.append(true).create(true);
        }
        let f = opts.open(&path)?;
        let have = if o.cont && first {
            f.metadata()?.len()
        } else {
            0
        };
        return Ok((Target::File(path, have, !existed), Some(f)));
    }
    if let Some(dir) = &prefix {
        std::fs::create_dir_all(dir)?;
    }
    let path = under(PathBuf::from(wget::file_name(url)));
    if o.cont && path.is_file() {
        let f = OpenOptions::new().append(true).open(&path)?;
        let have = f.metadata()?.len();
        return Ok((Target::File(path, have, false), Some(f)));
    }
    // `create_new`: never truncates a file that appeared after `unused_name` looked; then the
    // next free name is tried.
    let mut last = None;
    for _ in 0..100 {
        let name = unused_name(&path);
        match OpenOptions::new().write(true).create_new(true).open(&name) {
            Ok(f) => return Ok((Target::File(name, 0, true), Some(f))),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AlreadyExists)))
}

fn shown(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// Downloads one URL; returns its wget status.
fn fetch(curl: &Path, o: &Opts, raw_url: &str, first: bool) -> u8 {
    let url = wget::with_scheme(raw_url);
    let (target, file) = match open_target(o, &url, first) {
        Ok(t) => t,
        Err(e) => {
            if !o.quiet {
                eprintln!("wget: {}: {}", raw_url, skua_tools::gnu_error(&e));
            }
            return 3;
        }
    };
    let resume = match &target {
        Target::File(_, have, _) if o.cont => Some(*have),
        _ => None,
    };
    if !o.quiet && !o.no_verbose {
        match &target {
            Target::Stdout => eprintln!("--  {url}\nSaving to: 'STDOUT'"),
            Target::File(p, _, _) => eprintln!("--  {url}\nSaving to: '{}'", shown(p)),
        }
    }
    let mut cmd = Command::new(curl);
    cmd.args(wget::curl_args(o, &url, resume))
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(f) = &file {
        match f.try_clone() {
            Ok(f) => {
                cmd.stdout(Stdio::from(f));
            }
            Err(e) => {
                eprintln!("wget: {}", skua_tools::gnu_error(&e));
                return 3;
            }
        }
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("wget: cannot run {}: {e}", curl.display());
            return 1;
        }
    };
    // Bytes, then lossy: curl's messages can carry non-UTF-8 (a URL, a server's text).
    let mut raw = Vec::new();
    if let Some(mut s) = child.stderr.take()
        && let Err(e) = s.read_to_end(&mut raw)
    {
        eprintln!("wget: reading curl's errors: {e}");
    }
    let err = String::from_utf8_lossy(&raw);
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
    let message = err
        .lines()
        .last()
        .unwrap_or("")
        .split_once(") ")
        .map_or_else(|| err.trim().to_owned(), |(_, m)| m.trim().to_owned());
    // `-c` on a file that is already complete: the server answers 416.
    if code == 22 && o.cont && resume.is_some_and(|n| n > 0) && message.contains("416") {
        if !o.quiet {
            eprintln!("The file is already fully retrieved; nothing to do.");
        }
        return 0;
    }
    let status = wget::exit_status(code);
    let size = match &target {
        Target::File(p, _, _) => std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
        Target::Stdout => 0,
    };
    if status != 0 {
        // As GNU Wget, a file named after the URL that got nothing is not left behind.
        if let Target::File(p, _, true) = &target
            && o.output.is_none()
            && size == 0
        {
            drop(file);
            let _ = std::fs::remove_file(p);
        }
        if !o.quiet {
            if code == 22 {
                let status_code = message
                    .rsplit(' ')
                    .next()
                    .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or("");
                eprintln!("{url}:\nERROR {status_code}: {message}");
            } else {
                eprintln!("wget: {url}: {message}");
            }
        }
        return status;
    }
    // curl ends a resume of a complete file (416 with the same size) as a success with no data.
    if let Target::File(_, have, _) = &target
        && o.cont
        && *have > 0
        && size == *have
    {
        if !o.quiet {
            eprintln!("The file is already fully retrieved; nothing to do.");
        }
        return 0;
    }
    if !o.quiet {
        match &target {
            Target::Stdout => {}
            Target::File(p, _, _) if o.no_verbose => {
                eprintln!("{url} -> \"{}\" [{size}]", shown(p))
            }
            Target::File(p, _, _) => eprintln!("'{}' saved [{size}]", shown(p)),
        }
    }
    0
}

fn main() -> ExitCode {
    // `wget --version | head -1`, `wget -qO- URL | head`: a closed stdout ends quietly with 141.
    skua_tools::quiet_exit_on_stdout_failure(|| {});
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = match wget::parse(&args) {
        Parsed::Run(o) => o,
        Parsed::Help => {
            print!("{}", wget::HELP);
            return ExitCode::SUCCESS;
        }
        Parsed::Version => {
            println!(
                "skua-tools wget {}: a wget front end to curl.exe, not GNU Wget.",
                env!("CARGO_PKG_VERSION")
            );
            match find_curl() {
                Some(c) => println!("It runs {} ({}).", c.display(), curl_version(&c)),
                None => println!("It needs {CURL} ({CURL_PLACES}), which was not found."),
            }
            println!("Supported options: {}", wget::SUPPORTED);
            return ExitCode::SUCCESS;
        }
        Parsed::Error(message, status) => {
            eprintln!("{message}");
            return ExitCode::from(status);
        }
    };
    let Some(curl) = find_curl() else {
        eprintln!("wget: {CURL} not found (looked for {CURL_PLACES}); this wget runs curl");
        return ExitCode::from(1);
    };
    let statuses: Vec<u8> = o
        .urls
        .iter()
        .enumerate()
        .map(|(i, u)| fetch(&curl, &o, u, i == 0))
        .collect();
    ExitCode::from(wget::combine(&statuses))
}
