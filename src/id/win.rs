//! The Win32 side of `id`: the process token, account lookups and the machine's domains.

use super::sid::{Context, Sid};

/// What `LookupAccountSidW`/`LookupAccountNameW` say about an account.
#[derive(Debug, Clone)]
pub struct Account {
    pub sid: Sid,
    pub name: String,
    pub domain: String,
    /// `SID_NAME_USE`.
    pub kind: u32,
}

pub const SID_TYPE_USER: u32 = 1;
pub const SID_TYPE_GROUP: u32 = 2;
pub const SID_TYPE_DOMAIN: u32 = 3;
pub const SID_TYPE_ALIAS: u32 = 4;
pub const SID_TYPE_WELL_KNOWN_GROUP: u32 = 5;
pub const SID_TYPE_LABEL: u32 = 10;

/// The current process: its user, primary group and logon session.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub user: Sid,
    pub primary_group: Sid,
    pub logon: Option<Sid>,
}

#[cfg(windows)]
mod ffi {
    use core::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetCurrentProcess() -> *mut c_void;
        pub fn CloseHandle(h: *mut c_void) -> i32;
        pub fn GetComputerNameW(buf: *mut u16, len: *mut u32) -> i32;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn OpenProcessToken(p: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        pub fn GetTokenInformation(
            token: *mut c_void,
            class: u32,
            buf: *mut c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
        pub fn GetLengthSid(sid: *const c_void) -> u32;
        pub fn LookupAccountSidW(
            system: *const u16,
            sid: *const c_void,
            name: *mut u16,
            name_len: *mut u32,
            domain: *mut u16,
            domain_len: *mut u32,
            kind: *mut u32,
        ) -> i32;
        pub fn LookupAccountNameW(
            system: *const u16,
            name: *const u16,
            sid: *mut c_void,
            sid_len: *mut u32,
            domain: *mut u16,
            domain_len: *mut u32,
            kind: *mut u32,
        ) -> i32;
    }

    #[link(name = "netapi32")]
    unsafe extern "system" {
        pub fn NetGetJoinInformation(
            server: *const u16,
            name: *mut *mut u16,
            status: *mut u32,
        ) -> u32;
        pub fn NetUserGetInfo(
            server: *const u16,
            user: *const u16,
            level: u32,
            buf: *mut *mut u8,
        ) -> u32;
        pub fn NetApiBufferFree(buf: *mut c_void) -> u32;
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A null-terminated UTF-16 string.
///
/// # Safety
/// `p` is null or points to a null-terminated UTF-16 string.
#[cfg(windows)]
unsafe fn from_wide_ptr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: the caller promises a terminator.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: n elements were just read.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

/// The process token's user, primary group and logon SID.
#[cfg(windows)]
pub fn token_info() -> Option<TokenInfo> {
    use core::ffi::c_void;
    use ffi::*;
    const TOKEN_QUERY: u32 = 8;
    const TOKEN_USER: u32 = 1;
    const TOKEN_GROUPS: u32 = 2;
    const TOKEN_PRIMARY_GROUP: u32 = 5;
    const SE_GROUP_LOGON_ID: u32 = 0xC000_0000;

    /// The token class into a u64-aligned buffer.
    fn query(token: *mut c_void, class: u32) -> Option<Vec<u64>> {
        let mut len = 0u32;
        // SAFETY: a size query with a null buffer; `len` is a valid out-pointer.
        unsafe { GetTokenInformation(token, class, core::ptr::null_mut(), 0, &mut len) };
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: the buffer holds `len` bytes.
        let ok =
            unsafe { GetTokenInformation(token, class, buf.as_mut_ptr().cast(), len, &mut len) };
        (ok != 0).then_some(buf)
    }

    /// The SID at `p`.
    ///
    /// # Safety
    /// `p` points to a valid SID.
    unsafe fn sid_at(p: *const c_void) -> Option<Sid> {
        // SAFETY: a valid SID is GetLengthSid bytes long.
        let len = unsafe { GetLengthSid(p) } as usize;
        // SAFETY: as above.
        Sid::from_bytes(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), len) })
    }

