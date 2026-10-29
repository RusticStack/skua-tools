use super::run;

fn col(args: &[&str], input: &str) -> (i32, String, String) {
    let mut v = vec!["column".to_owned()];
    v.extend(args.iter().map(|s| (*s).to_owned()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let rc = run(&v, &mut input.as_bytes(), &mut out, &mut err, 80);
    (
        rc,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

#[test]
fn table_pads_all_but_last_column() {
    let (rc, out, _) = col(&["-t"], "name age city\nalice 30 paris\nbob 4 rome\n");
    assert_eq!(rc, 0);
    assert_eq!(
        out,
        "name   age  city\nalice  30   paris\nbob    4    rome\n"
    );
}

#[test]
fn separator_keeps_empty_fields() {
    let (_, out, _) = col(&["-t", "-s:"], "a:b:c\nlonger:x:\n:y:z\nlast\n");
    assert_eq!(
        out,
        "a       b  c\nlonger  x  \n        y  z\nlast       \n"
    );
}

#[test]
fn fill_mode_columns_and_tabs() {
    let input: String = (1..=30).map(|i| format!("{i}\n")).collect();
    let (_, out, _) = col(&[], &input);
    assert_eq!(
        out.lines().next(),
        Some("1\t4\t7\t10\t13\t16\t19\t22\t25\t28")
    );
    let (_, out, _) = col(&["-x", "-c", "40"], &input);
    assert_eq!(out.lines().next(), Some("1\t2\t3\t4\t5"));
}

#[test]
fn names_json_and_errors() {
    let (_, out, _) = col(&["-t", "-N", "a,b", "-R", "a"], "xx y\n");
    assert_eq!(out, " a  b\nxx  y\n");
    let (rc, out, _) = col(&["-J", "-N", "x,y"], "1 2\n");
    assert_eq!(rc, 0);
    assert!(out.starts_with("{\n   \"table\": [\n      {\n         \"x\": \"1\",\n"));
    let (rc, _, err) = col(&["-t", "-H", "9"], "a\n");
    assert_eq!(
        (rc, err.as_str()),
        (1, "column: undefined column name '9'\n")
    );
    let (rc, _, err) = col(&["--bogus"], "");
    assert_eq!(rc, 1);
    assert_eq!(
        err,
        "column: unknown option -- bogus\nTry 'column --help' for more information.\n"
    );
}
