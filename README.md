# Snowfall

A lightweight snowfall overlay for the Windows desktop. It consists of a transparent window that sits atop the screen and allows mouse clicks to pass through; snow falls in the background, accumulating in drifts along the top edges of windows, the taskbar, and the bottom of the screen - and when a window is closed or minimized, the accumulated snow cascades downward.

<img width="1918" height="1078" alt="Recording 2026-11-18 161711" src="https://github.com/user-attachments/assets/7e4b1162-ab82-4d3b-a742-4d15f33f938a" />

Physics is calculated on the GPU (using wgpu compute shaders), and rendering is performed via two instanced draw calls. The CPU polls the list of windows once every 200 ms and otherwise remains idle.

## Build and Run
```
cargo run --release
```

To exit: **Ctrl+Alt+Shift+Q**. There is no interface or system tray icon.
