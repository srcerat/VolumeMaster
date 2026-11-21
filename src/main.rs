#![windows_subsystem = "windows"]

mod audio;
mod autostart;
mod config;
mod osd;
mod tray;
mod ui;

use windows::core::w;
use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

fn main() {
    // 1. Ensure single instance
    let _mutex = unsafe {
        let mutex_res = CreateMutexW(None, true, w!("VolumeMaster_SingleInstance_Mutex"));
        let last_err = windows::Win32::Foundation::GetLastError();
        let already_running = matches!(last_err, Err(ref e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult());

        if already_running {
            // Already running! Bring existing window to front
            let existing_hwnd = FindWindowW(w!("VolumeMasterMainWindow"), None);
            if existing_hwnd.0 != 0 {
                ShowWindow(existing_hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(existing_hwnd);
            }
            return;
        }
        mutex_res.ok()
    };

    // 2. Ensure thread is on Default desktop
    osd::ensure_attached_to_desktop();

    // 3. Launch UI and message pump
    ui::MainWindow::create_and_run();
}
