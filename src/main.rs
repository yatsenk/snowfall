#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::Write;

/// The app has no console (windows subsystem), so errors go to `snowfall.log` next to the exe.
fn log(msg: &str) {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("snowfall.log")))
        .unwrap_or_else(|| "snowfall.log".into());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{msg}");
    }
}

#[cfg(windows)]
fn main() {
    // panic = "abort" kills the process right after the hook, so log from the hook
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    if let Err(e) = snowfall_windows::run() {
        log(&format!("error: {e}"));
    }
}

#[cfg(not(windows))]
fn main() {
    log("snowfall: only Windows is supported for now");
}
