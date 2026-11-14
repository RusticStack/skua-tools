//! `wget` end to end against a small HTTP server on 127.0.0.1 (an OS-assigned port), through
//! Windows' own curl.exe. No external network. Skipped when there is no System32 curl.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn have_curl() -> bool {
    std::env::var_os("SystemRoot")
        .is_some_and(|r| PathBuf::from(r).join(r"System32\curl.exe").is_file())
}

/// Serves until the test process ends.
fn server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = handle(stream);
            });
        }
    });
    format!("http://{addr}")
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("").to_owned();
    let mut headers = Vec::new();
    let mut length = 0usize;
    let mut range = None;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h)?;
        let h = h.trim_end().to_owned();
        if h.is_empty() {
            break;
        }
        let lower = h.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = lower.strip_prefix("range: bytes=") {
            range = v.trim_end_matches('-').parse::<usize>().ok();
        }
        headers.push(lower);
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let path = target.split('?').next().unwrap_or("");
    let reply = |stream: &mut TcpStream, status: &str, extra: &str, data: &[u8]| {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
            data.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(data)
    };
    match (method.as_str(), path) {
        ("GET", "/hello.txt") => reply(&mut stream, "200 OK", "", b"hello\n"),
        ("GET", "/") => reply(&mut stream, "200 OK", "", b"<html>index</html>\n"),
        ("GET", "/dir/data.bin") => reply(&mut stream, "200 OK", "", b"\x00\x01\x02"),
        ("GET", "/redirect") => reply(&mut stream, "302 Found", "Location: /hello.txt\r\n", b""),
        ("GET", "/headers") => {
            let text = headers.join("\n") + "\n";
            reply(&mut stream, "200 OK", "", text.as_bytes())
        }
        ("POST", "/post") => {
            let ct = headers
                .iter()
                .find(|h| h.starts_with("content-type:"))
                .cloned()
                .unwrap_or_default();
            let mut text = format!("{ct}\n").into_bytes();
            text.extend_from_slice(&body);
            reply(&mut stream, "200 OK", "", &text)
        }
        ("GET", "/range.txt") => {
            let full = b"0123456789";
            match range {
                Some(n) if n >= full.len() => reply(
                    &mut stream,
                    "416 Range Not Satisfiable",
                    &format!("Content-Range: bytes */{}\r\n", full.len()),
                    b"",
                ),
                Some(n) => reply(
                    &mut stream,
                    "206 Partial Content",
                    &format!("Content-Range: bytes {n}-9/10\r\n"),
                    &full[n..],
                ),
                None => reply(&mut stream, "200 OK", "", full),
            }
        }
        ("GET", "/slow") => {
            std::thread::sleep(std::time::Duration::from_secs(6));
            reply(&mut stream, "200 OK", "", b"late\n")
        }
        _ => reply(&mut stream, "404 Not Found", "", b"no\n"),
    }
}

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("skua-tools-wget-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("mkdir");
        Self(p)
    }
    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap_or_default()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wget(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wget"))
        .args(args)
        .current_dir(dir)
        .env("no_proxy", "*")
        .env("NO_PROXY", "*")
        .stdin(Stdio::null())
        .output()
        .expect("run wget")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn saves_to_the_url_file_name_and_never_overwrites() {
    if !have_curl() {
        return;
    }
    let base = server();
    let d = Dir::new("names");
    let out = wget(&d.0, &[&format!("{base}/hello.txt")]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("Saving to: 'hello.txt'"));
    assert_eq!(d.read("hello.txt"), b"hello\n");
    // A second run keeps the first file and saves hello.txt.1, as GNU Wget.
    assert_eq!(
        wget(&d.0, &["-q", &format!("{base}/hello.txt")])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(d.read("hello.txt.1"), b"hello\n");
    // No path: index.html; a query string is not part of the name.
    assert_eq!(
        wget(&d.0, &["-q", &format!("{base}/")]).status.code(),
        Some(0)
    );
    assert_eq!(d.read("index.html"), b"<html>index</html>\n");
    assert_eq!(
        wget(
            &d.0,
            &["-q", "-P", "sub", &format!("{base}/dir/data.bin?x=1")]
        )
        .status
        .code(),
        Some(0)
    );
    assert_eq!(d.read(r"sub\data.bin"), b"\x00\x01\x02");
}

#[test]
fn output_document_and_stdout() {
    if !have_curl() {
        return;
    }
    let base = server();
    let d = Dir::new("out");
    let out = wget(&d.0, &["-qO-", &format!("{base}/hello.txt")]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "hello\n");
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    let out = wget(
        &d.0,
        &[
            "-q",
            "-O",
            "both.txt",
            &format!("{base}/hello.txt"),
            &format!("{base}/redirect"),
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(d.read("both.txt"), b"hello\nhello\n");
}

#[test]
fn headers_user_agent_and_post() {
    if !have_curl() {
        return;
    }
    let base = server();
    let d = Dir::new("hdr");
    let out = wget(
        &d.0,
        &[
            "-qO-",
            "--header=X-Test: one",
            "-U",
            "agent/7",
            &format!("{base}/headers"),
        ],
    );
    let body = text(&out.stdout);
    assert!(
        body.contains("x-test: one") && body.contains("user-agent: agent/7"),
        "{body}"
    );
    let out = wget(&d.0, &["-qO-", &format!("{base}/headers")]);
    assert!(
        text(&out.stdout).contains("user-agent: wget/"),
        "{}",
        text(&out.stdout)
    );
    let out = wget(
        &d.0,
        &["-qO-", "--post-data=a=1&b=2", &format!("{base}/post")],
    );
    assert_eq!(
        text(&out.stdout),
        "content-type: application/x-www-form-urlencoded\na=1&b=2"
    );
    std::fs::write(d.0.join("body.json"), b"{\"k\":1}").expect("write");
    let out = wget(
        &d.0,
        &[
            "-qO-",
            "--post-file=body.json",
            "--header=Content-Type: application/json",
            &format!("{base}/post"),
        ],
    );
    assert_eq!(
        text(&out.stdout),
        "content-type: application/json\n{\"k\":1}"
    );
}

#[test]
fn continue_appends_the_rest() {
    if !have_curl() {
        return;
    }
    let base = server();
    let d = Dir::new("cont");
    std::fs::write(d.0.join("range.txt"), b"0123").expect("write");
    let out = wget(&d.0, &["-q", "-c", &format!("{base}/range.txt")]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(d.read("range.txt"), b"0123456789");
    let out = wget(&d.0, &["-c", &format!("{base}/range.txt")]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("already fully retrieved"));
    assert_eq!(d.read("range.txt"), b"0123456789");
}

#[test]
fn failures_have_wget_statuses() {
    if !have_curl() {
        return;
    }
    let base = server();
    let d = Dir::new("fail");
    // HTTP error: 8, no file left behind.
    let out = wget(&d.0, &[&format!("{base}/missing.txt")]);
    assert_eq!(out.status.code(), Some(8));
    assert!(
        text(&out.stderr).contains("ERROR 404"),
        "{}",
        text(&out.stderr)
    );
    assert!(!d.0.join("missing.txt").exists());
    // Connection refused: 4.
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();
    let out = wget(
        &d.0,
        &["-q", "-t", "1", &format!("http://127.0.0.1:{port}/x")],
    );
    assert_eq!(out.status.code(), Some(4));
    assert!(out.stderr.is_empty());
    // Timeout: 4.
    let out = wget(&d.0, &["-q", "-T", "1", "-O", "-", &format!("{base}/slow")]);
    assert_eq!(out.status.code(), Some(4));
    // Several URLs: the network failure (4) wins over the HTTP error (8).
    let out = wget(
        &d.0,
        &[
            "-q",
            "-O",
            "-",
            &format!("{base}/missing"),
            &format!("http://127.0.0.1:{port}/x"),
        ],
    );
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn usage_help_and_version() {
    let d = Dir::new("usage");
    let out = wget(&d.0, &["--mirror", "http://127.0.0.1:1/"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("not supported by the skua-tools wget"));
    let out = wget(&d.0, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("wget: missing URL"));
    let out = wget(&d.0, &["--version"]);
    assert_eq!(out.status.code(), Some(0));
    let v = text(&out.stdout);
    assert!(
        v.starts_with("skua-tools wget") && v.contains("not GNU Wget") && v.contains("curl"),
        "{v}"
    );
    let out = wget(&d.0, &["--help"]);
    assert!(text(&out.stdout).contains("--post-data=STRING"));
}
