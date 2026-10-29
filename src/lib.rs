//! Small tools for Skua Bash that Git Bash has and uutils does not: `file`, `dos2unix`,
//! `unix2dos`. Shared code: text classification, a JSON syntax check, GNU error text.

/// GNU's text (as Git Bash's tools print it) for an I/O error: Windows' own message is in the
/// user's UI language.
pub fn gnu_error(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => "No such file or directory".to_owned(),
        ErrorKind::PermissionDenied => "Permission denied".to_owned(),
        ErrorKind::AlreadyExists => "File exists".to_owned(),
        ErrorKind::IsADirectory => "Is a directory".to_owned(),
        ErrorKind::NotADirectory => "Not a directory".to_owned(),
        _ => {
            let s = e.to_string();
            match s.find(" (os error ") {
                Some(i) => s[..i].to_owned(),
                None => s,
            }
        }
    }
}

/// Text classification as libmagic's `ascmagic`.
pub mod text {
    /// The character set of a text file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Encoding {
        /// 7-bit text.
        Ascii,
        /// Valid UTF-8 with non-ASCII characters.
        Utf8,
        /// UTF-8 starting with a byte order mark.
        Utf8Bom,
        /// 8-bit text with bytes 0xA0..=0xFF only.
        Iso8859,
        /// 8-bit text with bytes 0x80..=0x9F.
        ExtendedAscii,
    }

    /// What libmagic reports about a text file.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TextInfo {
        /// The character set.
        pub encoding: Encoding,
        /// Lines ending in `\r\n`.
        pub crlf: usize,
        /// Lines ending in a lone `\r`.
        pub cr: usize,
        /// Lines ending in a lone `\n`.
        pub lf: usize,
        /// The longest line, in bytes.
        pub longest_line: usize,
        /// The text has ESC characters.
        pub escapes: bool,
        /// The text has backspaces (overstriking).
        pub backspaces: bool,
    }

    /// Characters libmagic accepts in text: printable, `\a\b\t\n\v\f\r` and ESC.
    fn is_text_byte(b: u8) -> bool {
        b >= 0x20 && b != 0x7f || matches!(b, 7..=13 | 0x1b)
    }

    /// `None` when the bytes are not text (binary data).
    pub fn classify_text(data: &[u8]) -> Option<TextInfo> {
        let (body, bom) = match data.strip_prefix(b"\xef\xbb\xbf") {
            Some(rest) => (rest, true),
            None => (data, false),
        };
        let encoding = if body.iter().all(|&b| b < 0x80) {
            if bom {
                Encoding::Utf8Bom
            } else {
                Encoding::Ascii
            }
        } else if std::str::from_utf8(body).is_ok() {
            if bom {
                Encoding::Utf8Bom
            } else {
                Encoding::Utf8
            }
        } else if body.iter().all(|&b| !(0x80..0xa0).contains(&b)) {
            Encoding::Iso8859
        } else {
            Encoding::ExtendedAscii
        };
        if !body.iter().all(|&b| b >= 0x80 || is_text_byte(b)) {
            return None;
        }
        let mut info = TextInfo {
            encoding,
            crlf: 0,
            cr: 0,
            lf: 0,
            longest_line: 0,
            escapes: body.contains(&0x1b),
            backspaces: body.contains(&8),
        };
        let mut line = 0usize;
        let mut i = 0;
        while i < body.len() {
            match body[i] {
                b'\r' if body.get(i + 1) == Some(&b'\n') => {
                    info.crlf += 1;
                    i += 1;
                    info.longest_line = info.longest_line.max(line);
                    line = 0;
                }
                b'\r' => {
                    info.cr += 1;
                    info.longest_line = info.longest_line.max(line);
                    line = 0;
                }
                b'\n' => {
                    info.lf += 1;
                    info.longest_line = info.longest_line.max(line);
                    line = 0;
                }
                _ => line += 1,
            }
            i += 1;
        }
        info.longest_line = info.longest_line.max(line);
        Some(info)
    }
}

