use std::ffi::c_void;
use windows::core::{w, Interface, Result};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Transparent, topmost, click-through window with a DirectComposition visual
/// (the visual is what wgpu renders into, so per-pixel alpha works).
pub struct Overlay {
    pub hwnd: HWND,
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
}

impl Overlay {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Result<Self> {
        unsafe {
            let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("SnowfallOverlay");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance,
                lpszClassName: class,
                ..Default::default()
            });

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOREDIRECTIONBITMAP,
                class,
                w!("Snowfall"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                hinstance,
                None,
            )?;
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)?;

            // Ctrl+Alt+Shift+Q quits (the app has no other UI).
            let _ = RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_ALT | MOD_SHIFT, 'Q' as u32);

            let device: IDCompositionDevice = DCompositionCreateDevice(None::<&IDXGIDevice>)?;
            let target = device.CreateTargetForHwnd(hwnd, true)?;
            let visual = device.CreateVisual()?;
            target.SetRoot(&visual)?;

            Ok(Self { hwnd, device, _target: target, visual })
        }
    }

    pub fn visual_ptr(&self) -> *mut c_void {
        self.visual.as_raw()
    }

    /// Call after wgpu has attached its swap chain to the visual.
    pub fn commit(&self) -> Result<()> {
        unsafe { self.device.Commit() }
    }

    pub fn show(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_HOTKEY => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
