//! End to end: `ps`, `pgrep` and `pkill` against a `ping` this test starts (and only that
//! process: the pattern carries a payload size derived from the test's own PID).

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Victim {
    child: Child,
    size: u32,
}

impl Victim {
    fn start(tag: u32) -> Self {
        Self::start_as(tag, "ping")
    }

    fn start_as(tag: u32, program: &str) -> Self {
        let size = 100 + (std::process::id() % 30_000) * 2 + tag;
        let child = Command::new(program)
            .args(["-n", "120", "-l", &size.to_string(), "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start ping");
        Victim { child, size }
    }

    fn pattern(&self) -> String {
        format!("(?i)ping.* -l {} ", self.size)
    }
}

impl Drop for Victim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn tool(exe: &str, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run tool")
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn pgrep(args: &[&str]) -> Output {
    tool(env!("CARGO_BIN_EXE_pgrep"), args)
}

fn wait_found(v: &Victim) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if pgrep(&["-f", &v.pattern()]).status.code() == Some(0) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("pgrep never found the victim");
}

#[test]
fn pgrep_ps_pkill_round_trip() {
    let v = Victim::start(0);
    wait_found(&v);
    let id = v.child.id().to_string();
    let pat = v.pattern();

    let o = pgrep(&["-f", &pat]);
    assert_eq!(text(&o), format!("{id}\n"));
    assert_eq!(text(&pgrep(&["-fl", &pat])), format!("{id} ping\n"));
    let a = text(&pgrep(&["-fa", &pat]));
    assert!(
        a.starts_with(&format!("{id} ")) && a.contains(" -n 120 -l "),
        "{a:?}"
    );
    assert_eq!(text(&pgrep(&["-fc", &pat])), "1\n");
    assert_eq!(text(&pgrep(&["-f", "-d", ",", &pat])), format!("{id}\n"));
    assert_eq!(text(&pgrep(&["-fn", &pat])), format!("{id}\n"));
    assert_eq!(text(&pgrep(&["-fo", &pat])), format!("{id}\n"));
    // Name match: without -f the pattern is the image name, without .exe, any case.
    assert!(text(&pgrep(&["-x", "ping"])).lines().any(|l| l == id));
    assert!(text(&pgrep(&["-x", "PING"])).lines().all(|l| l != id));
    assert!(text(&pgrep(&["-ix", "PING"])).lines().any(|l| l == id));
    assert!(pgrep(&["-x", "pin"]).stdout.is_empty());
    assert_eq!(pgrep(&["-x", "pin"]).status.code(), Some(1));
    assert!(!text(&pgrep(&["-fv", &pat])).lines().any(|l| l == id));

    let ps = tool(env!("CARGO_BIN_EXE_ps"), &["-p", &id]);
    let ps = text(&ps);
    let mut lines = ps.lines();
    assert_eq!(
        lines.next(),
        Some("      PID    PPID    PGID     WINPID   TTY         UID    STIME COMMAND")
    );
    let row = lines.next().unwrap_or_default();
    assert!(row.trim_start().starts_with(&id), "{row:?}");
    assert!(row.to_lowercase().ends_with("ping.exe"), "{row:?}");
    assert_eq!(lines.next(), None);
    let psf = text(&tool(env!("CARGO_BIN_EXE_ps"), &["-fp", &id]));
    assert!(
        psf.lines()
            .nth(1)
            .unwrap_or_default()
            .contains("-n 120 -l "),
        "{psf:?}"
    );

    // pkill: -e echoes, the victim dies, a second run finds nothing.
    let k = tool(env!("CARGO_BIN_EXE_pkill"), &["-e", "-f", &pat]);
    assert_eq!(
        k.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&k.stderr)
    );
    assert_eq!(text(&k), format!("ping killed (pid {id})\n"));
    let mut v = v;
    let end = Instant::now() + Duration::from_secs(10);
    while v.child.try_wait().ok().flatten().is_none() {
        assert!(Instant::now() < end, "victim still running");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(pgrep(&["-f", &pat]).status.code(), Some(1));
    let again = tool(env!("CARGO_BIN_EXE_pkill"), &["-f", &pat]);
    assert_eq!(again.status.code(), Some(1));
    assert!(again.stdout.is_empty() && again.stderr.is_empty());
    let gone = tool(env!("CARGO_BIN_EXE_ps"), &["-p", &id]);
    assert_eq!(gone.status.code(), Some(1));
}

#[test]
fn signal_zero_checks_without_killing() {
    let v = Victim::start(1);
    wait_found(&v);
    let pat = v.pattern();
    let o = tool(env!("CARGO_BIN_EXE_pkill"), &["-0", "-f", &pat]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(pgrep(&["-f", &pat]).status.code(), Some(0));
    let o = tool(
        env!("CARGO_BIN_EXE_pkill"),
        &["--signal", "CONT", "-f", &pat],
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(pgrep(&["-f", &pat]).status.code(), Some(0));
}

#[test]
fn usage_errors_exit_2_and_never_wait_for_stdin() {
    for exe in [env!("CARGO_BIN_EXE_pgrep"), env!("CARGO_BIN_EXE_pkill")] {
        let o = tool(exe, &[]);
        assert_eq!(o.status.code(), Some(2));
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert!(err.contains("no matching criteria specified"), "{err:?}");
        assert!(err.contains(" --help' for more information."), "{err:?}");
        assert_eq!(tool(exe, &["-Z", "x"]).status.code(), Some(2));
        assert_eq!(tool(exe, &["a", "b"]).status.code(), Some(2));
        assert_eq!(tool(exe, &["("]).status.code(), Some(2));
        assert_eq!(tool(exe, &["--help"]).status.code(), Some(0));
    }
    // The pattern matches no process (it would match itself only through its own command line).
    let o = pgrep(&["zzz-no-such-process-qq"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    let me = tool(
        env!("CARGO_BIN_EXE_pgrep"),
        &["-f", "zzz-no-such-process-qq"],
    );
    assert_eq!(me.status.code(), Some(1), "pgrep must not match itself");
}

#[test]
fn ps_usage_and_errors() {
    let ps = env!("CARGO_BIN_EXE_ps");
    let o = tool(ps, &["--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(text(&o).starts_with("Usage: ps [-aefls] [-u UID] [-p PID]\n"));
    let o = tool(ps, &["-x"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&o.stderr),
        "ps: unknown option -- x\nTry `ps --help' for more information.\n"
    );
    for args in [
        &["aux"][..],
        &["-ef"],
        &["-e"],
        &["-W"],
        &["-a"],
        &["-l"],
        &["-s"],
    ] {
        let o = tool(ps, args);
        assert_eq!(o.status.code(), Some(0), "{args:?}");
        assert!(text(&o).lines().count() > 5);
    }
}

#[test]
fn full_match_sees_the_name_as_typed_and_the_windows_path() {
    let v = Victim::start_as(2, r"C:\Windows\System32\PING.EXE");
    wait_found(&v);
    let id = v.child.id().to_string();
    let typed = format!("^PING -n 120 -l {} ", v.size);
    assert_eq!(text(&pgrep(&["-f", &typed])), format!("{id}\n"));
    let windows = format!("System32.PING\\.EXE -n 120 -l {} ", v.size);
    assert_eq!(text(&pgrep(&["-f", &windows])), format!("{id}\n"));
}

#[test]
fn newest_and_oldest_use_the_creation_time() {
    // Two pings started within the same second: -n and -o still tell them apart.
    let first = Victim::start(10);
    std::thread::sleep(Duration::from_millis(300));
    let second = Victim::start_as(10, "ping");
    wait_found(&first);
    let (a, b) = (first.child.id().to_string(), second.child.id().to_string());
    let pat = first.pattern();
    let mut both: Vec<String> = text(&pgrep(&["-f", &pat]))
        .lines()
        .map(str::to_owned)
        .collect();
    both.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(both, want);
    assert_eq!(text(&pgrep(&["-fo", &pat])), format!("{a}\n"));
    assert_eq!(text(&pgrep(&["-fn", &pat])), format!("{b}\n"));
}
