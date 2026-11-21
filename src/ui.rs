use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, TextOutW, UpdateWindow,
    FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HFONT, HPEN, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, SetWindowTheme, INITCOMMONCONTROLSEX, ICC_STANDARD_CLASSES,
    ICC_UPDOWN_CLASS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, GetWindowLongPtrW,
    PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SendMessageW,
    SetWindowLongPtrW, ShowWindow, BM_SETCHECK, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON,
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA,
    MSG, SW_HIDE, SW_RESTORE, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND, WM_HSCROLL, WM_LBUTTONDBLCLK,
    WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WM_SETFONT, WM_SETICON, WM_TIMER, WNDCLASSEXW,
    WS_CHILD, WS_CLIPCHILDREN, WS_GROUP, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::audio::{AudioManager, WM_APP_VOLUME_CHANGED};
use crate::autostart::{is_autostart_enabled, set_autostart};
use crate::config::{AppConfig, AppTheme, LockMode};
use crate::osd::{check_and_enforce_osd, on_explorer_restarted, set_osd_hide_desired};
use crate::tray::{
    load_custom_icon, TrayIconManager, IDM_AUTOSTART, IDM_EXIT, IDM_OPEN_UI, IDM_TOGGLE_LOCK,
    IDM_TOGGLE_OSD, IDM_TOGGLE_THEME, WM_TRAY_ICON,
};

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

// Trackbar constants
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETPOS: u32 = 0x0400 + 5;
const TBM_SETRANGE: u32 = 0x0400 + 6;
#[allow(dead_code)]
const TBS_AUTOTICKS: u32 = 0x0001;
const TBS_TRANSPARENTBKGND: u32 = 0x1000;

// Control IDs
const IDC_BTN_TOGGLE_OSD: u32 = 201;
const IDC_STATIC_OSD_STATUS: u32 = 202;
const IDC_STATIC_OSD_NOTE: u32 = 203;

const IDC_STATIC_CUR_VOL: u32 = 204;
const IDC_SLIDER_CUR_VOL: u32 = 205;
const IDC_BTN_MUTE: u32 = 206;

const IDC_CHK_LOCK_ENABLE: u32 = 207;
const IDC_RADIO_EXACT: u32 = 208;
const IDC_STATIC_EXACT: u32 = 209;
const IDC_SLIDER_EXACT: u32 = 210;

const IDC_RADIO_RANGE: u32 = 211;
const IDC_STATIC_MIN: u32 = 212;
const IDC_SLIDER_MIN: u32 = 213;
const IDC_STATIC_MAX: u32 = 214;
const IDC_SLIDER_MAX: u32 = 215;

const IDC_CHK_LOCK_UNMUTE: u32 = 216;

const IDC_CHK_AUTOSTART: u32 = 217;
const IDC_CHK_MIN_TO_TRAY: u32 = 218;
const IDC_BTN_TOGGLE_THEME: u32 = 219;
const IDC_BTN_HIDE_TO_TRAY: u32 = 220;

static TASKBAR_CREATED_MSG: Mutex<u32> = Mutex::new(0);

#[allow(dead_code)]
unsafe fn enable_dark_mode_for_app_and_window(hwnd: HWND, is_dark: bool) {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::PCSTR;
    if let Ok(uxtheme) = LoadLibraryW(w!("uxtheme.dll")) {
        let fn_preferred = GetProcAddress(uxtheme, PCSTR(135 as usize as *const u8));
        if let Some(f) = fn_preferred {
            let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set_mode(if is_dark { 2 } else { 3 });
        }
        let fn_allow = GetProcAddress(uxtheme, PCSTR(133 as usize as *const u8));
        if let Some(f) = fn_allow {
            let allow_dark: extern "system" fn(HWND, windows::Win32::Foundation::BOOL) -> windows::Win32::Foundation::BOOL = std::mem::transmute(f);
            allow_dark(hwnd, windows::Win32::Foundation::BOOL(if is_dark { 1 } else { 0 }));
        }
    }
}

pub struct ThemeColors {
    pub is_dark: bool,
    pub color_bg: COLORREF,
    pub color_card: COLORREF,
    pub color_border: COLORREF,
    pub color_text_primary: COLORREF,
    pub color_text_secondary: COLORREF,
    pub color_accent: COLORREF,
    pub brush_bg: HBRUSH,
    pub brush_card: HBRUSH,
    pub pen_border: HPEN,
}

