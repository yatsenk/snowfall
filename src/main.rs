#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    if let Err(e) = snowfall_windows::run() {
        eprintln!("snowfall: {e}");
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("snowfall: only Windows is supported for now");
}
