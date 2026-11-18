#![cfg(windows)]
//! Windows part: transparent click-through overlay (DirectComposition),
//! desktop window scanning, fullscreen detection and the low-power main loop.

mod desktop;
mod overlay;

use snowfall_core::{Rect, Snowfall};
use std::error::Error;
use std::time::{Duration, Instant};
use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, SetProcessDPIAware, TranslateMessage, MSG,
    PM_REMOVE, QS_ALLINPUT, WM_QUIT,
};

const POLL_ACTIVE: Duration = Duration::from_millis(200);
const POLL_PAUSED: Duration = Duration::from_millis(500);

pub fn run() -> Result<(), Box<dyn Error>> {
    unsafe {
        let _ = SetProcessDPIAware(); // work in physical pixels
    }
    let mon = desktop::primary_monitor();
    let (width, height) = ((mon.right - mon.left) as u32, (mon.bottom - mon.top) as u32);

    let overlay = overlay::Overlay::new(mon.left, mon.top, width as i32, height as i32)?;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..Default::default()
    });
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(overlay.visual_ptr()))?
    };
    let mut app = Snowfall::new(&instance, surface, width, height)?;
    overlay.commit()?;
    trim_memory();

    let mut paused = true; // overlay starts hidden; the first poll decides
    let mut rects: Vec<Rect> = Vec::new();
    let mut next_poll = Instant::now();
    let mut last = next_poll;

    loop {
        if !pump_messages() {
            return Ok(());
        }

        let now = Instant::now();
        if now >= next_poll {
            let fullscreen = desktop::fullscreen_app_active(overlay.hwnd, &mon);
            if fullscreen != paused {
                paused = fullscreen;
                if paused {
                    overlay.hide();
                    trim_memory();
                } else {
                    overlay.show();
                    app.reset(); // snowfall starts over from the top
                    last = now;
                }
            }
            if !paused {
                desktop::collect_windows(overlay.hwnd, &mon, &mut rects);
                app.set_windows(&rects);
            }
            next_poll = now + if paused { POLL_PAUSED } else { POLL_ACTIVE };
        }

        if paused {
            wait(next_poll.saturating_duration_since(Instant::now()));
            continue;
        }

        // Frame pacing comes from vsync: tick() blocks inside Present, so no
        // coarse Windows timer is involved and frames come out evenly spaced.
        let start = Instant::now();
        let dt = (start - last).as_secs_f32().min(0.05);
        last = start;
        app.tick(dt);

        // Safety net: if Present ever returns immediately, don't spin the CPU.
        if start.elapsed() < Duration::from_millis(2) {
            wait(Duration::from_millis(4));
        }
    }
}

/// Sleep in the kernel until the timeout or a window message.
fn wait(d: Duration) {
    let ms = ((d.as_micros() as u64 + 999) / 1000) as u32;
    unsafe {
        MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT);
    }
}

/// Returns false when WM_QUIT was received.
fn pump_messages() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

fn trim_memory() {
    unsafe {
        let _ = K32EmptyWorkingSet(GetCurrentProcess());
    }
}
