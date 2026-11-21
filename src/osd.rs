use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::System::StationsAndDesktops::{
    OpenDesktopW, SetThreadDesktop, DESKTOP_CONTROL_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowTextW, IsWindow, SendMessageW, ShowWindow,
    SW_MINIMIZE, SW_RESTORE, WM_APPCOMMAND,
};

const APPCOMMAND_VOLUME_UP: usize = 0xA0000;
const APPCOMMAND_VOLUME_DOWN: usize = 0x90000;

static CACHED_OSD_HWND: AtomicIsize = AtomicIsize::new(0);
static OSD_SHOULD_HIDE: AtomicBool = AtomicBool::new(true);

pub fn set_osd_hide_desired(hide: bool) {
    OSD_SHOULD_HIDE.store(hide, Ordering::SeqCst);
    if hide {
        hide_osd();
    } else {
        show_osd();
    }
}

#[allow(dead_code)]
pub fn is_osd_hide_desired() -> bool {
    OSD_SHOULD_HIDE.load(Ordering::SeqCst)
}

pub fn ensure_attached_to_desktop() {
    unsafe {
        if let Ok(def_desk) = OpenDesktopW(w!("Default"), DESKTOP_CONTROL_FLAGS(0), BOOL(0), 0x01FF) {
            let _ = SetThreadDesktop(def_desk);
        }
    }
}

pub fn find_osd_window() -> HWND {
    ensure_attached_to_desktop();
    unsafe {
        // 1. Windows 11: XamlExplorerHostIslandWindow with empty title and child DesktopWindowXamlSource
        let mut hwnd = HWND(0);
        loop {
            hwnd = FindWindowExW(HWND(0), hwnd, w!("XamlExplorerHostIslandWindow"), PCWSTR::null());
            if hwnd.0 == 0 {
                break;
            }
            let mut title_buf = [0u16; 256];
            let tlen = GetWindowTextW(hwnd, &mut title_buf);
            let title = String::from_utf16_lossy(&title_buf[..tlen as usize]);

            let child = FindWindowExW(
                hwnd,
                HWND(0),
                w!("Windows.UI.Composition.DesktopWindowContentBridge"),
                w!("DesktopWindowXamlSource"),
            );
            if child.0 != 0 && title.is_empty() {
                return hwnd;
            }
        }

        // 2. Windows 10: NativeHWNDHost with child DirectUIHWND
        let mut hwnd_win10 = HWND(0);
        loop {
            hwnd_win10 = FindWindowExW(HWND(0), hwnd_win10, w!("NativeHWNDHost"), PCWSTR::null());
            if hwnd_win10.0 == 0 {
                break;
            }
            let child = FindWindowExW(hwnd_win10, HWND(0), w!("DirectUIHWND"), PCWSTR::null());
            if child.0 != 0 {
                return hwnd_win10;
            }
        }
    }
    HWND(0)
}

pub fn trigger_osd_spawn() {
    ensure_attached_to_desktop();
    unsafe {
        let tray = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null());
        if tray.0 != 0 {
            let _ = SendMessageW(
                tray,
                WM_APPCOMMAND,
                WPARAM(tray.0 as _),
                LPARAM(APPCOMMAND_VOLUME_UP as _),
            );
            std::thread::sleep(Duration::from_millis(60));
            let _ = SendMessageW(
                tray,
                WM_APPCOMMAND,
                WPARAM(tray.0 as _),
                LPARAM(APPCOMMAND_VOLUME_DOWN as _),
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

pub fn hide_osd() -> bool {
    let mut hwnd = HWND(CACHED_OSD_HWND.load(Ordering::SeqCst));
    unsafe {
        if hwnd.0 == 0 || !IsWindow(hwnd).as_bool() {
            hwnd = find_osd_window();
            if hwnd.0 == 0 {
                trigger_osd_spawn();
                hwnd = find_osd_window();
            }
            if hwnd.0 != 0 {
                CACHED_OSD_HWND.store(hwnd.0, Ordering::SeqCst);
            }
        }

        if hwnd.0 != 0 && IsWindow(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            return true;
        }
    }
    false
}

pub fn show_osd() -> bool {
    let hwnd = HWND(CACHED_OSD_HWND.load(Ordering::SeqCst));
    unsafe {
        if hwnd.0 != 0 && IsWindow(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            trigger_osd_spawn();
            return true;
        } else {
            let found = find_osd_window();
            if found.0 != 0 {
                CACHED_OSD_HWND.store(found.0, Ordering::SeqCst);
                let _ = ShowWindow(found, SW_RESTORE);
                trigger_osd_spawn();
                return true;
            }
        }
    }
    false
}

/// Called periodically or on TaskbarCreated (when explorer restarts)
pub fn check_and_enforce_osd() {
    if !OSD_SHOULD_HIDE.load(Ordering::SeqCst) {
        return;
    }

    let hwnd = HWND(CACHED_OSD_HWND.load(Ordering::SeqCst));
    unsafe {
        if hwnd.0 == 0 || !IsWindow(hwnd).as_bool() {
            // Explorer restarted or window was destroyed!
            CACHED_OSD_HWND.store(0, Ordering::SeqCst);
            hide_osd();
        } else {
            // Re-enforce minimize in case something restored it
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
}

pub fn on_explorer_restarted() {
    ensure_attached_to_desktop();
    CACHED_OSD_HWND.store(0, Ordering::SeqCst);
    if OSD_SHOULD_HIDE.load(Ordering::SeqCst) {
        // Give new explorer 500ms to spin up shell components, then spawn & hide
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            hide_osd();
            // Retry once more after 1.5s to be 100% sure
            std::thread::sleep(Duration::from_millis(1500));
            hide_osd();
        });
    }
}
