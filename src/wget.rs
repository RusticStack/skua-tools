//! `wget` for Skua Bash: the common subset of GNU Wget's options, run by Windows' own
//! `curl.exe`. Git Bash has no wget and Windows has curl but no wget, while agents type
//! `wget URL`, `wget -q -O file URL`, `wget -qO- URL`. This is not GNU Wget: options outside the
//! subset are refused with a message instead of being guessed at.
//!
//! Supported: `-q`/`--quiet`, `-v`/`--verbose`, `-nv`/`--no-verbose`, `-O FILE`/
//! `--output-document=FILE` (`-` is stdout; several URLs go into one file), `-P DIR`/
//! `--directory-prefix=DIR`, `--header=LINE` (repeatable), `-c`/`--continue`,
//! `--no-check-certificate`, `-T SECONDS`/`--timeout=SECONDS`, `-t N`/`--tries=N` (`0`/`inf`:
//! many), `-U AGENT`/`--user-agent=AGENT`, `--post-data=STRING`, `--post-file=FILE`, `-h`/
//! `--help`, `-V`/`--version`. Short options group (`-qO-`, `-qO file`).
//!
//! As GNU Wget: redirects are followed (at most 20), a file name comes from the URL's last path
//! segment (`index.html` for none), an existing file is not overwritten (`name.1`, `name.2`, ...)
//! unless `-c` appends to it, and the exit status is 0 (success), 1 (generic), 2 (usage), 3 (file
//! I/O), 4 (network), 5 (TLS verification), 8 (HTTP error status); with several URLs the lowest
//! non-zero status other than 1 wins.

/// The parsed command line.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Opts {
    /// `-q`.
    pub quiet: bool,
    /// `-nv`.
    pub no_verbose: bool,
    /// `-O`; `-` is stdout.
    pub output: Option<String>,
    /// `-P`.
    pub prefix: Option<String>,
    /// `--header`, in order.
    pub headers: Vec<String>,
    /// `-c`.
    pub cont: bool,
    /// `--no-check-certificate`.
    pub insecure: bool,
    /// `-T`, in seconds (0: none).
    pub timeout: Option<f64>,
    /// `-t`; `None` for 0/`inf` (as many as curl allows).
    pub tries: Option<Option<u32>>,
    /// `-U`.
    pub user_agent: Option<String>,
    /// `--post-data`.
    pub post_data: Option<String>,
    /// `--post-file`.
    pub post_file: Option<String>,
    /// The URLs.
    pub urls: Vec<String>,
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Parsed {
    /// Download.
    Run(Opts),
    /// `--help`.
    Help,
    /// `--version`.
    Version,
    /// A usage error: the message, and the status (2, or 1 for a missing URL as GNU Wget).
    Error(String, u8),
}

/// Options with a value, by long name, and their short letter.
const WITH_VALUE: [(&str, Option<char>); 8] = [
    ("output-document", Some('O')),
    ("directory-prefix", Some('P')),
    ("header", None),
    ("timeout", Some('T')),
    ("tries", Some('t')),
    ("user-agent", Some('U')),
    ("post-data", None),
    ("post-file", None),
];

/// Flags, by long name, and their short letter.
const FLAGS: [(&str, Option<char>); 7] = [
    ("quiet", Some('q')),
    ("verbose", Some('v')),
    ("no-verbose", None),
    ("continue", Some('c')),
    ("no-check-certificate", None),
    ("help", Some('h')),
    ("version", Some('V')),
];

