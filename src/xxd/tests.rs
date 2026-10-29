use super::opts::{Parsed, parse};
use super::{dump, revert};

fn opts(args: &[&str]) -> super::opts::Opts {
    let mut v = vec!["xxd".to_owned()];
    v.extend(args.iter().map(|s| (*s).to_owned()));
    match parse(&v) {
        Ok(Parsed::Run(o)) => o,
        _ => panic!("parse"),
    }
}

fn dump_str(args: &[&str], data: &[u8]) -> String {
    let o = opts(args);
    let mut out = Vec::new();
    dump::dump(&o, None, &mut &data[..], &mut out, 0, false).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn default_dump() {
    let s = dump_str(&[], b"Hello, World!\nsecond line here ok\n\x00\xff\x7f~");
    assert_eq!(
        s,
        "00000000: 4865 6c6c 6f2c 2057 6f72 6c64 210a 7365  Hello, World!.se\n\
         00000010: 636f 6e64 206c 696e 6520 6865 7265 206f  cond line here o\n\
         00000020: 6b0a 00ff 7f7e                           k....~\n"
    );
}

#[test]
fn groups_and_little_endian() {
    assert_eq!(
        dump_str(&["-g4", "-l", "20"], b"Hello, World!\nsecond"),
        "00000000: 48656c6c 6f2c2057 6f726c64 210a7365  Hello, World!.se\n\
         00000010: 636f6e64                             cond\n"
    );
    assert_eq!(
        dump_str(&["-e", "-c5", "-g4"], b"Hello"),
        "00000000: 6c6c6548       6f  Hello\n"
    );
}

#[test]
fn include_and_plain() {
    let o = opts(&["-i", "-n", "x"]);
    let mut out = Vec::new();
    dump::dump(&o, Some("x"), &mut &b"abc"[..], &mut out, 0, false).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "unsigned char x[] = {\n  0x61, 0x62, 0x63\n};\nunsigned int x_len = 3;\n"
    );
    assert_eq!(dump_str(&["-p", "-c", "2"], b"abc"), "6162\n63\n");
}

#[test]
fn autoskip() {
    let z = vec![0u8; 64];
    assert_eq!(
        dump_str(&["-a"], &z),
        "00000000: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n*\n\
         00000030: 0000 0000 0000 0000 0000 0000 0000 0000  ................\n"
    );
}

#[test]
fn revert_patch_and_text() {
    let o = opts(&["-r"]);
    let mut out = Vec::new();
    {
        let mut sink = revert::PipeSink::new(&mut out);
        revert::revert(&o, &mut &b"0: 41 42\n8: 43\n"[..], &mut sink)
            .ok()
            .unwrap();
    }
    assert_eq!(out, b"AB\0\0\0\0\0\0C");
}
