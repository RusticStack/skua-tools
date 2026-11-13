//! Windows SIDs and Cygwin's SID <-> uid/gid mapping (Cygwin user guide, "POSIX accounts,
//! permission, and security", section "Mapping Windows accounts to POSIX accounts"), checked
//! against Git Bash's `id.exe`:
//!
//! | SID | id |
//! | --- | --- |
//! | `S-1-5-RID` (NT authority) and `S-1-5-32-RID` (builtin) | RID |
//! | the current logon session `S-1-5-5-X-Y` | 4095 (`CurrentSession`); other sessions 4094 |
//! | other `S-1-5-X-RID` | 0x1000 * X + RID |
//! | other well-known `S-1-X-Y` | 0x10000 + 0x100 * X + Y |
//! | mandatory labels `S-1-16-RID` | 0x60000 + RID |
//! | this machine's accounts `S-1-5-21-a-b-c-RID` | 0x30000 + RID |
//! | the primary domain's accounts | 0x100000 + RID |
//! | anything else | 4294967295 (-1) |

/// A security identifier: identifier authority and sub-authorities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sid {
    pub authority: u64,
    pub subs: Vec<u32>,
}

/// Cygwin's "no id" (-1 as an unsigned uid).
pub const NO_ID: u32 = u32::MAX;
/// The uid/gid of the current logon session SID.
pub const CURRENT_SESSION: u32 = 4095;
/// The uid/gid of any other logon session SID.
pub const OTHER_SESSION: u32 = 4094;

impl Sid {
    pub fn new(authority: u64, subs: &[u32]) -> Self {
        Sid {
            authority,
            subs: subs.to_vec(),
        }
    }

    /// Parse the binary SID layout (revision, count, 6-byte big-endian authority, LE RIDs).
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 8 || b[0] != 1 {
            return None;
        }
        let n = usize::from(b[1]);
        if b.len() < 8 + 4 * n {
            return None;
        }
        let authority = b[2..8].iter().fold(0u64, |a, &x| (a << 8) | u64::from(x));
        let subs = (0..n)
            .map(|i| {
                let o = 8 + 4 * i;
                u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
            })
            .collect();
        Some(Sid { authority, subs })
    }

    /// The binary SID layout, for Win32 calls.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = vec![1, self.subs.len() as u8];
        v.extend_from_slice(&self.authority.to_be_bytes()[2..]);
        for s in &self.subs {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    /// `S-1-5-21-a-b-c` for `S-1-5-21-a-b-c-RID`: the domain (or machine) of an account.
    pub fn domain(&self) -> Option<Sid> {
        (self.is_account() && self.subs.len() == 5).then(|| Sid {
            authority: 5,
            subs: self.subs[..4].to_vec(),
        })
    }

    /// `S-1-5-21-...`: an account of a machine or domain.
    pub fn is_account(&self) -> bool {
        self.authority == 5 && self.subs.first() == Some(&21)
    }

    /// `S-1-1-0` (Everyone): Cygwin has no passwd entry for it.
    pub fn is_everyone(&self) -> bool {
        self.authority == 1 && self.subs == [0]
    }

    /// `S-1-5-5-X-Y`: a logon session.
    pub fn is_logon(&self) -> bool {
        self.authority == 5 && self.subs.len() == 3 && self.subs[0] == 5
    }

    /// `domain` + `rid`.
    pub fn child(domain: &Sid, rid: u32) -> Sid {
        let mut s = domain.clone();
        s.subs.push(rid);
        s
    }
}

impl std::fmt::Display for Sid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "S-1-{}", self.authority)?;
        for s in &self.subs {
            write!(f, "-{s}")?;
        }
        Ok(())
    }
}

/// What the mapping needs to know about the machine and the session.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// This machine's account domain (`S-1-5-21-a-b-c`).
    pub machine: Option<Sid>,
    /// The primary domain, when the machine is joined to one.
    pub primary: Option<Sid>,
    /// The current logon session SID.
    pub logon: Option<Sid>,
}

/// Cygwin's uid/gid for a SID.
pub fn sid_to_id(sid: &Sid, ctx: &Context) -> u32 {
    let s = &sid.subs;
    match (sid.authority, s.len()) {
        (5, 1) => s[0],
        (5, 2) if s[0] == 32 => s[1],
        (5, 3) if s[0] == 5 => {
            if ctx.logon.as_ref() == Some(sid) {
                CURRENT_SESSION
            } else {
                OTHER_SESSION
            }
        }
        (5, 5) if s[0] == 21 => {
            let dom = sid.domain();
            let rid = s[4];
            if dom.is_some() && dom == ctx.machine {
                0x30000u32.saturating_add(rid)
            } else if dom.is_some() && dom == ctx.primary {
                0x100000u32.saturating_add(rid)
            } else {
                NO_ID
            }
        }
        (5, 2) if s[0] != 21 => s[0]
            .checked_mul(0x1000)
            .and_then(|v| v.checked_add(s[1]))
            .unwrap_or(NO_ID),
        (16, 1) => 0x60000u32.saturating_add(s[0]),
        (x, 1) if x != 5 && x < 0x100 && s[0] < 0x100 => 0x10000 + 0x100 * x as u32 + s[0],
        _ => NO_ID,
    }
}

/// The SIDs that could have uid/gid `id`, most specific first. The caller keeps the first one
/// Windows knows (and whose forward mapping gives `id` back).
pub fn id_candidates(id: u32, ctx: &Context) -> Vec<Sid> {
    let mut v = Vec::new();
    if id == CURRENT_SESSION {
        v.extend(ctx.logon.clone());
        return v;
    }
    if id < 0x200 {
        v.push(Sid::new(5, &[id]));
    } else if id < 0x1000 {
        v.push(Sid::new(5, &[32, id]));
    }
    if (0x1000..0x60000).contains(&id) {
        let (x, rid) = (id >> 12, id & 0xfff);
        if x != 21 && x != 32 && x != 5 {
            v.push(Sid::new(5, &[x, rid]));
        }
    }
    if (0x10000..0x20000).contains(&id) {
        let x = u64::from((id - 0x10000) >> 8);
        if x != 5 && x != 16 {
            v.push(Sid::new(x, &[id & 0xff]));
        }
    }
    if (0x30000..0x100000).contains(&id)
        && let Some(m) = &ctx.machine
    {
        v.push(Sid::child(m, id - 0x30000));
    }
    if (0x60000..0x70000).contains(&id) {
        v.push(Sid::new(16, &[id - 0x60000]));
    }
    if id >= 0x100000
        && id != NO_ID
        && let Some(p) = &ctx.primary
    {
        v.push(Sid::child(p, id - 0x100000));
    }
    v.retain(|s| !s.is_everyone() && sid_to_id(s, ctx) == id);
    v
}