/// GNU Wget options this wget refuses (it does not do what they ask), long and short.
const UNSUPPORTED: &[&str] = &[
    "recursive",
    "mirror",
    "level",
    "no-parent",
    "page-requisites",
    "convert-links",
    "timestamping",
    "no-clobber",
    "input-file",
    "background",
    "output-file",
    "append-output",
    "no-directories",
    "force-directories",
    "accept",
    "reject",
    "execute",
    "spider",
    "limit-rate",
    "user",
    "password",
    "http-user",
    "http-password",
    "server-response",
    "method",
    "body-data",
    "body-file",
    "content-disposition",
    "max-redirect",
    "ca-certificate",
    "certificate",
    "private-key",
    "no-proxy",
    "load-cookies",
    "save-cookies",
    "referer",
    "wait",
    "random-wait",
    "quota",
    "inet4-only",
    "inet6-only",
    "show-progress",
    "progress",
    "read-timeout",
];
const UNSUPPORTED_SHORT: &str = "rlNkKpibowaAReSndxHDQ46";

/// The options this wget has, for messages.
pub const SUPPORTED: &str =
    "-q -v -nv -O -P --header -c --no-check-certificate -T -t -U --post-data --post-file -h -V";

fn unsupported(opt: &str) -> Parsed {
    Parsed::Error(
        format!(
            "wget: option '{opt}' is not supported by the skua-tools wget (a front end to curl, not GNU Wget); supported: {SUPPORTED}"
        ),
        2,
    )
}

fn unknown(opt: &str) -> Parsed {
    Parsed::Error(
        format!(
            "wget: unrecognized option '{opt}'\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options."
        ),
        2,
    )
}

fn set(o: &mut Opts, name: &str, value: &str) -> Result<(), String> {
    let bad = |what: &str| {
        format!("wget: {name}: Invalid {what} '{value}'.\nTry `wget --help' for more options.")
    };
    match name {
        "output-document" => o.output = Some(value.to_owned()),
        "directory-prefix" => o.prefix = Some(value.to_owned()),
        "header" => o.headers.push(value.to_owned()),
        "timeout" => {
            o.timeout = Some(
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|t| t.is_finite() && *t >= 0.0)
                    .ok_or_else(|| bad("time period"))?,
            );
        }
        "tries" => {
            o.tries = Some(if value == "0" || value.eq_ignore_ascii_case("inf") {
                None
            } else {
                Some(value.parse::<u32>().map_err(|_| bad("number"))?)
            });
        }
        "user-agent" => o.user_agent = Some(value.to_owned()),
        "post-data" => o.post_data = Some(value.to_owned()),
        "post-file" => o.post_file = Some(value.to_owned()),
        _ => return Err(format!("wget: unrecognized option '--{name}'")),
    }
    Ok(())
}

/// `true` to stop with help or version.
fn flag(o: &mut Opts, name: &str) -> Option<Parsed> {
    match name {
        "quiet" => o.quiet = true,
        "verbose" => {
            o.quiet = false;
            o.no_verbose = false;
        }
        "no-verbose" => o.no_verbose = true,
        "continue" => o.cont = true,
        "no-check-certificate" => o.insecure = true,
        "help" => return Some(Parsed::Help),
        "version" => return Some(Parsed::Version),
        _ => {}
    }
    None
}

