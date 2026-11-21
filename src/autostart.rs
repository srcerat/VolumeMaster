use std::env;
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

const SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: PCWSTR = w!("VolumeMaster");

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY(0);
        if RegOpenKeyExW(HKEY_CURRENT_USER, SUBKEY, 0, KEY_QUERY_VALUE, &mut hkey).is_err() {
            return false;
        }

        let mut data_len = 0u32;
        let query_res = RegQueryValueExW(
            hkey,
            APP_NAME,
            None,
            None,
            None,
            Some(&mut data_len),
        );

        let _ = RegCloseKey(hkey);
        query_res.is_ok() && data_len > 0
    }
}

pub fn set_autostart(enable: bool) -> bool {
    unsafe {
        let mut hkey = HKEY(0);
        if RegOpenKeyExW(HKEY_CURRENT_USER, SUBKEY, 0, KEY_SET_VALUE, &mut hkey).is_err() {
            return false;
        }

        let ret = if enable {
            if let Ok(exe_path) = env::current_exe() {
                let path_str = exe_path.to_string_lossy().to_string();
                let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes: &[u8] = std::slice::from_raw_parts(
                    wide_path.as_ptr() as *const u8,
                    wide_path.len() * 2,
                );
                RegSetValueExW(
                    hkey,
                    APP_NAME,
                    0,
                    REG_SZ,
                    Some(bytes),
                ).is_ok()
            } else {
                false
            }
        } else {
            let del_res = RegDeleteValueW(hkey, APP_NAME);
            del_res.is_ok() || del_res.err().map_or(false, |e| e.code().0 == 0x80070002u32 as i32)
        };

        let _ = RegCloseKey(hkey);
        ret
    }
}