impl ThemeColors {
    pub fn new(theme: AppTheme) -> Self {
        unsafe {
            if theme == AppTheme::Dark {
                let color_bg = rgb(24, 24, 27);         // #18181b
                let color_card = rgb(34, 34, 38);       // #222226
                let color_border = rgb(55, 55, 62);     // #37373e
                let color_text_primary = rgb(245, 245, 247);
                let color_text_secondary = rgb(161, 161, 170);
                let color_accent = rgb(59, 130, 246);

                let brush_bg = CreateSolidBrush(color_bg);
                let brush_card = CreateSolidBrush(color_card);
                let pen_border = CreatePen(PS_SOLID, 1, color_border);

                Self {
                    is_dark: true,
                    color_bg,
                    color_card,
                    color_border,
                    color_text_primary,
                    color_text_secondary,
                    color_accent,
                    brush_bg,
                    brush_card,
                    pen_border,
                }
            } else {
                let color_bg = rgb(244, 244, 246);      // #f4f4f6
                let color_card = rgb(255, 255, 255);    // #ffffff
                let color_border = rgb(226, 226, 232);  // #e2e2e8
                let color_text_primary = rgb(24, 24, 27);
                let color_text_secondary = rgb(113, 113, 122);
                let color_accent = rgb(37, 99, 235);

                let brush_bg = CreateSolidBrush(color_bg);
                let brush_card = CreateSolidBrush(color_card);
                let pen_border = CreatePen(PS_SOLID, 1, color_border);

                Self {
                    is_dark: false,
                    color_bg,
                    color_card,
                    color_border,
                    color_text_primary,
                    color_text_secondary,
                    color_accent,
                    brush_bg,
                    brush_card,
                    pen_border,
                }
            }
        }
    }

    pub fn dispose(&mut self) {
        unsafe {
            if self.brush_bg.0 != 0 {
                DeleteObject(self.brush_bg);
            }
            if self.brush_card.0 != 0 {
                DeleteObject(self.brush_card);
            }
            if self.pen_border.0 != 0 {
                DeleteObject(self.pen_border);
            }
        }
    }
}

pub struct MainWindow {
    pub hwnd: HWND,
    pub config: AppConfig,
    pub tray: Option<TrayIconManager>,
    pub theme: ThemeColors,
    pub hfont_regular: HFONT,
    pub hfont_bold: HFONT,
    pub hfont_card_title: HFONT,

    // Controls
    pub btn_toggle_osd: HWND,
    pub static_osd_status: HWND,
    pub static_osd_note: HWND,
    pub static_cur_vol: HWND,
    pub slider_cur_vol: HWND,
    pub btn_mute: HWND,

    pub chk_lock_enable: HWND,
    pub radio_exact: HWND,
    pub static_exact: HWND,
    pub slider_exact: HWND,

    pub radio_range: HWND,
    pub static_min: HWND,
    pub slider_min: HWND,
    pub static_max: HWND,
    pub slider_max: HWND,

    pub chk_lock_unmute: HWND,
    pub chk_autostart: HWND,
    pub chk_min_to_tray: HWND,
    pub btn_toggle_theme: HWND,
    pub btn_hide_to_tray: HWND,
}

impl MainWindow {
    pub fn create_and_run() {
        unsafe {
            let mut icc = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_STANDARD_CLASSES | ICC_UPDOWN_CLASS,
            };
            let _ = InitCommonControlsEx(&mut icc);

            let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
                w!("VolumeMaster.AudioControl.1"),
            );

            let taskbar_msg = RegisterWindowMessageW(w!("TaskbarCreated"));
            *TASKBAR_CREATED_MSG.lock().unwrap() = taskbar_msg;