/// Parses the arguments after the program name.
pub fn parse(args: &[String]) -> Parsed {
    let mut o = Opts::default();
    let mut i = 0;
    let mut stop = None;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" {
            o.urls.extend(args[i..].iter().cloned());
            break;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_owned())),
                None => (long, None),
            };
            if WITH_VALUE.iter().any(|(n, _)| *n == name) {
                let value = match inline {
                    Some(v) => v,
                    None if i < args.len() => {
                        i += 1;
                        args[i - 1].clone()
                    }
                    None => {
                        return Parsed::Error(
                            format!(
                                "wget: option '--{name}' requires an argument\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options."
                            ),
                            2,
                        );
                    }
                };
                if let Err(e) = set(&mut o, name, &value) {
                    return Parsed::Error(e, 2);
                }
            } else if FLAGS.iter().any(|(n, _)| *n == name) && inline.is_none() {
                stop = stop.or(flag(&mut o, name));
            } else if UNSUPPORTED.contains(&name) {
                return unsupported(&format!("--{name}"));
            } else {
                return unknown(a);
            }
            continue;
        }
        if a == "-nv" {
            o.no_verbose = true;
            continue;
        }
        if a.len() > 1 && a.starts_with('-') {
            let chars: Vec<char> = a[1..].chars().collect();
            let mut k = 0;
            while k < chars.len() {
                let c = chars[k];
                k += 1;
                if let Some((name, _)) = WITH_VALUE.iter().find(|(_, s)| *s == Some(c)) {
                    let rest: String = chars[k..].iter().collect();
                    let value = if !rest.is_empty() {
                        rest
                    } else if i < args.len() {
                        i += 1;
                        args[i - 1].clone()
                    } else {
                        return Parsed::Error(
                            format!(
                                "wget: option requires an argument -- '{c}'\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options."
                            ),
                            2,
                        );
                    };
                    if let Err(e) = set(&mut o, name, &value) {
                        return Parsed::Error(e, 2);
                    }
                    break;
                } else if let Some((name, _)) = FLAGS.iter().find(|(_, s)| *s == Some(c)) {
                    stop = stop.or(flag(&mut o, name));
                } else if UNSUPPORTED_SHORT.contains(c) {
                    return unsupported(a);
                } else {
                    return Parsed::Error(
                        format!(
                            "wget: invalid option -- '{c}'\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options."
                        ),
                        2,
                    );
                }
            }
            continue;
        }
        o.urls.push(a.clone());
    }
    if let Some(stop) = stop {
        return stop;
    }
    if o.urls.is_empty() {
        return Parsed::Error(
            "wget: missing URL\nUsage: wget [OPTION]... [URL]...\n\nTry `wget --help' for more options."
                .to_owned(),
            1,
        );
    }
    if o.post_data.is_some() && o.post_file.is_some() {
        return Parsed::Error(
            "wget: --post-data and --post-file cannot be used together".to_owned(),
            2,
        );
    }
    Parsed::Run(o)
}