    let mut token = core::ptr::null_mut();
    // SAFETY: GetCurrentProcess is a pseudo handle; `token` is a valid out-pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let user = query(token, TOKEN_USER);
    let group = query(token, TOKEN_PRIMARY_GROUP);
    let groups = query(token, TOKEN_GROUPS);
    // SAFETY: the handle was opened above and is closed once.
    unsafe { CloseHandle(token) };

    // TOKEN_USER and TOKEN_PRIMARY_GROUP start with a SID pointer into the same buffer.
    // SAFETY: the buffers were filled by GetTokenInformation for these classes.
    let user = unsafe { sid_at(*user?.as_ptr().cast::<*const c_void>())? };
    // SAFETY: as above.
    let primary_group = unsafe { sid_at(*group?.as_ptr().cast::<*const c_void>())? };
    let logon = groups.and_then(|g| {
        // TOKEN_GROUPS: u32 count, then (padded to 8) SID_AND_ATTRIBUTES { PSID, u32 } entries.
        let base = g.as_ptr().cast::<u8>();
        // SAFETY: the count is the first field.
        let count = unsafe { *base.cast::<u32>() } as usize;
        (0..count).find_map(|i| {
            // SAFETY: entry i of `count` lies in the buffer (16 bytes each after 8 bytes).
            unsafe {
                let entry = base.add(8 + 16 * i);
                let attrs = *entry.add(8).cast::<u32>();
                if attrs & SE_GROUP_LOGON_ID == SE_GROUP_LOGON_ID {
                    sid_at(*entry.cast::<*const c_void>())
                } else {
                    None
                }
            }
        })
    });
    Some(TokenInfo {
        user,
        primary_group,
        logon,
    })
}