/// A JSON syntax check (RFC 8259 values), for `file`'s "JSON text data".
pub mod json {
    struct P<'a> {
        s: &'a [u8],
        i: usize,
        depth: usize,
    }

    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            }
        }
        fn eat(&mut self, lit: &[u8]) -> bool {
            if self.s[self.i..].starts_with(lit) {
                self.i += lit.len();
                true
            } else {
                false
            }
        }
        fn value(&mut self) -> bool {
            self.depth += 1;
            if self.depth > 256 {
                return false;
            }
            self.ws();
            let ok = match self.s.get(self.i) {
                Some(b'{') => self.object(),
                Some(b'[') => self.array(),
                Some(b'"') => self.string(),
                Some(b't') => self.eat(b"true"),
                Some(b'f') => self.eat(b"false"),
                Some(b'n') => self.eat(b"null"),
                Some(b'-' | b'0'..=b'9') => self.number(),
                _ => false,
            };
            self.depth -= 1;
            ok
        }
        fn object(&mut self) -> bool {
            self.i += 1;
            self.ws();
            if self.eat(b"}") {
                return true;
            }
            loop {
                self.ws();
                if !self.string() {
                    return false;
                }
                self.ws();
                if !self.eat(b":") || !self.value() {
                    return false;
                }
                self.ws();
                if self.eat(b"}") {
                    return true;
                }
                if !self.eat(b",") {
                    return false;
                }
            }
        }
        fn array(&mut self) -> bool {
            self.i += 1;
            self.ws();
            if self.eat(b"]") {
                return true;
            }
            loop {
                if !self.value() {
                    return false;
                }
                self.ws();
                if self.eat(b"]") {
                    return true;
                }
                if !self.eat(b",") {
                    return false;
                }
            }
        }
        fn string(&mut self) -> bool {
            if !self.eat(b"\"") {
                return false;
            }
            while let Some(&c) = self.s.get(self.i) {
                self.i += 1;
                match c {
                    b'"' => return true,
                    b'\\' => self.i += 1,
                    0..=0x1f => return false,
                    _ => {}
                }
            }
            false
        }
        fn number(&mut self) -> bool {
            let start = self.i;
            while let Some(&c) = self.s.get(self.i) {
                if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E') {
                    self.i += 1;
                } else {
                    break;
                }
            }
            self.i > start
        }
    }

    /// The whole input is one JSON object or array (with surrounding whitespace).
    pub fn is_json(s: &[u8]) -> bool {
        let mut p = P { s, i: 0, depth: 0 };
        p.ws();
        if !matches!(s.get(p.i), Some(b'{' | b'[')) || !p.value() {
            return false;
        }
        p.ws();
        p.i == s.len()
    }
}

/// For tools that print with `println!`: when the reader of stdout has gone away (`unzip -l x |
/// head -1`) end quietly with status 141, as a Git Bash tool killed by SIGPIPE does, instead of
/// the panic message and abort that `println!` would produce. Any other write failure ends with
/// `write error` and status 1. `cleanup` runs first (remove a temporary file).
pub fn quiet_exit_on_stdout_failure(cleanup: impl Fn() + Send + Sync + 'static) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = info.to_string();
        if !text.contains("failed printing to stdout") {
            previous(info);
            return;
        }
        cleanup();
        // ERROR_NO_DATA (232) and ERROR_BROKEN_PIPE (109): the pipe was closed by the reader.
        if text.contains("os error 232") || text.contains("os error 109") {
            std::process::exit(141);
        }
        eprintln!("write error");
        std::process::exit(1);
    }));
}

#[cfg(test)]
mod tests {
    use super::json::is_json;

    #[test]
    fn json() {
        assert!(is_json(b"{\"a\": [1, 2.5e3, true, null, \"x\\\"y\"]}\n"));
        assert!(is_json(b"[]"));
        assert!(!is_json(b"{\"a\": }"));
        assert!(!is_json(b"{} x"));
        assert!(!is_json(b"\"s\""));
    }
}