/// A URL with no scheme gets `http://`, as GNU Wget does.
pub fn with_scheme(url: &str) -> String {
    if url.contains("://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    }
}

/// The local file name GNU Wget picks for `url`: the last path segment, without the query or
/// fragment, percent-decoded where safe; `index.html` when there is none.
pub fn file_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.split(['#', '?']).next().unwrap_or("");
    let path = rest.find('/').map_or("", |i| &rest[i..]);
    let seg = path.rsplit('/').next().unwrap_or("");
    let decoded = percent_decode(seg);
    let safe: String = decoded
        .chars()
        .map(|c| {
            if "\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if safe.is_empty() || safe == "." || safe == ".." {
        "index.html".to_owned()
    } else {
        safe
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 3 <= b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            let v = u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(0);
            if v != b'/' && v != 0 {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// GNU Wget's exit status for a curl exit status.
pub fn exit_status(curl: i32) -> u8 {
    match curl {
        0 => 0,
        // HTTP status >= 400 with --fail; too many redirects.
        22 | 47 => 8,
        // DNS, connect, timeout, empty reply, send/receive errors, partial file, HTTP/2 framing.
        5..=7 | 16 | 18 | 28 | 52 | 55 | 56 | 92 => 4,
        // TLS handshake and certificate problems.
        35 | 51 | 53 | 54 | 58 | 59 | 60 | 64 | 66 | 77 | 80 | 83 | 90 | 91 => 5,
        // Writing the output, reading the post file.
        23 | 26 => 3,
        _ => 1,
    }
}

/// The overall status for several URLs: the lowest non-zero status other than 1, else 1 if any
/// failed, else 0.
pub fn combine(statuses: &[u8]) -> u8 {
    statuses
        .iter()
        .copied()
        .filter(|&s| s > 1)
        .min()
        .unwrap_or_else(|| u8::from(statuses.contains(&1)))
}

/// The curl arguments for one URL (everything but the output, which the caller wires).
pub fn curl_args(o: &Opts, url: &str, resume_from: Option<u64>) -> Vec<String> {
    let mut a: Vec<String> = [
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-redirs",
        "20",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if o.insecure {
        a.push("--insecure".to_owned());
    }
    a.push("--user-agent".to_owned());
    a.push(
        o.user_agent
            .clone()
            .unwrap_or_else(|| "Wget/1.25.0 (skua-tools; curl)".to_owned()),
    );
    for h in &o.headers {
        a.push("--header".to_owned());
        a.push(h.clone());
    }
    if let Some(t) = o.timeout.filter(|t| *t > 0.0) {
        let t = format!("{t}");
        a.extend(["--connect-timeout".to_owned(), t.clone()]);
        // No data for that long ends the transfer, as wget's read timeout.
        a.extend([
            "--speed-time".to_owned(),
            format!("{}", o.timeout.map_or(1, |t| t.ceil().max(1.0) as u64)),
            "--speed-limit".to_owned(),
            "1".to_owned(),
        ]);
    }
    if let Some(tries) = o.tries {
        let retries = tries.map_or(100, |n| n.saturating_sub(1));
        if retries > 0 {
            a.extend(["--retry".to_owned(), retries.to_string()]);
        }
    }
    if let Some(d) = &o.post_data {
        a.extend(["--data-raw".to_owned(), d.clone()]);
    }
    if let Some(f) = &o.post_file {
        a.extend(["--data-binary".to_owned(), format!("@{f}")]);
    }
    if let Some(n) = resume_from.filter(|n| *n > 0) {
        a.extend(["--continue-at".to_owned(), n.to_string()]);
    }
    a.extend(["--url".to_owned(), url.to_owned()]);
    a
}

/// The usage text for `--help`.
pub const HELP: &str = "\
skua-tools wget: a wget front end to curl.exe (not GNU Wget).
Usage: wget [OPTION]... [URL]...

  -V,  --version                  show the version and the curl it uses
  -h,  --help                     this help
  -q,  --quiet                    print nothing
  -v,  --verbose                  print what is saved (the default)
  -nv, --no-verbose               one line per URL
  -O,  --output-document=FILE     write to FILE ('-': standard output)
  -P,  --directory-prefix=DIR     save files under DIR
  -c,  --continue                 resume a partly downloaded file
  -T,  --timeout=SECONDS          connect and read timeout
  -t,  --tries=NUMBER             tries (0 or inf: many)
  -U,  --user-agent=AGENT         User-Agent header
       --header=STRING            an extra header line (repeatable)
       --post-data=STRING         POST STRING (application/x-www-form-urlencoded)
       --post-file=FILE           POST the contents of FILE
       --no-check-certificate     do not verify the server's certificate

Other GNU Wget options are refused with a message. Exit status: 0 ok, 1 generic error,
2 usage, 3 file I/O, 4 network failure, 5 TLS verification, 8 HTTP error status.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Parsed {
        parse(&args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }
    fn run(args: &[&str]) -> Opts {
        match p(args) {
            Parsed::Run(o) => o,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn common_agent_forms() {
        let o = run(&["-qO-", "http://h/x"]);
        assert!(o.quiet);
        assert_eq!(o.output.as_deref(), Some("-"));
        let o = run(&["-q", "-O", "out.json", "http://h/x"]);
        assert_eq!(o.output.as_deref(), Some("out.json"));
        let o = run(&[
            "--output-document=f",
            "-P",
            "dl",
            "--header=A: b",
            "--header",
            "C: d",
            "u",
        ]);
        assert_eq!(o.output.as_deref(), Some("f"));
        assert_eq!(o.prefix.as_deref(), Some("dl"));
        assert_eq!(o.headers, ["A: b", "C: d"]);
        let o = run(&[
            "-c",
            "--no-check-certificate",
            "-T",
            "5",
            "-t",
            "3",
            "-U",
            "ua",
            "--post-data=a=1",
            "u",
        ]);
        assert!(o.cont && o.insecure);
        assert_eq!(o.timeout, Some(5.0));
        assert_eq!(o.tries, Some(Some(3)));
        assert_eq!(o.user_agent.as_deref(), Some("ua"));
        assert_eq!(o.post_data.as_deref(), Some("a=1"));
        assert_eq!(run(&["--tries=inf", "u"]).tries, Some(None));
        assert!(run(&["-nv", "u"]).no_verbose);
        assert_eq!(run(&["--", "-u"]).urls, ["-u"]);
    }

    #[test]
    fn help_version_and_errors() {
        assert_eq!(p(&["--help"]), Parsed::Help);
        assert_eq!(p(&["-V"]), Parsed::Version);
        assert!(matches!(p(&[]), Parsed::Error(m, 1) if m.starts_with("wget: missing URL")));
        assert!(
            matches!(p(&["-r", "u"]), Parsed::Error(m, 2) if m.contains("'-r' is not supported"))
        );
        assert!(
            matches!(p(&["--mirror", "u"]), Parsed::Error(m, 2) if m.contains("'--mirror' is not supported"))
        );
        assert!(
            matches!(p(&["--bogus", "u"]), Parsed::Error(m, 2) if m.starts_with("wget: unrecognized option '--bogus'"))
        );
        assert!(
            matches!(p(&["-Z", "u"]), Parsed::Error(m, 2) if m.starts_with("wget: invalid option -- 'Z'"))
        );
        assert!(matches!(p(&["-O"]), Parsed::Error(_, 2)));
        assert!(
            matches!(p(&["-T", "x", "u"]), Parsed::Error(m, 2) if m.contains("Invalid time period"))
        );
    }

    #[test]
    fn file_names_as_wget() {
        assert_eq!(file_name("http://h/a/b/file.tar.gz?x=1#f"), "file.tar.gz");
        assert_eq!(file_name("http://h/"), "index.html");
        assert_eq!(file_name("http://h"), "index.html");
        assert_eq!(file_name("https://h/dir/"), "index.html");
        assert_eq!(file_name("http://h/a%20b.txt"), "a b.txt");
        assert_eq!(file_name("http://h/x%2Fy"), "x%2Fy");
        assert_eq!(with_scheme("example.com/x"), "http://example.com/x");
    }

    #[test]
    fn statuses() {
        assert_eq!(exit_status(0), 0);
        assert_eq!(exit_status(22), 8);
        assert_eq!(exit_status(7), 4);
        assert_eq!(exit_status(28), 4);
        assert_eq!(exit_status(60), 5);
        assert_eq!(exit_status(23), 3);
        assert_eq!(exit_status(3), 1);
        assert_eq!(combine(&[0, 8, 4]), 4);
        assert_eq!(combine(&[0, 1]), 1);
        assert_eq!(combine(&[1, 8]), 8);
        assert_eq!(combine(&[0, 0]), 0);
    }

    #[test]
    fn curl_command_line() {
        let o = run(&["-T", "2", "-t", "3", "--header=X: y", "--post-data=a", "u"]);
        let a = curl_args(&o, "http://h/u", Some(5));
        let s = a.join(" ");
        assert!(
            s.starts_with("--silent --show-error --fail --location --max-redirs 20"),
            "{s}"
        );
        assert!(
            s.contains("--connect-timeout 2 --speed-time 2 --speed-limit 1"),
            "{s}"
        );
        assert!(
            s.contains("--retry 2") && s.contains("--header X: y"),
            "{s}"
        );
        assert!(
            s.contains("--data-raw a") && s.contains("--continue-at 5"),
            "{s}"
        );
        assert!(s.ends_with("--url http://h/u"), "{s}");
    }
}
