//! End-to-end checks of `start` (dry run only: nothing is ever opened).

use std::process::Command;

fn dry(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_start"))
        .args(args)
        .env("SKUA_START_DRY_RUN", "1")
        .output()
        .expect("run start");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn dry_run_prints_the_cmd_line() {
    assert_eq!(
        dry(&["https://a.b/?x=1&y=2"]),
        "cmd.exe /c start \"\" \"https://a.b/?x=1&y=2\"\r\n"
            .replace("\r\n", "\n")
            .as_str()
            .to_owned()
            + ""
    );
}

#[test]
fn no_arguments_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_start"))
        .output()
        .expect("run start");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("Usage: start"));
}