            let class_name = w!("VolumeMasterMainWindow");
            let hinstance: HINSTANCE = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null())
                .unwrap_or_default()
                .into();

            let hicon_big = load_custom_icon(32);
            let hicon_sm = load_custom_icon(16);

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: hicon_big,
                hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                    HINSTANCE(0),
                    windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
                )
                .unwrap_or_default(),
                hbrBackground: HBRUSH(0),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: class_name,
                hIconSm: hicon_sm,
            };

            RegisterClassExW(&wc);

            let style = WS_OVERLAPPEDWINDOW.0
                & !windows::Win32::UI::WindowsAndMessaging::WS_THICKFRAME.0
                & !windows::Win32::UI::WindowsAndMessaging::WS_MAXIMIZEBOX.0
                | WS_CLIPCHILDREN.0;

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                w!("Volume Master - Control Center"),
                windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                450,
                650,
                HWND(0),
                windows::Win32::UI::WindowsAndMessaging::HMENU(0),
                hinstance,
                None,
            );

            // Set Taskbar and Titlebar Icons directly
            SendMessageW(hwnd, WM_SETICON, WPARAM(1 /* ICON_BIG */), LPARAM(hicon_big.0));
            SendMessageW(hwnd, WM_SETICON, WPARAM(0 /* ICON_SMALL */), LPARAM(hicon_sm.0));

            ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);

            // Main message pump
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND(0), 0, 0).as_bool() {
                let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    pub fn new(hwnd: HWND) -> Self {
        let config = AppConfig::load();
        let theme = ThemeColors::new(config.theme);

        unsafe {
            let hfont_regular = CreateFontW(
                15, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, 1, 0, 0, 0, 0, w!("Segoe UI"),
            );
            let hfont_bold = CreateFontW(
                15, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, 1, 0, 0, 0, 0, w!("Segoe UI"),
            );
            let hfont_card_title = CreateFontW(
                16, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0, 1, 0, 0, 0, 0, w!("Segoe UI"),
            );

            let tray = Some(TrayIconManager::new(hwnd));

            Self {
                hwnd,
                config,
                tray,
                theme,
                hfont_regular,
                hfont_bold,
                hfont_card_title,
                btn_toggle_osd: HWND(0),
                static_osd_status: HWND(0),
                static_osd_note: HWND(0),
                static_cur_vol: HWND(0),
                slider_cur_vol: HWND(0),
                btn_mute: HWND(0),
                chk_lock_enable: HWND(0),
                radio_exact: HWND(0),
                static_exact: HWND(0),
                slider_exact: HWND(0),
                radio_range: HWND(0),
                static_min: HWND(0),
                slider_min: HWND(0),
                static_max: HWND(0),
                slider_max: HWND(0),
                chk_lock_unmute: HWND(0),
                chk_autostart: HWND(0),
                chk_min_to_tray: HWND(0),
                btn_toggle_theme: HWND(0),
                btn_hide_to_tray: HWND(0),
            }
        }
    }

    pub fn init_controls(&mut self) {
        let hwnd = self.hwnd;
        let hinstance: HINSTANCE = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null())
                .unwrap_or_default()
                .into()
        };

        unsafe {
            let create_ctrl = |class: PCWSTR, text: PCWSTR, style: u32, x, y, w, h, id: u32| -> HWND {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    text,
                    WS_CHILD | WS_VISIBLE | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
                    x,
                    y,
                    w,
                    h,
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::HMENU(id as _),
                    hinstance,
                    None,
                )
            };

            // Section 1: OSD Box (Card 1: y = 12..102)
            self.static_osd_status = create_ctrl(w!("STATIC"), w!("Volume OSD: Loading..."), 0, 28, 42, 230, 20, IDC_STATIC_OSD_STATUS);
            self.btn_toggle_osd = create_ctrl(w!("BUTTON"), w!("Toggle OSD"), BS_PUSHBUTTON as u32, 268, 38, 142, 28, IDC_BTN_TOGGLE_OSD);
            self.static_osd_note = create_ctrl(w!("STATIC"), w!("✔ Auto-recovery on Explorer restart is active"), 0, 28, 72, 380, 20, IDC_STATIC_OSD_NOTE);

            // Section 2: Master Volume (Card 2: y = 112..218)
            self.static_cur_vol = create_ctrl(w!("STATIC"), w!("Current Volume: 50%"), 0, 28, 142, 230, 20, IDC_STATIC_CUR_VOL);
            self.btn_mute = create_ctrl(w!("BUTTON"), w!("Unmuted 🔊"), BS_PUSHBUTTON as u32, 280, 138, 130, 28, IDC_BTN_MUTE);
            self.slider_cur_vol = create_ctrl(w!("msctls_trackbar32"), w!(""), TBS_TRANSPARENTBKGND, 28, 172, 382, 32, IDC_SLIDER_CUR_VOL);

            // Section 3: Volume Locker & Limits (Card 3: y = 228..468)
            self.chk_lock_enable = create_ctrl(w!("BUTTON"), w!("Enable Volume Lock / Limiter"), BS_AUTOCHECKBOX as u32, 28, 258, 380, 22, IDC_CHK_LOCK_ENABLE);

            // Exact mode
            self.radio_exact = create_ctrl(w!("BUTTON"), w!("Lock to exact volume:"), (BS_AUTORADIOBUTTON as u32) | WS_GROUP.0, 28, 288, 190, 20, IDC_RADIO_EXACT);
            self.static_exact = create_ctrl(w!("STATIC"), w!("Target: 50%"), 0, 230, 288, 180, 20, IDC_STATIC_EXACT);
            self.slider_exact = create_ctrl(w!("msctls_trackbar32"), w!(""), TBS_TRANSPARENTBKGND, 38, 312, 372, 30, IDC_SLIDER_EXACT);

            // Range mode
            self.radio_range = create_ctrl(w!("BUTTON"), w!("Clamp between Min and Max limits:"), BS_AUTORADIOBUTTON as u32, 28, 348, 380, 20, IDC_RADIO_RANGE);
            self.static_min = create_ctrl(w!("STATIC"), w!("Min: 20%"), 0, 38, 372, 160, 20, IDC_STATIC_MIN);
            self.slider_min = create_ctrl(w!("msctls_trackbar32"), w!(""), TBS_TRANSPARENTBKGND, 38, 394, 170, 30, IDC_SLIDER_MIN);

            self.static_max = create_ctrl(w!("STATIC"), w!("Max: 80%"), 0, 230, 372, 160, 20, IDC_STATIC_MAX);
            self.slider_max = create_ctrl(w!("msctls_trackbar32"), w!(""), TBS_TRANSPARENTBKGND, 230, 394, 170, 30, IDC_SLIDER_MAX);

            self.chk_lock_unmute = create_ctrl(w!("BUTTON"), w!("Lock Mute State (Keep Unmuted, ignores mute key)"), BS_AUTOCHECKBOX as u32, 28, 434, 380, 22, IDC_CHK_LOCK_UNMUTE);

            // Section 4: Preferences (Card 4: y = 478..588)
            self.chk_autostart = create_ctrl(w!("BUTTON"), w!("Start with Windows (System Startup)"), BS_AUTOCHECKBOX as u32, 28, 508, 380, 22, IDC_CHK_AUTOSTART);
            self.chk_min_to_tray = create_ctrl(w!("BUTTON"), w!("Minimize to System Tray on Close (X)"), BS_AUTOCHECKBOX as u32, 28, 532, 380, 22, IDC_CHK_MIN_TO_TRAY);

            let theme_label = if self.config.theme == AppTheme::Dark {
                w!("Theme: Dark")
            } else {
                w!("Theme: Light")
            };
            self.btn_toggle_theme = create_ctrl(w!("BUTTON"), theme_label, BS_PUSHBUTTON as u32, 28, 558, 160, 28, IDC_BTN_TOGGLE_THEME);
            self.btn_hide_to_tray = create_ctrl(w!("BUTTON"), w!("Minimize to Tray"), BS_DEFPUSHBUTTON as u32, 268, 558, 142, 28, IDC_BTN_HIDE_TO_TRAY);

            // Apply slider ranges (0..100)
            let set_range = |s: HWND| {
                SendMessageW(s, TBM_SETRANGE, WPARAM(1), LPARAM((100 << 16) | 0));
            };
            set_range(self.slider_cur_vol);
            set_range(self.slider_exact);
            set_range(self.slider_min);
            set_range(self.slider_max);

            // Apply fonts
            let apply_font = |ctrl: HWND, font: HFONT| {
                SendMessageW(ctrl, WM_SETFONT, WPARAM(font.0 as _), LPARAM(1));
            };

            let all_ctrls = [
                self.btn_toggle_osd, self.static_osd_status, self.static_osd_note, self.static_cur_vol,
                self.btn_mute, self.chk_lock_enable, self.radio_exact, self.static_exact,
                self.radio_range, self.static_min, self.static_max, self.chk_lock_unmute,
                self.chk_autostart, self.chk_min_to_tray, self.btn_toggle_theme, self.btn_hide_to_tray,
            ];
            for c in all_ctrls {
                apply_font(c, self.hfont_regular);
            }
            apply_font(self.static_osd_status, self.hfont_bold);
            apply_font(self.static_cur_vol, self.hfont_bold);

            self.apply_theme_to_window();
            self.sync_ui_from_state();

            // Set 1-second timer for explorer/osd health check
            windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, 1, 1000, None);
        }
    }

    pub fn apply_theme_to_window(&mut self) {
        unsafe {
            // 1. Update Title bar dark mode
            let dark_val = if self.config.theme == AppTheme::Dark { 1 } else { 0 };
            let dark_mode = windows::Win32::Foundation::BOOL(dark_val);
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as _,
                std::mem::size_of_val(&dark_mode) as u32,
            );

            // 2. Refresh theme colors
            self.theme.dispose();
            self.theme = ThemeColors::new(self.config.theme);

            enable_dark_mode_for_app_and_window(self.hwnd, self.config.theme == AppTheme::Dark);

            // 3. Set Windows 11 control styles
            let ctrl_theme = if self.config.theme == AppTheme::Dark {
                w!("DarkMode_Explorer")
            } else {
                w!("Explorer")
            };

            let ctrls = [
                self.btn_toggle_osd, self.btn_mute, self.btn_toggle_theme, self.btn_hide_to_tray,
                self.chk_lock_enable, self.radio_exact, self.radio_range, self.chk_lock_unmute,
                self.chk_autostart, self.chk_min_to_tray,
            ];
            for c in ctrls {
                let _ = SetWindowTheme(c, ctrl_theme, PCWSTR::null());
            }

            // 4. Update theme toggle button label
            if self.config.theme == AppTheme::Dark {
                self.set_button_text(self.btn_toggle_theme, "Theme: Dark");
            } else {
                self.set_button_text(self.btn_toggle_theme, "Theme: Light");
            }

            // 5. Invalidate window and all child controls to redraw
            let _ = InvalidateRect(self.hwnd, None, true);
            let child_ctrls = [
                self.slider_cur_vol, self.slider_exact, self.slider_min, self.slider_max,
                self.static_osd_status, self.static_osd_note, self.static_cur_vol,
                self.static_exact, self.static_min, self.static_max,
                self.chk_lock_enable, self.radio_exact, self.radio_range, self.chk_lock_unmute,
                self.chk_autostart, self.chk_min_to_tray,
                self.btn_toggle_osd, self.btn_mute, self.btn_toggle_theme, self.btn_hide_to_tray,
            ];
            for c in child_ctrls {
                let _ = InvalidateRect(c, None, true);
                let _ = UpdateWindow(c);
            }
            let _ = UpdateWindow(self.hwnd);
        }
    }

    pub fn sync_ui_from_state(&mut self) {
        unsafe {
            set_osd_hide_desired(self.config.osd_hidden);
            self.update_osd_ui();

            let cur_vol = AudioManager::get_volume();
            let is_muted = AudioManager::is_muted();
            self.update_volume_ui(cur_vol, is_muted);

            let check = if self.config.lock_enabled { 1usize } else { 0usize };
            SendMessageW(self.chk_lock_enable, BM_SETCHECK, WPARAM(check), LPARAM(0));

            let exact_check = if self.config.lock_mode == LockMode::Exact { 1usize } else { 0usize };
            SendMessageW(self.radio_exact, BM_SETCHECK, WPARAM(exact_check), LPARAM(0));

            let range_check = if self.config.lock_mode == LockMode::Range { 1usize } else { 0usize };
            SendMessageW(self.radio_range, BM_SETCHECK, WPARAM(range_check), LPARAM(0));

            SendMessageW(self.slider_exact, TBM_SETPOS, WPARAM(1), LPARAM(self.config.exact_volume as _));
            self.set_static_text(self.static_exact, &format!("Target: {}%", self.config.exact_volume));

            SendMessageW(self.slider_min, TBM_SETPOS, WPARAM(1), LPARAM(self.config.min_volume as _));
            self.set_static_text(self.static_min, &format!("Min: {}%", self.config.min_volume));

            SendMessageW(self.slider_max, TBM_SETPOS, WPARAM(1), LPARAM(self.config.max_volume as _));
            self.set_static_text(self.static_max, &format!("Max: {}%", self.config.max_volume));

            let unmute_check = if self.config.lock_unmuted { 1usize } else { 0usize };
            SendMessageW(self.chk_lock_unmute, BM_SETCHECK, WPARAM(unmute_check), LPARAM(0));

            let auto_enabled = is_autostart_enabled();
            self.config.start_with_windows = auto_enabled;
            let auto_check = if auto_enabled { 1usize } else { 0usize };
            SendMessageW(self.chk_autostart, BM_SETCHECK, WPARAM(auto_check), LPARAM(0));

            let tray_check = if self.config.minimize_to_tray { 1usize } else { 0usize };
            SendMessageW(self.chk_min_to_tray, BM_SETCHECK, WPARAM(tray_check), LPARAM(0));

            AudioManager::update_settings(
                self.config.lock_enabled,
                self.config.lock_mode,
                self.config.exact_volume,
                self.config.min_volume,
                self.config.max_volume,
                self.config.lock_unmuted,
            );
        }
    }

    pub fn update_osd_ui(&self) {
        if self.config.osd_hidden {
            self.set_static_text(self.static_osd_status, "Volume OSD: [ HIDDEN ]");
            self.set_button_text(self.btn_toggle_osd, "Show Volume OSD");
        } else {
            self.set_static_text(self.static_osd_status, "Volume OSD: [ VISIBLE ]");
            self.set_button_text(self.btn_toggle_osd, "Hide Volume OSD");
        }
    }

    pub fn update_volume_ui(&self, volume: u32, muted: bool) {
        unsafe {
            self.set_static_text(self.static_cur_vol, &format!("Current Volume: {}%", volume));
            SendMessageW(self.slider_cur_vol, TBM_SETPOS, WPARAM(1), LPARAM(volume as _));
            if muted {
                self.set_button_text(self.btn_mute, "Muted 🔇");
            } else {
                self.set_button_text(self.btn_mute, "Unmuted 🔊");
            }
        }
    }

    pub fn set_static_text(&self, hwnd: HWND, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
        }
    }

    pub fn set_button_text(&self, hwnd: HWND, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
        }
    }
}