/// `LookupAccountSidW` on this machine.
#[cfg(windows)]
pub fn lookup_sid(sid: &Sid) -> Option<Account> {
    let bytes = sid.to_bytes();
    let (mut nlen, mut dlen, mut kind) = (256u32, 256u32, 0u32);
    let mut name = vec![0u16; 256];
    let mut domain = vec![0u16; 256];
    // SAFETY: the SID bytes are a valid SID; the buffers hold the lengths passed.
    let ok = unsafe {
        ffi::LookupAccountSidW(
            core::ptr::null(),
            bytes.as_ptr().cast(),
            name.as_mut_ptr(),
            &mut nlen,
            domain.as_mut_ptr(),
            &mut dlen,
            &mut kind,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(Account {
        sid: sid.clone(),
        name: String::from_utf16_lossy(&name[..nlen as usize]),
        domain: String::from_utf16_lossy(&domain[..dlen as usize]),
        kind,
    })
}

/// `LookupAccountNameW` on this machine.
#[cfg(windows)]
pub fn lookup_name(name: &str) -> Option<Account> {
    if name.is_empty() || name.contains('\0') {
        return None;
    }
    let w = wide(name);
    let mut sid = vec![0u64; 9]; // 72 bytes: SECURITY_MAX_SID_SIZE is 68.
    let (mut slen, mut dlen, mut kind) = (72u32, 256u32, 0u32);
    let mut domain = vec![0u16; 256];
    // SAFETY: `w` is null-terminated; the buffers hold the lengths passed.
    let ok = unsafe {
        ffi::LookupAccountNameW(
            core::ptr::null(),
            w.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut slen,
            domain.as_mut_ptr(),
            &mut dlen,
            &mut kind,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: the SID buffer is 72 bytes long and initialised.
    let bytes = unsafe { std::slice::from_raw_parts(sid.as_ptr().cast::<u8>(), 72) };
    let sid = Sid::from_bytes(bytes)?;
    Some(Account {
        sid,
        name: name.to_owned(),
        domain: String::from_utf16_lossy(&domain[..dlen as usize]),
        kind,
    })
}

/// This machine's NetBIOS name.
#[cfg(windows)]
pub fn computer_name() -> Option<String> {
    let mut buf = vec![0u16; 64];
    let mut len = 64u32;
    // SAFETY: the buffer holds `len` characters.
    let ok = unsafe { ffi::GetComputerNameW(buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// The NetBIOS name of the domain this machine is joined to, if any.
#[cfg(windows)]
pub fn joined_domain() -> Option<String> {
    const NET_SETUP_DOMAIN_NAME: u32 = 3;
    let mut name = core::ptr::null_mut();
    let mut status = 0u32;
    // SAFETY: both are valid out-pointers; the buffer is freed with NetApiBufferFree.
    let rc = unsafe { ffi::NetGetJoinInformation(core::ptr::null(), &mut name, &mut status) };
    if rc != 0 {
        return None;
    }
    // SAFETY: on success `name` is a null-terminated string allocated by the API.
    let s = unsafe { from_wide_ptr(name) };
    // SAFETY: allocated by NetGetJoinInformation, freed once.
    unsafe { ffi::NetApiBufferFree(name.cast()) };
    (status == NET_SETUP_DOMAIN_NAME && !s.is_empty()).then_some(s)
}

/// The RID of a local user's primary group (`USER_INFO_3.usri3_primary_group_id`).
#[cfg(windows)]
pub fn local_primary_group_rid(user: &str) -> Option<u32> {
    /// `USER_INFO_3` up to `usri3_primary_group_id`.
    #[repr(C)]
    struct UserInfo3 {
        name: *mut u16,
        password: *mut u16,
        password_age: u32,
        priv_: u32,
        home_dir: *mut u16,
        comment: *mut u16,
        flags: u32,
        script_path: *mut u16,
        auth_flags: u32,
        full_name: *mut u16,
        usr_comment: *mut u16,
        parms: *mut u16,
        workstations: *mut u16,
        last_logon: u32,
        last_logoff: u32,
        acct_expires: u32,
        max_storage: u32,
        units_per_week: u32,
        logon_hours: *mut u8,
        bad_pw_count: u32,
        num_logons: u32,
        logon_server: *mut u16,
        country_code: u32,
        code_page: u32,
        user_id: u32,
        primary_group_id: u32,
    }
    let w = wide(user);
    let mut buf = core::ptr::null_mut();
    // SAFETY: `w` is null-terminated; `buf` is a valid out-pointer freed below.
    let rc = unsafe { ffi::NetUserGetInfo(core::ptr::null(), w.as_ptr(), 3, &mut buf) };
    if rc != 0 || buf.is_null() {
        return None;
    }
    // SAFETY: level 3 returns a USER_INFO_3, whose prefix is declared above.
    let rid = unsafe { (*buf.cast::<UserInfo3>()).primary_group_id };
    // SAFETY: allocated by NetUserGetInfo, freed once.
    unsafe { ffi::NetApiBufferFree(buf.cast()) };
    Some(rid)
}

/// The machine and primary domain SIDs and the logon SID.
#[cfg(windows)]
pub fn context(token: Option<&TokenInfo>) -> (Context, Option<String>, Option<String>) {
    let computer = computer_name();
    let machine = computer
        .as_deref()
        .and_then(lookup_name)
        .filter(|a| a.kind == SID_TYPE_DOMAIN)
        .map(|a| a.sid);
    let joined = joined_domain();
    let primary = joined
        .as_deref()
        .and_then(lookup_name)
        .filter(|a| a.kind == SID_TYPE_DOMAIN)
        .map(|a| a.sid);
    let ctx = Context {
        machine,
        primary,
        logon: token.and_then(|t| t.logon.clone()),
    };
    (ctx, computer, joined)
}

#[cfg(not(windows))]
pub fn token_info() -> Option<TokenInfo> {
    None
}

#[cfg(not(windows))]
pub fn lookup_sid(_sid: &Sid) -> Option<Account> {
    None
}

#[cfg(not(windows))]
pub fn lookup_name(_name: &str) -> Option<Account> {
    None
}

#[cfg(not(windows))]
pub fn local_primary_group_rid(_user: &str) -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn context(_token: Option<&TokenInfo>) -> (Context, Option<String>, Option<String>) {
    (Context::default(), None, None)
}
