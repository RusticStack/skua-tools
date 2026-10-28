//! Calendar and console helpers shared by `gzip`, `unzip` and `zip` (included with `#[path]`).
//!
//! Dates in zip archives are local times (MS-DOS format) and `unzip -l` / `gzip -l` print local
//! times, so Unix seconds are converted with the Windows time-zone rules (the only `unsafe` in
//! the crate lives in `sys`).
#![allow(dead_code)]

/// A broken-down calendar time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// UTC calendar time of Unix seconds.
pub fn civil_from_unix_utc(secs: i64) -> Civil {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    Civil {
        year: y as i32,
        month: m as u32,
        day: d as u32,
        hour: (rem / 3600) as u32,
        min: (rem % 3600 / 60) as u32,
        sec: (rem % 60) as u32,
    }
}

/// Unix seconds of a UTC calendar time.
pub fn unix_from_civil_utc(c: &Civil) -> i64 {
    days_from_civil(i64::from(c.year), i64::from(c.month), i64::from(c.day)) * 86_400
        + i64::from(c.hour) * 3600
        + i64::from(c.min) * 60
        + i64::from(c.sec)
}

/// Local calendar time of Unix seconds.
pub fn local_from_unix(secs: i64) -> Civil {
    sys::local_from_unix(secs)
}

/// Unix seconds of a local calendar time.
pub fn unix_from_local(c: &Civil) -> i64 {
    sys::unix_from_local(c)
}

/// MS-DOS (date, time) words of a calendar time; years outside 1980..=2107 are clamped.
pub fn dos_from_civil(c: &Civil) -> (u16, u16) {
    let year = c.year.clamp(1980, 2107) as u32;
    let date = ((year - 1980) << 9) | (c.month.clamp(1, 12) << 5) | c.day.clamp(1, 31);
    let time = (c.hour.min(23) << 11) | (c.min.min(59) << 5) | (c.sec.min(59) / 2);
    (date as u16, time as u16)
}

/// Calendar time of MS-DOS (date, time) words.
pub fn civil_from_dos(date: u16, time: u16) -> Civil {
    let (date, time) = (u32::from(date), u32::from(time));
    Civil {
        year: 1980 + (date >> 9) as i32,
        month: ((date >> 5) & 15).clamp(1, 12),
        day: (date & 31).max(1),
        hour: (time >> 11).min(23),
        min: ((time >> 5) & 63).min(59),
        sec: ((time & 31) * 2).min(59),
    }
}

/// Removes a file, clearing the read-only attribute first: Windows refuses to delete such a file
/// where Git Bash's `unlink` does not.
pub fn force_remove(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            if let Ok(m) = std::fs::metadata(path) {
                let mut p = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                std::fs::set_permissions(path, p)?;
            }
            std::fs::remove_file(path)
        }
        r => r,
    }
}

/// Gives `to` the read-only attribute of `from` (gzip keeps the mode of the file it replaces).
pub fn copy_readonly(from: &std::fs::Metadata, to: &std::path::Path) {
    if from.permissions().readonly()
        && let Ok(m) = std::fs::metadata(to)
    {
        let mut p = m.permissions();
        p.set_readonly(true);
        let _ = std::fs::set_permissions(to, p);
    }
}

/// Whether a read from stdin would return now (data or end of file) or within `wait_ms`.
/// A terminal or a regular file is always ready; a pipe that stays silent is not, so a pipe
/// nobody writes to cannot hang a prompt (agents run with such stdin).
pub fn stdin_ready(wait_ms: u64) -> bool {
    sys::stdin_ready(wait_ms)
}

#[cfg(windows)]
mod sys {
    use super::{Civil, civil_from_unix_utc, unix_from_civil_utc};
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        dow: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }

    unsafe extern "system" {
        fn SystemTimeToTzSpecificLocalTime(
            tz: *const c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
        fn TzSpecificLocalTimeToSystemTime(
            tz: *const c_void,
            local: *const SystemTime,
            utc: *mut SystemTime,
        ) -> i32;
        fn GetStdHandle(which: u32) -> isize;
        fn GetFileType(handle: isize) -> u32;
        fn PeekNamedPipe(
            handle: isize,
            buf: *mut c_void,
            size: u32,
            read: *mut u32,
            avail: *mut u32,
            left: *mut u32,
        ) -> i32;
    }

    fn to_st(c: &Civil) -> SystemTime {
        SystemTime {
            year: c.year as u16,
            month: c.month as u16,
            day: c.day as u16,
            hour: c.hour as u16,
            minute: c.min as u16,
            second: c.sec as u16,
            ..SystemTime::default()
        }
    }

    fn from_st(s: &SystemTime) -> Civil {
        Civil {
            year: i32::from(s.year),
            month: u32::from(s.month),
            day: u32::from(s.day),
            hour: u32::from(s.hour),
            min: u32::from(s.minute),
            sec: u32::from(s.second),
        }
    }

    pub fn local_from_unix(secs: i64) -> Civil {
        let utc = civil_from_unix_utc(secs);
        if !(1601..=30_000).contains(&utc.year) {
            return utc;
        }
        let (u, mut l) = (to_st(&utc), SystemTime::default());
        // SAFETY: both pointers refer to live SYSTEMTIME structs; a null zone means "current".
        let ok = unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &u, &mut l) };
        if ok == 0 { utc } else { from_st(&l) }
    }

    pub fn unix_from_local(c: &Civil) -> i64 {
        let (l, mut u) = (to_st(c), SystemTime::default());
        // SAFETY: as above.
        let ok = unsafe { TzSpecificLocalTimeToSystemTime(std::ptr::null(), &l, &mut u) };
        unix_from_civil_utc(&if ok == 0 { *c } else { from_st(&u) })
    }

    pub fn stdin_ready(wait_ms: u64) -> bool {
        const STD_INPUT_HANDLE: u32 = -10i32 as u32;
        const FILE_TYPE_PIPE: u32 = 3;
        // SAFETY: plain handle queries; the out parameters are live locals.
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            if h == 0 || h == -1 || GetFileType(h) != FILE_TYPE_PIPE {
                return true;
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
            loop {
                let mut avail = 0u32;
                let ok = PeekNamedPipe(
                    h,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut avail,
                    std::ptr::null_mut(),
                );
                // A failed peek means the writer closed the pipe: a read returns end of file.
                if ok == 0 || avail > 0 {
                    return true;
                }
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{Civil, civil_from_unix_utc, unix_from_civil_utc};

    pub fn local_from_unix(secs: i64) -> Civil {
        civil_from_unix_utc(secs)
    }

    pub fn unix_from_local(c: &Civil) -> i64 {
        unix_from_civil_utc(c)
    }

    pub fn stdin_ready(_wait_ms: u64) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_round_trip() {
        for secs in [0i64, 86_399, 1_000_000_000, 1_767_225_600, 951_782_400] {
            assert_eq!(unix_from_civil_utc(&civil_from_unix_utc(secs)), secs);
        }
        let c = civil_from_unix_utc(1_767_225_600);
        assert_eq!((c.year, c.month, c.day, c.hour), (2026, 1, 1, 0));
    }

    #[test]
    fn dos_round_trip() {
        let c = Civil {
            year: 2026,
            month: 1,
            day: 1,
            hour: 12,
            min: 30,
            sec: 8,
        };
        let (d, t) = dos_from_civil(&c);
        assert_eq!(civil_from_dos(d, t), c);
    }
}
