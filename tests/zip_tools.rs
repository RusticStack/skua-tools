//! End-to-end tests of `gzip`, `unzip` and `zip` as processes: round trips, naming, messages,
//! exit codes, and that nothing waits on a prompt when stdin is not a terminal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn run(exe: &str, dir: &Path, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn")
}

fn gzip(dir: &Path, args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_gzip"), dir, args)
}
fn zip(dir: &Path, args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_zip"), dir, args)
}
fn unzip(dir: &Path, args: &[&str]) -> Output {
    run(env!("CARGO_BIN_EXE_unzip"), dir, args)
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn sandbox(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("skua-tools-zip-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("mkdir");
    d
}

#[test]
fn gzip_round_trip_and_messages() {
    let d = sandbox("gz");
    let text = "alpha\nbeta\n".repeat(50);
    fs::write(d.join("a.txt"), &text).expect("write");
    let o = gzip(&d, &["a.txt"]);
    assert!(o.status.success());
    assert!(d.join("a.txt.gz").exists() && !d.join("a.txt").exists());
    // -l: header and one row, 19-wide numbers, the name without the suffix.
    let l = out(&gzip(&d, &["-l", "a.txt.gz"]));
    let lines: Vec<&str> = l.lines().collect();
    assert_eq!(
        lines[0],
        "         compressed        uncompressed  ratio uncompressed_name"
    );
    assert!(lines[1].ends_with(" a.txt") && lines[1].contains(&format!("{:>19}", text.len())));
    // -d refuses to overwrite, exit 2, GNU wording.
    fs::write(d.join("a.txt"), "other").expect("write");
    let o = gzip(&d, &["-d", "a.txt.gz"]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(err(&o), "gzip: a.txt already exists;\tnot overwritten\n");
    let o = gzip(&d, &["-dkf", "a.txt.gz"]);
    assert!(o.status.success());
    assert_eq!(fs::read_to_string(d.join("a.txt")).expect("read"), text);
    // Names that are not gzip files.
    let o = gzip(&d, &["-t", "a.txt"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(err(&o), "\ngzip: a.txt: not in gzip format\n");
    let o = gzip(&d, &["-d", "a.txt"]);
    assert_eq!(err(&o), "gzip: a.txt: unknown suffix -- ignored\n");
    assert_eq!(o.status.code(), Some(2));
    let o = gzip(&d, &["-x"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        err(&o),
        "gzip: unknown option -- x\nTry `gzip --help' for more information.\n"
    );
    let o = gzip(&d, &["nonexistent"]);
    assert_eq!(err(&o), "gzip: nonexistent: No such file or directory\n");
}

#[test]
fn gzip_stdin_and_multicall() {
    let d = sandbox("gz2");
    let exe = env!("CARGO_BIN_EXE_gzip");
    let mut c = Command::new(exe)
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    std::io::Write::write_all(&mut c.stdin.take().expect("stdin"), b"hello\n").expect("write");
    let packed = c.wait_with_output().expect("wait").stdout;
    assert_eq!(&packed[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3]);
    fs::write(d.join("h.gz"), &packed).expect("write");
    for form in [["gunzip", "-c"], ["zcat", "-q"]] {
        let o = gzip_multicall(&d, form[0], &[form[1], "h.gz"]);
        assert_eq!(out(&o), "hello\n", "{form:?}");
    }
    // Truncated input prints what it could and the gzip error.
    fs::write(d.join("t.gz"), &packed[..packed.len() - 6]).expect("write");
    let o = gzip(&d, &["-dc", "t.gz"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(err(&o), "\ngzip: t.gz: unexpected end of file\n");
}

fn gzip_multicall(dir: &Path, name: &str, args: &[&str]) -> Output {
    let mut a = vec![name];
    a.extend_from_slice(args);
    gzip(dir, &a)
}

#[test]
fn zip_then_unzip() {
    let d = sandbox("zip");
    fs::create_dir_all(d.join("proj/src")).expect("mkdir");
    fs::write(d.join("proj/a.txt"), "alpha\nbeta\n".repeat(40)).expect("write");
    fs::write(d.join("proj/src/m.rs"), "fn main() {}\n").expect("write");
    let o = zip(&d, &["-r", "out.zip", "proj"]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "  adding: proj/ (stored 0%)");
    assert!(
        lines[1].starts_with("  adding: proj/a.txt (deflated "),
        "{text}"
    );
    assert_eq!(lines[2], "  adding: proj/src/ (stored 0%)");
    assert_eq!(lines[3], "  adding: proj/src/m.rs (stored 0%)");
    // Extracting into another directory reproduces the tree.
    fs::create_dir_all(d.join("x")).expect("mkdir");
    let o = unzip(&d, &["-d", "x", "out.zip"]);
    assert!(o.status.success(), "{}", err(&o));
    let t = out(&o);
    assert!(
        t.starts_with("Archive:  out.zip\n   creating: x/proj/\n  inflating: x/proj/a.txt"),
        "{t}"
    );
    assert!(t.contains(" extracting: x/proj/src/m.rs"), "{t}");
    assert_eq!(
        fs::read(d.join("x/proj/a.txt")).expect("read"),
        fs::read(d.join("proj/a.txt")).expect("read")
    );
    // -l table.
    let l = out(&unzip(&d, &["-l", "out.zip", "proj/src/*"]));
    assert!(l.contains("  Length      Date    Time    Name\n"), "{l}");
    assert!(
        l.ends_with(
            "---------                     -------\n       13                     2 files\n"
        ),
        "{l}"
    );
    // Existing files and no terminal: the prompt goes to stderr, then end of file means "none".
    let o = unzip(&d, &["-d", "x", "out.zip"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        err(&o),
        "replace x/proj/a.txt? [y]es, [n]o, [A]ll, [N]one, [r]ename:  NULL\n(EOF or read error, treating as \"[N]one\" ...)\n"
    );
    let o = unzip(&d, &["-oqd", "x", "out.zip"]);
    assert!(o.status.success() && o.stdout.is_empty());
    // Update, delete, junk paths.
    fs::write(d.join("new.txt"), "n\n").expect("write");
    let o = zip(&d, &["-j", "out.zip", "new.txt", "proj/src/m.rs"]);
    assert_eq!(
        out(&o),
        "  adding: new.txt (stored 0%)\nupdating: m.rs (stored 0%)\n"
            .replace("updating: m.rs", "  adding: m.rs")
    );
    let o = zip(&d, &["-d", "out.zip", "new.txt"]);
    assert_eq!(out(&o), "deleting: new.txt\n");
    let o = zip(&d, &["-u", "out.zip", "proj/a.txt"]);
    assert_eq!(o.status.code(), Some(12));
    assert_eq!(err(&o), "\nzip error: Nothing to do! (out.zip)\n");
    let o = zip(&d, &["out2.zip", "missing.txt"]);
    assert_eq!(o.status.code(), Some(12));
    assert_eq!(
        err(&o),
        "\tzip warning: name not matched: missing.txt\n\nzip error: Nothing to do! (out2.zip)\n"
    );
}

#[test]
fn unzip_refuses_path_escapes_and_patterns() {
    let d = sandbox("slip");
    // An archive with `../evil.txt` and an absolute name, written by hand (stored entries).
    let mut z: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, body) in [
        ("../evil.txt", "evil\n"),
        ("/abs.txt", "abs\n"),
        ("ok.txt", "ok\n"),
    ] {
        let off = z.len() as u32;
        let crc = {
            let mut c = flate2::Crc::new();
            c.update(body.as_bytes());
            c.sum()
        };
        let hdr = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central {
                h.extend_from_slice(&[20, 3]);
            }
            h.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(body.len() as u32).to_le_bytes());
            h.extend_from_slice(&(body.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&[0, 0]);
            h
        };
        z.extend(hdr(0x0403_4b50, false));
        z.extend_from_slice(name.as_bytes());
        z.extend_from_slice(body.as_bytes());
        let mut c = hdr(0x0201_4b50, true);
        c.extend_from_slice(&[0; 10]);
        c.extend_from_slice(&off.to_le_bytes());
        c.extend_from_slice(name.as_bytes());
        central.extend(c);
    }
    let cd_off = z.len() as u32;
    z.extend_from_slice(&central);
    z.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    z.extend_from_slice(&[0, 0, 0, 0, 3, 0, 3, 0]);
    z.extend_from_slice(&(central.len() as u32).to_le_bytes());
    z.extend_from_slice(&cd_off.to_le_bytes());
    z.extend_from_slice(&[0, 0]);
    fs::write(d.join("slip.zip"), &z).expect("write");
    fs::create_dir_all(d.join("w")).expect("mkdir");
    let o = unzip(&d, &["-d", "w", "slip.zip"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        out(&o).contains("warning:  skipped \"../\" path component(s) in ../evil.txt\n"),
        "{}",
        out(&o)
    );
    assert_eq!(
        err(&o),
        "warning:  stripped absolute path spec from /abs.txt\n"
    );
    assert!(
        d.join("w/evil.txt").exists()
            && d.join("w/abs.txt").exists()
            && d.join("w/ok.txt").exists()
    );
    assert!(!d.join("evil.txt").exists());
    // `*` does not cross `/`, member names that match nothing exit 11.
    let o = unzip(&d, &["-l", "slip.zip", "nomatch"]);
    assert_eq!(o.status.code(), Some(11));
    let o = unzip(&d, &["-p", "slip.zip", "ok.txt"]);
    assert_eq!(out(&o), "ok\n");
    let o = unzip(&d, &["nonexistent.zip"]);
    assert_eq!(o.status.code(), Some(9));
    assert_eq!(
        err(&o),
        "unzip:  cannot find or open nonexistent.zip, nonexistent.zip.zip or nonexistent.zip.ZIP.\n"
    );
}
