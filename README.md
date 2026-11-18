# Snowfall

A lightweight snowfall overlay for the Windows desktop. It consists of a transparent window that sits atop the screen and allows mouse clicks to pass through; snow falls in the background, accumulating in drifts along the top edges of windows, the taskbar, and the bottom of the screen - and when a window is closed or minimized, the accumulated snow cascades downward.

<img width="1918" height="1020" alt="Recording 2026-11-18 161652 (1)" src="https://github.com/user-attachments/assets/0a9dc280-2d86-45a4-ba36-ee4b816aaf6e" />

Physics is calculated on the GPU (using wgpu compute shaders), and rendering is performed via two instanced draw calls. The CPU polls the list of windows once every 200 ms and otherwise remains idle.


### Installation & Run

1. **Clone the repository**
   ```bash
   git clone https://github.com/yatsenk/snowfall.git
   cd snowfall
   ```

2. **Build and Run**
   ```bash
   cargo run --release
   ```


To exit: **Ctrl+Alt+Shift+Q**. There is no interface or system tray icon.

### Windows (x86_64)
[![Download Snowfall](https://img.shields.io/badge/Download-snowfall.exe-2496ED?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/yatsenk/snowfall/releases/download/0.1.0/snowfall-x86_64-pc-windows-msvc.exe)
