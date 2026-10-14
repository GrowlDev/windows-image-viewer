//! Small Windows integrations: title bar colour, Paint.NET, Explorer, Open with, Recycle Bin.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use windows_sys::Win32::Foundation::{FILETIME, HWND, SYSTEMTIME};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, OAIF_EXEC,
    OAIF_HIDE_REGISTRATION, OPENASINFO, SHFILEOPSTRUCTW, SHFileOperationW, SHOpenWithDialog,
    ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Hide the window from the screen without hiding it from Windows (no flash while the
/// first frame is prepared).
pub fn set_cloaked(hwnd: HWND, cloaked: bool) {
    const DWMWA_CLOAK: u32 = 13;
    let value: u32 = cloaked.into();
    // SAFETY: valid window handle, 4-byte BOOL value.
    unsafe { DwmSetWindowAttribute(hwnd, DWMWA_CLOAK, (&raw const value).cast(), 4) };
}

/// Paint the native title bar in the app's own colours (Windows 11; ignored elsewhere).
pub fn style_title_bar(hwnd: HWND, caption: [u8; 3], text: [u8; 3]) {
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;
    let colorref = |[r, g, b]: [u8; 3]| r as u32 | (g as u32) << 8 | (b as u32) << 16;
    let set = |attr: u32, value: u32| {
        // SAFETY: valid window handle from the running event loop, 4-byte value.
        unsafe { DwmSetWindowAttribute(hwnd, attr, (&raw const value).cast(), 4) };
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    set(DWMWA_CAPTION_COLOR, colorref(caption));
    set(DWMWA_TEXT_COLOR, colorref(text));
}

/// Run a shell verb ("open", "edit") on a file or URI.
pub fn shell_execute(verb: &str, target: &str) -> Result<(), String> {
    let (verb, target) = (wide(verb), wide(target));
    // SAFETY: NUL-terminated strings that outlive the call; no owner window needed.
    let code = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    } as isize;
    if code > 32 { Ok(()) } else { Err(format!("Couldn't open it (error {code})")) }
}

pub fn show_in_explorer(path: &Path) -> Result<(), String> {
    // Explorer wants the switch and path as one raw argument.
    use std::os::windows::process::CommandExt;
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// The standard Windows "Open with" picker.
pub fn open_with(hwnd: HWND, path: &Path) {
    let file = wide(path);
    let info = OPENASINFO {
        pcszFile: file.as_ptr(),
        pcszClass: std::ptr::null(),
        oaifInFlags: OAIF_EXEC | OAIF_HIDE_REGISTRATION,
    };
    // SAFETY: `file` outlives the modal dialog; hwnd is our window.
    unsafe { SHOpenWithDialog(hwnd, &info) };
}

/// Move to the Recycle Bin (undoable from Explorer), never a permanent delete.
pub fn recycle(path: &Path) -> Result<(), String> {
    let mut from = wide(path);
    from.push(0); // the API takes a double-NUL-terminated list
    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: `from` outlives the call and is double-NUL terminated.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code == 0 && op.fAnyOperationsAborted == 0 && !path.exists() {
        Ok(())
    } else {
        Err(format!("Couldn't move it to the Recycle Bin (error {code:#x})"))
    }
}

/// "14 Oct 2026, 11:07" in local time.
pub fn local_time(time: SystemTime) -> Option<String> {
    let since = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    let ticks = since.as_nanos() as u64 / 100 + 116_444_736_000_000_000;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: plain out-parameters.
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
        {
            return None;
        }
    }
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.get(local.wMonth.wrapping_sub(1) as usize)?;
    Some(format!("{} {month} {}, {:02}:{:02}", local.wDay, local.wYear, local.wHour, local.wMinute))
}

pub fn file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 { format!("{value:.1} {}", UNITS[unit]) } else { format!("{value:.0} {}", UNITS[unit]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_explorer() {
        assert_eq!(file_size(300), "300 bytes");
        assert_eq!(file_size(1229), "1.2 KB");
        assert_eq!(file_size(5 * 1024 * 1024 + 1), "5.0 MB");
        assert_eq!(file_size(250 * 1024), "250 KB");
    }

    #[test]
    fn times_format() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        let s = local_time(t).unwrap();
        assert!(s.contains("2026"), "{s}");
    }

    #[test]
    fn recycling_a_missing_file_is_an_error() {
        assert!(recycle(Path::new(r"C:\definitely\not\here.png")).is_err());
    }
}

/// Diagnostics (IMAGEVIEWER_TRACE_WINDOWS=1 with IMAGEVIEWER_TRACE): log every visibility,
/// cloak and size change of this process's windows, polled each millisecond.
pub fn trace_windows() {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, InternalGetWindowText, IsWindowVisible};

    unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> i32 {
        let mut pid = 0;
        // SAFETY: valid handle from EnumWindows; `list` is the Vec passed below.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() {
                (*(list as *mut Vec<isize>)).push(hwnd as isize);
            }
        }
        1
    }

    std::thread::spawn(|| {
        let mut last = std::collections::HashMap::<isize, String>::new();
        for _ in 0..4000 {
            let mut list: Vec<isize> = Vec::new();
            // SAFETY: the callback only pushes into `list`, which outlives the call.
            unsafe { EnumWindows(Some(collect), &mut list as *mut _ as LPARAM) };
            for h in list {
                let hwnd = h as HWND;
                let mut cloaked = 0u32;
                let mut r: RECT = unsafe { std::mem::zeroed() };
                // SAFETY: out-parameters sized for the attributes requested.
                let state = unsafe {
                    DwmGetWindowAttribute(hwnd, 14, (&raw mut cloaked).cast(), 4);
                    GetWindowRect(hwnd, &mut r);
                    let mut class = [0u16; 128];
                    let n = GetClassNameW(hwnd, class.as_mut_ptr(), 128);
                    let mut title = [0u16; 128];
                    // Not GetWindowText: that waits on the (busy) UI thread.
                    let t = InternalGetWindowText(hwnd, title.as_mut_ptr(), 128);
                    format!(
                        "visible={} cloaked={} rect={}x{} at {},{} class={:?} title={:?}",
                        IsWindowVisible(hwnd), cloaked, r.right - r.left, r.bottom - r.top, r.left, r.top,
                        String::from_utf16_lossy(&class[..n as usize]), String::from_utf16_lossy(&title[..t.max(0) as usize])
                    )
                };
                if last.get(&h) != Some(&state) {
                    crate::trace(&format!("window {h:#x}: {state}"));
                    last.insert(h, state);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
}
