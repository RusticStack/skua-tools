//! Command-line quoting for MSYS/Cygwin programs started from a native Windows program.
//!
//! Rust's `Command::args` quotes by the Microsoft C runtime rules. Cygwin's own command-line
//! parser differs: outside double quotes it also treats `'` as a quote, expands `*`/`?` (globbing)
//! and `~`, and does not take `\"` as an escaped quote. Wrapping every argument that is not plainly
//! safe in `"..."` (with `\"` and doubled backslashes before a quote) gives it back unchanged.

use std::ffi::{OsStr, OsString};

fn plain(arg: &str) -> bool {
    !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./:=,+@%\\".contains(c))
}

/// One argument as the Cygwin parser expects it.
pub fn quote(arg: &OsStr) -> OsString {
    let Some(s) = arg.to_str() else {
        // Not valid Unicode: leave it to Rust's own quoting by wrapping minimally.
        let mut o = OsString::from("\"");
        o.push(arg);
        o.push("\"");
        return o;
    };
    if plain(s) {
        return OsString::from(s);
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in s.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    OsString::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(s: &str) -> String {
        quote(OsStr::new(s)).to_string_lossy().into_owned()
    }

    #[test]
    fn plain_arguments_are_left_alone() {
        assert_eq!(q("-pe"), "-pe");
        assert_eq!(q(r"C:\a\b.txt"), r"C:\a\b.txt");
    }

    #[test]
    fn quotes_and_special_characters_are_wrapped() {
        assert_eq!(q(r#"s/"//g"#), r#""s/\"//g""#);
        assert_eq!(q("it's"), "\"it's\"");
        assert_eq!(q("~"), "\"~\"");
        assert_eq!(q("a b"), "\"a b\"");
        assert_eq!(q(""), "\"\"");
        assert_eq!(q("*.c"), "\"*.c\"");
    }

    #[test]
    fn backslashes_before_a_quote_or_the_end_are_doubled() {
        assert_eq!(q(r"a b\"), r#""a b\\""#);
        assert_eq!(q(r#"a\"b"#), r#""a\\\"b""#);
    }
}
