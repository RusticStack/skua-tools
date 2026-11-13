//! `id` as Git Bash's (GNU coreutils 8.32 on Cygwin) with Git for Windows' default
//! `/etc/nsswitch.conf` (`passwd: files db`, `group: files # db`, no `/etc/passwd` or
//! `/etc/group`):
//!
//! - users come from Windows accounts, with Cygwin's SID <-> uid/gid mapping (`sid.rs`);
//! - groups have no database, so a group never has a name (`cannot find name for group ID N`,
//!   status 1) and the current user's group list is the primary group only; a named user has
//!   no supplementary groups.
//!
//! Account names: users and aliases print Windows' spelling, other kinds (well-known groups,
//! `None`, labels) the spelling typed, as Cygwin's account cache does. Accounts of this machine
//! are `MACHINE+name` when the machine is joined to a domain; other domains' accounts are
//! `DOMAIN+name`. Not done: trusted domains' POSIX offsets (their accounts are -1).

mod args;
pub mod sid;
#[cfg(test)]
mod tests;
mod win;

use std::io::{self, Write};
use std::process::ExitCode;

use args::{Options, Parsed};
use sid::{Context, NO_ID, Sid, id_candidates, sid_to_id};
use win::{Account, TokenInfo};

/// A passwd entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passwd {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
}

/// The machine, its domain and the session, for lookups.
struct Accounts {
    ctx: Context,
    computer: Option<String>,
    joined: Option<String>,
}

impl Accounts {
    fn new(token: Option<&TokenInfo>) -> Self {
        let (ctx, computer, joined) = win::context(token);
        Accounts {
            ctx,
            computer,
            joined,
        }
    }

    fn is_machine(&self, sid: &Sid) -> bool {
        sid.domain().is_some() && sid.domain() == self.ctx.machine
    }

    /// Cygwin's name for an account looked up by SID.
    fn display_name(&self, a: &Account) -> String {
        if self.is_machine(&a.sid) {
            match (&self.joined, &self.computer) {
                (Some(_), Some(c)) => format!("{c}+{}", a.name),
                _ => a.name.clone(),
            }
        } else if a.sid.is_account()
            && a.sid.domain().is_some()
            && a.sid.domain() != self.ctx.primary
        {
            format!("{}+{}", a.domain, a.name)
        } else {
            a.name.clone()
        }
    }

    /// The passwd entry for `sid`; `typed` is the name the user was looked up by.
    fn by_sid(&self, sid: &Sid, typed: Option<&str>) -> Option<Passwd> {
        if sid.is_everyone() {
            return None;
        }
        if sid.is_logon() && self.ctx.logon.as_ref() == Some(sid) {
            return Some(Passwd {
                name: "CurrentSession".to_owned(),
                uid: sid::CURRENT_SESSION,
                gid: sid::CURRENT_SESSION,
            });
        }
        let a = win::lookup_sid(sid)?;
        if !matches!(
            a.kind,
            win::SID_TYPE_USER
                | win::SID_TYPE_GROUP
                | win::SID_TYPE_ALIAS
                | win::SID_TYPE_WELL_KNOWN_GROUP
                | win::SID_TYPE_LABEL
        ) {
            return None;
        }
        let uid = sid_to_id(sid, &self.ctx);
        if uid == NO_ID {
            return None;
        }
        let name = match typed {
            Some(t) if !matches!(a.kind, win::SID_TYPE_USER | win::SID_TYPE_ALIAS) => t.to_owned(),
            _ => self.display_name(&a),
        };
        let gid = if a.kind == win::SID_TYPE_USER {
            let rid = if self.is_machine(sid) {
                win::local_primary_group_rid(&a.name).unwrap_or(513)
            } else {
                513
            };
            sid.domain()
                .map_or(NO_ID, |d| sid_to_id(&Sid::child(&d, rid), &self.ctx))
        } else {
            uid
        };
        Some(Passwd { name, uid, gid })
    }

    /// `getpwnam`.
    fn by_name(&self, spec: &str) -> Option<Passwd> {
        if spec == "CurrentSession" {
            let logon = self.ctx.logon.clone()?;
            return self.by_sid(&logon, None);
        }
        if spec.contains('\\') {
            return None;
        }
        if let Some((dom, user)) = spec.split_once('+') {
            self.joined.as_ref()?;
            let a = win::lookup_name(&format!("{dom}\\{user}"))?;
            if !a.domain.eq_ignore_ascii_case(dom) || !a.sid.is_account() {
                return None;
            }
            return self.by_sid(&a.sid, Some(spec));
        }
        let a = win::lookup_name(spec)?;
        // On a domain member this machine's accounts need their `MACHINE+` prefix.
        if self.joined.is_some() && self.is_machine(&a.sid) {
            return None;
        }
        self.by_sid(&a.sid, Some(spec))
    }

    /// `getpwuid`.
    fn by_uid(&self, uid: u32) -> Option<Passwd> {
        id_candidates(uid, &self.ctx)
            .iter()
            .find_map(|s| self.by_sid(s, None))
    }

    /// `parse_user_spec`: a name, else a number (`+` allowed).
    fn by_spec(&self, spec: &str) -> Option<Passwd> {
        if spec.is_empty() {
            return None;
        }
        self.by_name(spec).or_else(|| {
            let digits = spec.strip_prefix('+').unwrap_or(spec);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse::<u32>().ok().and_then(|u| self.by_uid(u))
        })
    }
}

