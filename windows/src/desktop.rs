use snowfall_core::Rect;
use std::ffi::c_void;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
    IsIconic, IsWindowVisible, IsZoomed, GWL_EXSTYLE, WS_EX_TRANSPARENT,
};

const MIN_WIDTH: i32 = 80;
const MIN_HEIGHT: i32 = 20;

pub fn primary_monitor() -> RECT {
    unsafe { monitor_rect(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)) }
}

unsafe fn monitor_rect(m: HMONITOR) -> RECT {
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(m, &mut mi);
    mi.rcMonitor
}

fn same(a: &RECT, b: &RECT) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}

unsafe fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = GetClassNameW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok()
        && cloaked != 0
}

/// Visible frame without the invisible resize/shadow border.
unsafe fn bounds(hwnd: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    let size = std::mem::size_of::<RECT>() as u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, size).is_ok() {
        Some(r)
    } else {
        GetWindowRect(hwnd, &mut r).ok().map(|_| r)
    }
}

/// True when the foreground window is maximized or covers the whole monitor
/// (video player, game, F11 browser, ...). The desktop/taskbar don't count.
pub fn fullscreen_app_active(own: HWND, mon: &RECT) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg == own || !IsWindowVisible(fg).as_bool() {
            return false;
        }
        if matches!(
            class_name(fg).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        if !same(&monitor_rect(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST)), mon) {
            return false;
        }
        if IsZoomed(fg).as_bool() {
            return true;
        }
        match bounds(fg) {
            Some(r) => r.left <= mon.left && r.top <= mon.top && r.right >= mon.right && r.bottom >= mon.bottom,
            None => false,
        }
    }
}

struct EnumState<'a> {
    own: HWND,
    mon: RECT,
    out: &'a mut Vec<Rect>,
}

/// Collects top-level window rectangles (overlay coordinates), front-to-back.
pub fn collect_windows(own: HWND, mon: &RECT, out: &mut Vec<Rect>) {
    out.clear();
    let mut state = EnumState { own, mon: *mon, out };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut state as *mut EnumState as isize));
    }
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let st = &mut *(lparam.0 as *mut EnumState);
    if let Some(r) = window_rect(hwnd, st) {
        st.out.push(r);
    }
    TRUE
}

unsafe fn window_rect(hwnd: HWND, st: &EnumState) -> Option<Rect> {
    if hwnd == st.own
        || !IsWindowVisible(hwnd).as_bool()
        || IsIconic(hwnd).as_bool()
        || is_cloaked(hwnd)
        || (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0) != 0
    {
        return None;
    }
    let class = class_name(hwnd);
    if class == "Progman" || class == "WorkerW" {
        return None;
    }
    if GetWindowTextLengthW(hwnd) == 0 && class != "Shell_TrayWnd" {
        return None;
    }
    let r = bounds(hwnd)?;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w < MIN_WIDTH || h < MIN_HEIGHT || r.right <= st.mon.left || r.left >= st.mon.right {
        return None;
    }
    Some(Rect {
        left: (r.left - st.mon.left) as f32,
        top: (r.top - st.mon.top) as f32,
        right: (r.right - st.mon.left) as f32,
        bottom: (r.bottom - st.mon.top) as f32,
    })
}
