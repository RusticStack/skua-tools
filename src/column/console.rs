//! The console width, for fill mode when stdout is a terminal.

#[cfg(windows)]
pub fn console_width() -> Option<usize> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct Rect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct Info {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        window: Rect,
        max: Coord,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleScreenBufferInfo(handle: *mut c_void, info: *mut Info) -> i32;
    }
    let handle = std::io::stdout().as_raw_handle();
    let mut info = Info {
        size: Coord { x: 0, y: 0 },
        cursor: Coord { x: 0, y: 0 },
        attributes: 0,
        window: Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        max: Coord { x: 0, y: 0 },
    };
    // SAFETY: `info` matches CONSOLE_SCREEN_BUFFER_INFO; the call only writes into it.
    let ok = unsafe { GetConsoleScreenBufferInfo(handle.cast(), &mut info) };
    let w = i32::from(info.window.right) - i32::from(info.window.left) + 1;
    (ok != 0 && w > 0).then_some(w as usize)
}

#[cfg(not(windows))]
pub fn console_width() -> Option<usize> {
    None
}
