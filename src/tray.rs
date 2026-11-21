use windows::Win32::Foundation::{BOOL, HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    InsertMenuItemW, SetForegroundWindow, TrackPopupMenuEx, HICON, IMAGE_FLAGS,
    MENUITEMINFOW, MFS_CHECKED, MFS_DEFAULT, MFS_DISABLED, MFS_UNCHECKED, MIIM_FTYPE,
    MIIM_ID, MIIM_STATE, MIIM_STRING, MFT_SEPARATOR, MFT_STRING, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_RIGHTBUTTON,
};

pub const WM_TRAY_ICON: u32 = 0x8000 + 100;

pub const IDM_HEADER: u32 = 1001;
pub const IDM_TOGGLE_OSD: u32 = 1002;
pub const IDM_TOGGLE_LOCK: u32 = 1003;
pub const IDM_OPEN_UI: u32 = 1004;
pub const IDM_AUTOSTART: u32 = 1005;
pub const IDM_EXIT: u32 = 1006;
pub const IDM_TOGGLE_THEME: u32 = 1007;

pub fn load_custom_icon(size: i32) -> HICON {
    let bytes: &[u8] = match size {
        16 => include_bytes!("../resources/icon_16.png"),
        32 => include_bytes!("../resources/icon_32.png"),
        _ => include_bytes!("../resources/icon_48.png"),
    };
    unsafe {
        CreateIconFromResourceEx(
            bytes,
            BOOL(1),
            0x00030000,
            size,
            size,
            IMAGE_FLAGS(0),
        )
        .unwrap_or(HICON(0))
    }
}

pub struct TrayIconManager {
    hwnd: HWND,
    hicon: HICON,
}

impl TrayIconManager {
    pub fn new(hwnd: HWND) -> Self {
        let hicon = load_custom_icon(16);
        let mut mgr = Self { hwnd, hicon };
        mgr.add_icon();
        mgr
    }

    pub fn add_icon(&mut self) -> bool {
        unsafe {
            let mut nid = NOTIFYICONDATAW::default();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY_ICON;
            nid.hIcon = self.hicon;

            let tip = "Volume Master\0".encode_utf16().collect::<Vec<u16>>();
            let copy_len = tip.len().min(nid.szTip.len());
            nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);

            Shell_NotifyIconW(NIM_ADD, &nid).as_bool()
        }
    }

    pub fn remove_icon(&self) {
        unsafe {
            let mut nid = NOTIFYICONDATAW::default();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.hwnd;
            nid.uID = 1;
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            if self.hicon.0 != 0 {
                let _ = DestroyIcon(self.hicon);
            }
        }
    }

    pub fn show_context_menu(
        &self,
        osd_hidden: bool,
        lock_enabled: bool,
        autostart: bool,
        is_dark: bool,
    ) {
        unsafe {
            let hmenu = CreatePopupMenu().unwrap();

            let add_item = |id: u32, text: &str, state: u32, is_sep: bool| {
                let mut wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let mut mii = MENUITEMINFOW::default();
                mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
                mii.fMask = MIIM_ID | MIIM_STATE | MIIM_STRING | MIIM_FTYPE;
                mii.wID = id;
                mii.fState = windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_STATE(state);
                if is_sep {
                    mii.fType = MFT_SEPARATOR;
                } else {
                    mii.fType = MFT_STRING;
                    mii.dwTypeData = windows::core::PWSTR(wide.as_mut_ptr());
                }
                let _ = InsertMenuItemW(hmenu, id, false, &mii);
            };

            // 1. Header
            add_item(IDM_HEADER, "Volume Master", MFS_DISABLED.0, false);
            add_item(0, "", 0, true);

            // 2. OSD Toggle
            let osd_str = if osd_hidden {
                "Hide Volume OSD: [Active]"
            } else {
                "Hide Volume OSD: [Off]"
            };
            add_item(
                IDM_TOGGLE_OSD,
                osd_str,
                if osd_hidden { MFS_CHECKED.0 } else { MFS_UNCHECKED.0 },
                false,
            );

            // 3. Lock Toggle
            let lock_str = if lock_enabled {
                "Volume Lock: [Active]"
            } else {
                "Volume Lock: [Off]"
            };
            add_item(
                IDM_TOGGLE_LOCK,
                lock_str,
                if lock_enabled { MFS_CHECKED.0 } else { MFS_UNCHECKED.0 },
                false,
            );

            add_item(0, "", 0, true);

            // 4. Open Control Center
            add_item(IDM_OPEN_UI, "Open Control Center", MFS_DEFAULT.0, false);

            // 5. Theme Toggle
            let theme_str = if is_dark {
                "Theme: Dark (Switch to Light)"
            } else {
                "Theme: Light (Switch to Dark)"
            };
            add_item(IDM_TOGGLE_THEME, theme_str, 0, false);

            // 6. Autostart
            add_item(
                IDM_AUTOSTART,
                "Start with Windows",
                if autostart { MFS_CHECKED.0 } else { MFS_UNCHECKED.0 },
                false,
            );

            add_item(0, "", 0, true);

            // 7. Exit
            add_item(IDM_EXIT, "Exit", 0, false);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);

            let _ = TrackPopupMenuEx(
                hmenu,
                (TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON).0,
                pt.x,
                pt.y,
                self.hwnd,
                None,
            );

            let _ = DestroyMenu(hmenu);
        }
    }
}
