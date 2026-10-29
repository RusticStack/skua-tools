//! A reader that goes away (`unzip -l x.zip | head -1`) must end `zip`/`unzip` quietly with status
//! 141, not with a panic message and abort, and `zip` must not leave its temporary file behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn sandbox(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("skua-tools-pipe-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("many")).expect("mkdir");
    for i in 0..3000 {
        fs::write(
            d.join("many")
                .join(format!("file-with-a-long-name-{i:05}.txt")),
            "x\n",
        )
        .expect("write");
    }
    d
}

/// Runs `exe args` in `dir` with stdout closed right after the spawn; returns (status, stderr).
fn run_closed(exe: &str, dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    drop(child.stdout.take());
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn unzip_list_into_a_closed_pipe_exits_141_quietly() {
    let d = sandbox("unzip");
    let z = Command::new(env!("CARGO_BIN_EXE_zip"))
        .args(["-rq", "m.zip", "many"])
        .current_dir(&d)
        .status()
        .expect("zip");
    assert!(z.success());
    for args in [
        &["-l", "m.zip"][..],
        &["-p", "m.zip", "many/file-with-a-long-name-00001.txt"],
    ] {
        let (code, stderr) = run_closed(env!("CARGO_BIN_EXE_unzip"), &d, args);
        assert_eq!(code, Some(141), "{args:?}: {stderr}");
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
    }
}

#[test]
fn zip_verbose_into_a_closed_pipe_exits_141_and_leaves_no_temp_file() {
    let d = sandbox("zip");
    let (code, stderr) = run_closed(env!("CARGO_BIN_EXE_zip"), &d, &["-r", "m2.zip", "many"]);
    assert_eq!(code, Some(141), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    let leftovers: Vec<_> = fs::read_dir(&d)
        .expect("read_dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("zi"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}
