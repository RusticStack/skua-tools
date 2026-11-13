use super::args::{Options, Parsed, parse};
use super::sid::{Context, NO_ID, Sid, id_candidates, sid_to_id};

fn ctx() -> Context {
    Context {
        machine: Some(Sid::new(5, &[21, 1, 2, 3])),
        primary: None,
        logon: Some(Sid::new(5, &[5, 0, 12345])),
    }
}

fn sid(s: &str) -> Sid {
    let mut it = s.trim_start_matches("S-1-").split('-');
    let authority = it.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    Sid {
        authority,
        subs: it.map(|x| x.parse().unwrap_or(0)).collect(),
    }
}

#[test]
fn cygwin_mapping() {
    let c = ctx();
    // Values as Git Bash's id.exe prints them for these SIDs.
    let cases = [
        ("S-1-5-21-1-2-3-1001", 197609),
        ("S-1-5-21-1-2-3-513", 197121),
        ("S-1-5-21-1-2-3-500", 197108),
        ("S-1-5-18", 18),
        ("S-1-5-4", 4),
        ("S-1-5-11", 11),
        ("S-1-5-32-544", 544),
        ("S-1-5-32-545", 545),
        ("S-1-5-64-10", 262154),
        ("S-1-2-0", 66048),
        ("S-1-2-1", 66049),
        ("S-1-16-8192", 401408),
        ("S-1-5-5-0-12345", 4095),
        ("S-1-5-5-0-999", 4094),
        ("S-1-5-21-9-9-9-1001", NO_ID),
        ("S-1-5-80-1-2-3-4-5", NO_ID),
    ];
    for (s, id) in cases {
        assert_eq!(sid_to_id(&sid(s), &c), id, "{s}");
    }
    let mut joined = ctx();
    joined.primary = Some(Sid::new(5, &[21, 7, 8, 9]));
    assert_eq!(
        sid_to_id(&sid("S-1-5-21-7-8-9-1104"), &joined),
        0x100000 + 1104
    );
}

#[test]
fn reverse_mapping_round_trips() {
    let c = ctx();
    for id in [18, 4, 545, 262154, 66049, 401408, 197609, 4095] {
        let cands = id_candidates(id, &c);
        assert!(!cands.is_empty(), "{id}");
        assert!(cands.iter().all(|s| sid_to_id(s, &c) == id), "{id}");
    }
    // Everyone (S-1-1-0, 65792) has no passwd entry; -1 maps to nothing.
    assert!(!id_candidates(65792, &c).iter().any(Sid::is_everyone));
    assert!(id_candidates(NO_ID, &c).is_empty());
    assert_eq!(id_candidates(544, &c), vec![sid("S-1-5-32-544")]);
}

#[test]
fn sid_bytes_round_trip() {
    let s = sid("S-1-5-21-2214389790-119616845-436061127-1001");
    let b = s.to_bytes();
    assert_eq!(b.len(), 8 + 4 * 5);
    assert_eq!(Sid::from_bytes(&b), Some(s.clone()));
    assert_eq!(
        s.to_string(),
        "S-1-5-21-2214389790-119616845-436061127-1001"
    );
    assert_eq!(Sid::from_bytes(&b[..10]), None);
}

fn args(a: &[&str]) -> Parsed {
    parse(&a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
}

#[test]
fn options_as_gnu_id() {
    let run = |a: &[&str]| match args(a) {
        Parsed::Run(o) => o,
        p => panic!("{a:?}: {p:?}"),
    };
    assert_eq!(
        run(&["-un"]),
        Options {
            user: true,
            name: true,
            ..Options::default()
        }
    );
    assert!(run(&["--u", "x", "--real"]).real);
    assert_eq!(run(&["x", "-u", "--", "-g"]).users, ["x", "-g"]);
    assert!(run(&["-a", "--zero", "-G"]).zero);
    let fatal = |a: &[&str]| match args(a) {
        Parsed::Fatal(m) => m,
        p => panic!("{a:?}: {p:?}"),
    };
    assert_eq!(
        fatal(&["-ug"]),
        "cannot print \"only\" of more than one choice"
    );
    assert_eq!(
        fatal(&["-n"]),
        "cannot print only names or real IDs in default format"
    );
    assert_eq!(
        fatal(&["--z"]),
        "option --zero not permitted in default format"
    );
    assert_eq!(
        fatal(&["-Z", "-u"]),
        "--context (-Z) works only on an SELinux-enabled kernel"
    );
    let usage = |a: &[&str]| match args(a) {
        Parsed::Usage(m) => m,
        p => panic!("{a:?}: {p:?}"),
    };
    assert_eq!(usage(&["-x"]), "unknown option -- x");
    assert_eq!(usage(&["-h"]), "unknown option -- h");
    assert_eq!(usage(&["--bogus"]), "unknown option -- bogus");
    assert_eq!(usage(&["--g"]), "ambiguous option -- g");
    assert_eq!(
        usage(&["--help=x"]),
        "option doesn't take an argument -- help"
    );
    assert_eq!(args(&["--hel"]), Parsed::Help);
    assert_eq!(args(&["--vers", "-x"]), Parsed::Version);
    assert_eq!(
        args(&["--group"]),
        Parsed::Run(Options {
            group: true,
            ..Options::default()
        })
    );
}