// Window procedure
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar_created = *TASKBAR_CREATED_MSG.lock().unwrap();
    if taskbar_created != 0 && msg == taskbar_created {
        // Explorer restarted! Re-add tray icon and re-hide OSD
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
        if !ptr.is_null() {
            let win = &mut *ptr;
            if let Some(tray) = win.tray.as_mut() {
                tray.add_icon();
            }
        }
        on_explorer_restarted();
        return LRESULT(0);
    }

    match msg {
        WM_CREATE => {
            let mut main_win = Box::new(MainWindow::new(hwnd));
            let _ = AudioManager::init(hwnd);
            main_win.init_controls();

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(main_win) as isize);
            LRESULT(0)
        }

        WM_APP_VOLUME_CHANGED => {
            let volume = wparam.0 as u32;
            let muted = lparam.0 != 0;
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &mut *ptr;
                win.update_volume_ui(volume, muted);
            }
            LRESULT(0)
        }

        WM_TIMER => {
            // Periodic 1s health check for OSD window
            check_and_enforce_osd();
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as _);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &*ptr;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, win.theme.brush_bg);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_PAINT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &*ptr;
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);

                // Draw Cards
                let old_brush = SelectObject(hdc, win.theme.brush_card);
                let old_pen = SelectObject(hdc, win.theme.pen_border);

                // Card 1: OSD
                let _ = RoundRect(hdc, 15, 12, 425, 102, 12, 12);
                // Card 2: Volume Control
                let _ = RoundRect(hdc, 15, 112, 425, 218, 12, 12);
                // Card 3: Locker & Limits
                let _ = RoundRect(hdc, 15, 228, 425, 468, 12, 12);
                // Card 4: Preferences
                let _ = RoundRect(hdc, 15, 478, 425, 598, 12, 12);

                // Draw Card Section Titles with Card Title Font
                SelectObject(hdc, win.hfont_card_title);
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, win.theme.color_text_primary);

                let _ = TextOutW(hdc, 28, 20, w!("Windows 11 Volume Indicator (OSD)").as_wide());
                let _ = TextOutW(hdc, 28, 120, w!("System Volume Control").as_wide());
                let _ = TextOutW(hdc, 28, 236, w!("Volume Locker & Limits").as_wide());
                let _ = TextOutW(hdc, 28, 486, w!("Preferences").as_wide());

                SelectObject(hdc, old_pen);
                SelectObject(hdc, old_brush);
                EndPaint(hwnd, &ps);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as _);
            let static_hwnd = HWND(lparam.0);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &*ptr;
                SetBkMode(hdc, TRANSPARENT);
                if static_hwnd == win.static_osd_note {
                    SetTextColor(hdc, win.theme.color_text_secondary);
                } else if static_hwnd == win.static_osd_status {
                    if win.config.osd_hidden {
                        SetTextColor(hdc, win.theme.color_accent);
                    } else {
                        SetTextColor(hdc, win.theme.color_text_primary);
                    }
                } else {
                    SetTextColor(hdc, win.theme.color_text_primary);
                }
                return LRESULT(win.theme.brush_card.0 as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_CTLCOLORBTN => {
            let hdc = HDC(wparam.0 as _);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &*ptr;
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, win.theme.color_text_primary);
                return LRESULT(win.theme.brush_card.0 as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_HSCROLL => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &mut *ptr;
                let slider_hwnd = HWND(lparam.0);

                if slider_hwnd == win.slider_cur_vol {
                    let pos = SendMessageW(slider_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as u32;
                    AudioManager::set_volume(pos);
                    win.set_static_text(win.static_cur_vol, &format!("Current Volume: {}%", pos));

                    if win.config.lock_mode == LockMode::Exact {
                        win.config.exact_volume = pos;
                        SendMessageW(win.slider_exact, TBM_SETPOS, WPARAM(1), LPARAM(pos as _));
                        win.set_static_text(win.static_exact, &format!("Target: {}%", pos));
                        AudioManager::update_settings(
                            win.config.lock_enabled,
                            win.config.lock_mode,
                            win.config.exact_volume,
                            win.config.min_volume,
                            win.config.max_volume,
                            win.config.lock_unmuted,
                        );
                        win.config.save();
                    }
                } else if slider_hwnd == win.slider_exact {
                    let pos = SendMessageW(slider_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as u32;
                    win.config.exact_volume = pos;
                    win.set_static_text(win.static_exact, &format!("Target: {}%", pos));
                    AudioManager::update_settings(
                        win.config.lock_enabled,
                        win.config.lock_mode,
                        win.config.exact_volume,
                        win.config.min_volume,
                        win.config.max_volume,
                        win.config.lock_unmuted,
                    );
                    win.config.save();
                } else if slider_hwnd == win.slider_min {
                    let mut pos = SendMessageW(slider_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as u32;
                    if pos > win.config.max_volume {
                        pos = win.config.max_volume;
                        SendMessageW(slider_hwnd, TBM_SETPOS, WPARAM(1), LPARAM(pos as _));
                    }
                    win.config.min_volume = pos;
                    win.set_static_text(win.static_min, &format!("Min: {}%", pos));
                    AudioManager::update_settings(
                        win.config.lock_enabled,
                        win.config.lock_mode,
                        win.config.exact_volume,
                        win.config.min_volume,
                        win.config.max_volume,
                        win.config.lock_unmuted,
                    );
                    win.config.save();
                } else if slider_hwnd == win.slider_max {
                    let mut pos = SendMessageW(slider_hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as u32;
                    if pos < win.config.min_volume {
                        pos = win.config.min_volume;
                        SendMessageW(slider_hwnd, TBM_SETPOS, WPARAM(1), LPARAM(pos as _));
                    }
                    win.config.max_volume = pos;
                    win.set_static_text(win.static_max, &format!("Max: {}%", pos));
                    AudioManager::update_settings(
                        win.config.lock_enabled,
                        win.config.lock_mode,
                        win.config.exact_volume,
                        win.config.min_volume,
                        win.config.max_volume,
                        win.config.lock_unmuted,
                    );
                    win.config.save();
                }
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &mut *ptr;

                match id {
                    IDC_BTN_TOGGLE_OSD | IDM_TOGGLE_OSD => {
                        win.config.osd_hidden = !win.config.osd_hidden;
                        set_osd_hide_desired(win.config.osd_hidden);
                        win.update_osd_ui();
                        let _ = InvalidateRect(win.static_osd_status, None, true);
                        win.config.save();
                    }

                    IDC_BTN_MUTE => {
                        let cur_muted = AudioManager::is_muted();
                        AudioManager::set_mute(!cur_muted);
                        win.update_volume_ui(AudioManager::get_volume(), !cur_muted);
                    }

                    IDC_CHK_LOCK_ENABLE | IDM_TOGGLE_LOCK => {
                        win.config.lock_enabled = !win.config.lock_enabled;
                        let check = if win.config.lock_enabled { 1usize } else { 0usize };
                        SendMessageW(win.chk_lock_enable, BM_SETCHECK, WPARAM(check), LPARAM(0));
                        AudioManager::update_settings(
                            win.config.lock_enabled,
                            win.config.lock_mode,
                            win.config.exact_volume,
                            win.config.min_volume,
                            win.config.max_volume,
                            win.config.lock_unmuted,
                        );
                        win.config.save();
                    }

                    IDC_RADIO_EXACT => {
                        win.config.lock_mode = LockMode::Exact;
                        SendMessageW(win.radio_exact, BM_SETCHECK, WPARAM(1), LPARAM(0));
                        SendMessageW(win.radio_range, BM_SETCHECK, WPARAM(0), LPARAM(0));
                        AudioManager::update_settings(
                            win.config.lock_enabled,
                            win.config.lock_mode,
                            win.config.exact_volume,
                            win.config.min_volume,
                            win.config.max_volume,
                            win.config.lock_unmuted,
                        );
                        win.config.save();
                    }

                    IDC_RADIO_RANGE => {
                        win.config.lock_mode = LockMode::Range;
                        SendMessageW(win.radio_range, BM_SETCHECK, WPARAM(1), LPARAM(0));
                        SendMessageW(win.radio_exact, BM_SETCHECK, WPARAM(0), LPARAM(0));
                        AudioManager::update_settings(
                            win.config.lock_enabled,
                            win.config.lock_mode,
                            win.config.exact_volume,
                            win.config.min_volume,
                            win.config.max_volume,
                            win.config.lock_unmuted,
                        );
                        win.config.save();
                    }

                    IDC_CHK_LOCK_UNMUTE => {
                        win.config.lock_unmuted = !win.config.lock_unmuted;
                        let check = if win.config.lock_unmuted { 1usize } else { 0usize };
                        SendMessageW(win.chk_lock_unmute, BM_SETCHECK, WPARAM(check), LPARAM(0));
                        AudioManager::update_settings(
                            win.config.lock_enabled,
                            win.config.lock_mode,
                            win.config.exact_volume,
                            win.config.min_volume,
                            win.config.max_volume,
                            win.config.lock_unmuted,
                        );
                        win.config.save();
                    }

                    IDC_CHK_AUTOSTART | IDM_AUTOSTART => {
                        win.config.start_with_windows = !win.config.start_with_windows;
                        set_autostart(win.config.start_with_windows);
                        let check = if win.config.start_with_windows { 1usize } else { 0usize };
                        SendMessageW(win.chk_autostart, BM_SETCHECK, WPARAM(check), LPARAM(0));
                        win.config.save();
                    }

                    IDC_CHK_MIN_TO_TRAY => {
                        win.config.minimize_to_tray = !win.config.minimize_to_tray;
                        let check = if win.config.minimize_to_tray { 1usize } else { 0usize };
                        SendMessageW(win.chk_min_to_tray, BM_SETCHECK, WPARAM(check), LPARAM(0));
                        win.config.save();
                    }

                    IDC_BTN_TOGGLE_THEME | IDM_TOGGLE_THEME => {
                        win.config.theme = if win.config.theme == AppTheme::Dark {
                            AppTheme::Light
                        } else {
                            AppTheme::Dark
                        };
                        win.apply_theme_to_window();
                        win.config.save();
                    }

                    IDC_BTN_HIDE_TO_TRAY => {
                        ShowWindow(hwnd, SW_HIDE);
                    }

                    IDM_OPEN_UI => {
                        ShowWindow(hwnd, SW_RESTORE);
                        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                    }

                    IDM_EXIT => {
                        win.tray.take().map(|t| t.remove_icon());
                        AudioManager::shutdown();
                        PostQuitMessage(0);
                    }

                    _ => {}
                }
            }
            LRESULT(0)
        }

        WM_TRAY_ICON => {
            let event = (lparam.0 & 0xFFFF) as u32;
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &mut *ptr;
                if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                    ShowWindow(hwnd, SW_RESTORE);
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                } else if event == WM_RBUTTONUP {
                    if let Some(tray) = win.tray.as_ref() {
                        tray.show_context_menu(
                            win.config.osd_hidden,
                            win.config.lock_enabled,
                            win.config.start_with_windows,
                            win.config.theme == AppTheme::Dark,
                        );
                    }
                }
            }
            LRESULT(0)
        }

        WM_CLOSE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let win = &*ptr;
                if win.config.minimize_to_tray {
                    ShowWindow(hwnd, SW_HIDE);
                    return LRESULT(0);
                }
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow;
            if !ptr.is_null() {
                let mut win = Box::from_raw(ptr);
                if let Some(tray) = win.tray.take() {
                    tray.remove_icon();
                }
                win.theme.dispose();
                DeleteObject(win.hfont_regular);
                DeleteObject(win.hfont_bold);
                DeleteObject(win.hfont_card_title);
            }
            AudioManager::shutdown();
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
