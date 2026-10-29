use super::run;

fn iconv(args: &[&str], input: &[u8]) -> (i32, Vec<u8>, String) {
    let mut v = vec!["iconv".to_owned()];
    v.extend(args.iter().map(|s| (*s).to_owned()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let env = |k: &str| (k == "LANG").then(|| "en_US.UTF-8".to_owned());
    let rc = run(&v, &mut &input[..], &mut out, &mut err, &env);
    (rc, out, String::from_utf8_lossy(&err).into_owned())
}

#[test]
fn utf16_forms() {
    let (rc, out, _) = iconv(&["-f", "UTF-8", "-t", "UTF-16"], "h\u{e9}".as_bytes());
    assert_eq!((rc, out), (0, vec![0xfe, 0xff, 0, b'h', 0, 0xe9]));
    let (_, out, _) = iconv(
        &["-f", "UTF-8", "-t", "UTF-16LE"],
        "\u{20ac}\u{1f600}".as_bytes(),
    );
    assert_eq!(out, vec![0xac, 0x20, 0x3d, 0xd8, 0x00, 0xde]);
    let (_, out, _) = iconv(&["-f", "UTF-16LE", "-t", "UTF-8"], &[b'h', 0, 0xe9, 0]);
    assert_eq!(out, "h\u{e9}".as_bytes());
}

#[test]
fn unconvertible_reports_position() {
    let (rc, out, err) = iconv(&["-f", "UTF-8", "-t", "ASCII"], "ab\n\u{e9}".as_bytes());
    assert_eq!(rc, 1);
    assert_eq!(out, b"ab\n");
    assert_eq!(err, "iconv: (stdin):2:0: cannot convert\n");
    let (rc, out, _) = iconv(&["-c", "-f", "UTF-8", "-t", "ASCII"], "a\u{e9}b".as_bytes());
    assert_eq!((rc, out.as_slice()), (1, &b"ab"[..]));
    let (rc, out, _) = iconv(
        &["-f", "UTF-8", "-t", "ASCII//IGNORE"],
        "a\u{e9}b".as_bytes(),
    );
    assert_eq!((rc, out.as_slice()), (0, &b"ab"[..]));
}

#[test]
fn translit_and_subst() {
    let (_, out, _) = iconv(
        &["-f", "UTF-8", "-t", "ASCII//TRANSLIT"],
        "h\u{e9}llo \u{20ac}".as_bytes(),
    );
    assert_eq!(out, b"h'ello EUR");
    let (rc, out, _) = iconv(
        &["--unicode-subst=<U+%04X>", "-f", "UTF-8", "-t", "ASCII"],
        "\u{4e2d}".as_bytes(),
    );
    assert_eq!((rc, out.as_slice()), (0, &b"<U+4E2D>"[..]));
}

#[test]
fn code_pages_and_errors() {
    let (_, out, _) = iconv(&["-f", "CP1252", "-t", "UTF-8"], &[0x80, 0xe9]);
    assert_eq!(out, "\u{20ac}\u{e9}".as_bytes());
    let (rc, _, err) = iconv(&["-f", "CP1252", "-t", "UTF-8"], &[0x81]);
    assert_eq!(
        (rc, err.as_str()),
        (1, "iconv: (stdin):1:0: cannot convert\n")
    );
    let (rc, _, err) = iconv(&["-f", "BOGUS", "-t", "UTF-8"], b"");
    assert_eq!(rc, 1);
    assert!(err.starts_with("iconv: conversion from BOGUS unsupported\n"));
    let (_, _, err) = iconv(&["-f", "UTF-8", "-t", "UTF-16LE"], b"ab\xe2\x82");
    assert_eq!(
        err,
        "\niconv: (stdin):1:2: incomplete character or shift sequence\n"
    );
}
