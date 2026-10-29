//! `xxd -r` into a pipe pads gaps with NULs (a Windows anonymous pipe accepts a seek without
//! moving, so stdout must be checked for being a regular file first).
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn revert_to_a_pipe_pads_gaps_with_nul() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xxd"))
        .arg("-r")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn xxd");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"0: 41 42\n8: 43\n")
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.stdout, b"AB\0\0\0\0\0\0C");
}