/// What is printed for one user: ids, name and the group list.
struct Subject {
    uid: u32,
    gid: u32,
    name: Option<String>,
    groups: Vec<u32>,
}

/// GNU `quote()` in a UTF-8 locale: curly quotes, C escapes for `\` and control characters.
fn quote(s: &str) -> String {
    let mut q = String::from("\u{2018}");
    for c in s.chars() {
        match c {
            '\\' => q.push_str("\\\\"),
            '\x07' => q.push_str("\\a"),
            '\x08' => q.push_str("\\b"),
            '\x0c' => q.push_str("\\f"),
            '\n' => q.push_str("\\n"),
            '\r' => q.push_str("\\r"),
            '\t' => q.push_str("\\t"),
            '\x0b' => q.push_str("\\v"),
            c if c.is_ascii_control() => q.push_str(&format!("\\{:03o}", c as u32)),
            c => q.push(c),
        }
    }
    q.push('\u{2019}');
    q
}

/// Run `id` with `args` (without the program name); returns the exit status.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let o = match args::parse(args) {
        Parsed::Run(o) => o,
        Parsed::Help => {
            let _ = out.write_all(args::HELP.as_bytes());
            return 0;
        }
        Parsed::Version => {
            let _ = writeln!(out, "id (skua-tools) {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Parsed::Usage(m) => {
            let _ = writeln!(err, "id: {m}");
            let _ = writeln!(err, "Try 'id --help' for more information.");
            return 1;
        }
        Parsed::Fatal(m) => {
            let _ = writeln!(err, "id: {m}");
            return 1;
        }
    };
    let token = win::token_info();
    let acc = Accounts::new(token.as_ref());
    let mut ok = true;
    if o.users.is_empty() {
        let Some(t) = token.as_ref() else {
            let _ = writeln!(err, "id: cannot get the process token");
            return 1;
        };
        let gid = sid_to_id(&t.primary_group, &acc.ctx);
        let subject = Subject {
            uid: sid_to_id(&t.user, &acc.ctx),
            gid,
            name: acc.by_sid(&t.user, None).map(|p| p.name),
            groups: vec![gid],
        };
        ok &= print(&o, &acc, &subject, out, err);
    }
    for spec in &o.users {
        match acc.by_spec(spec) {
            Some(p) => {
                let subject = Subject {
                    uid: p.uid,
                    gid: p.gid,
                    name: Some(p.name),
                    groups: Vec::new(),
                };
                ok &= print(&o, &acc, &subject, out, err);
            }
            None => {
                let _ = writeln!(err, "id: {}: no such user", quote(spec));
                ok = false;
            }
        }
    }
    if ok { 0 } else { 1 }
}

/// A user's name: the one it was looked up by, else `getpwuid`.
fn user_name(acc: &Accounts, s: &Subject) -> Option<String> {
    s.name.clone().or_else(|| acc.by_uid(s.uid).map(|p| p.name))
}

/// Print one subject in the format the options ask for; false when a name was missing.
fn print(
    o: &Options,
    acc: &Accounts,
    s: &Subject,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> bool {
    let end = if o.zero { '\0' } else { '\n' };
    let mut ok = true;
    let mut group = |g: u32, out: &mut dyn Write, ok: &mut bool| {
        if o.name {
            let _ = writeln!(err, "id: cannot find name for group ID {g}");
            *ok = false;
        }
        let _ = write!(out, "{g}");
    };
    if o.user {
        if o.name {
            match user_name(acc, s) {
                Some(n) => {
                    let _ = write!(out, "{n}");
                }
                None => {
                    let _ = writeln!(err, "id: cannot find name for user ID {}", s.uid);
                    let _ = write!(out, "{}", s.uid);
                    ok = false;
                }
            }
        } else {
            let _ = write!(out, "{}", s.uid);
        }
    } else if o.group {
        group(s.gid, out, &mut ok);
    } else if o.groups {
        let delim = if o.zero { '\0' } else { ' ' };
        group(s.gid, out, &mut ok);
        for &g in s.groups.iter().filter(|g| **g != s.gid) {
            let _ = write!(out, "{delim}");
            group(g, out, &mut ok);
        }
    } else {
        let _ = write!(out, "uid={}", s.uid);
        if let Some(n) = user_name(acc, s) {
            let _ = write!(out, "({n})");
        }
        let _ = write!(out, " gid={}", s.gid);
        if !s.groups.is_empty() {
            let list: Vec<String> = s.groups.iter().map(u32::to_string).collect();
            let _ = write!(out, " groups={}", list.join(","));
        }
    }
    let _ = write!(out, "{end}");
    ok
}

pub fn main_exit() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut err = io::stderr().lock();
    let code = run(&args, &mut out, &mut err);
    if let Err(e) = out.flush() {
        // The reader of a pipe went away: end as a tool killed by SIGPIPE does.
        if matches!(e.raw_os_error(), Some(232 | 109)) {
            return ExitCode::from(141);
        }
        let _ = writeln!(err, "id: write error");
        return ExitCode::from(1);
    }
    ExitCode::from(code)
}
