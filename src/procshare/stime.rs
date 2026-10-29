//! STIME of `ps`: process start in local time, as Cygwin prints it.

use crate::procs::Proc;

/// Broken-down local time, enough for STIME.
pub struct Local {
    pub month: usize,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

#[cfg(windows)]
fn local_time(unix: u64) -> Option<Local> {
    #[repr(C)]
    #[derive(Default)]
    struct SysTime {
        year: u16,
        month: u16,
        dow: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FileTimeToSystemTime(ft: *const u64, st: *mut SysTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            tz: *const core::ffi::c_void,
            utc: *const SysTime,
            local: *mut SysTime,
        ) -> i32;
    }
    let ft: u64 = (unix + 11_644_473_600) * 10_000_000;
    let (mut utc, mut local) = (SysTime::default(), SysTime::default());
    // SAFETY: both structs are valid for writes, the FILETIME pointer is valid for reads; a
    // null time zone means the current one.
    let ok = unsafe {
        FileTimeToSystemTime(&ft, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(core::ptr::null(), &utc, &mut local) != 0
    };
    ok.then_some(Local {
        month: usize::from(local.month),
        day: local.day,
        hour: local.hour,
        minute: local.minute,
        second: local.second,
    })
}

#[cfg(not(windows))]
fn local_time(unix: u64) -> Option<Local> {
    let day_secs = unix % 86_400;
    Some(Local {
        month: 1,
        day: 1,
        hour: u16::try_from(day_secs / 3600).unwrap_or(0),
        minute: u16::try_from(day_secs % 3600 / 60).unwrap_or(0),
        second: u16::try_from(day_secs % 60).unwrap_or(0),
    })
}

/// STIME as Cygwin prints it: `HH:MM:SS` for a process younger than a day, else `Mon  D`.
pub fn stime_text(t: &Local, age_secs: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if age_secs >= 24 * 3600 {
        let month = MONTHS
            .get(t.month.wrapping_sub(1))
            .copied()
            .unwrap_or("???");
        format!("{month} {:>2}", t.day)
    } else {
        format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second)
    }
}

pub fn stime(p: &Proc, now: u64) -> String {
    match local_time(p.start_secs()) {
        Some(t) => stime_text(&t, now.saturating_sub(p.start_secs())),
        None => "00:00:00".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stime_as_cygwin() {
        let t = Local {
            month: 10,
            day: 5,
            hour: 9,
            minute: 3,
            second: 7,
        };
        assert_eq!(stime_text(&t, 3600), "09:03:07");
        assert_eq!(stime_text(&t, 90_000), "Oct  5");
    }
}
