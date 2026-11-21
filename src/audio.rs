use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;
use windows::core::{implement, Result};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, IMMDeviceEnumerator, MMDeviceEnumerator,
    eRender, eConsole,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::config::LockMode;

pub const WM_APP_VOLUME_CHANGED: u32 = 0x8000 + 101;

static INTERNAL_CHANGE: AtomicBool = AtomicBool::new(false);
static LOCK_ENABLED: AtomicBool = AtomicBool::new(true);
static LOCK_MODE_IS_EXACT: AtomicBool = AtomicBool::new(true);
static EXACT_VOLUME: AtomicU32 = AtomicU32::new(50);
static MIN_VOLUME: AtomicU32 = AtomicU32::new(20);
static MAX_VOLUME: AtomicU32 = AtomicU32::new(80);
static LOCK_UNMUTED: AtomicBool = AtomicBool::new(true);
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

struct SendWrapper<T>(pub T);
unsafe impl<T> Send for SendWrapper<T> {}
unsafe impl<T> Sync for SendWrapper<T> {}

static GLOBAL_ENDPOINT: Mutex<Option<SendWrapper<IAudioEndpointVolume>>> = Mutex::new(None);
static GLOBAL_CALLBACK: Mutex<Option<SendWrapper<IAudioEndpointVolumeCallback>>> = Mutex::new(None);

#[implement(IAudioEndpointVolumeCallback)]
struct AudioCallback;

impl IAudioEndpointVolumeCallback_Impl for AudioCallback {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        if let Some(notify) = unsafe { pnotify.as_ref() } {
            let current_percent = (notify.fMasterVolume * 100.0).round() as u32;
            let is_muted = notify.bMuted.as_bool();

            // Notify UI
            let hwnd = HWND(MAIN_HWND.load(Ordering::SeqCst));
            if hwnd.0 != 0 {
                unsafe {
                    let _ = PostMessageW(
                        hwnd,
                        WM_APP_VOLUME_CHANGED,
                        WPARAM(current_percent as usize),
                        LPARAM(if is_muted { 1 } else { 0 }),
                    );
                }
            }

            // Check if this was an internal change by our UI
            if INTERNAL_CHANGE.load(Ordering::SeqCst) {
                return Ok(());
            }

            // External change detected! Check if locking is enabled:
            if LOCK_ENABLED.load(Ordering::SeqCst) {
                if let Ok(guard) = GLOBAL_ENDPOINT.lock() {
                    if let Some(wrapper) = guard.as_ref() {
                        let endpoint = &wrapper.0;
                        if LOCK_MODE_IS_EXACT.load(Ordering::SeqCst) {
                            let target = EXACT_VOLUME.load(Ordering::SeqCst);
                            if current_percent != target {
                                let target_scalar = (target as f32) / 100.0;
                                unsafe {
                                    let _ = endpoint.SetMasterVolumeLevelScalar(target_scalar, std::ptr::null());
                                }
                            }
                        } else {
                            let min = MIN_VOLUME.load(Ordering::SeqCst);
                            let max = MAX_VOLUME.load(Ordering::SeqCst);
                            if current_percent < min {
                                let min_scalar = (min as f32) / 100.0;
                                unsafe {
                                    let _ = endpoint.SetMasterVolumeLevelScalar(min_scalar, std::ptr::null());
                                }
                            } else if current_percent > max {
                                let max_scalar = (max as f32) / 100.0;
                                unsafe {
                                    let _ = endpoint.SetMasterVolumeLevelScalar(max_scalar, std::ptr::null());
                                }
                            }
                        }

                        // Check mute lock (e.g. keep unmuted if external mute button messes with volume)
                        if LOCK_UNMUTED.load(Ordering::SeqCst) && is_muted {
                            unsafe {
                                let _ = endpoint.SetMute(false, std::ptr::null());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct AudioManager;

impl AudioManager {
    pub fn init(hwnd: HWND) -> Result<()> {
        MAIN_HWND.store(hwnd.0, Ordering::SeqCst);
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;

            let cb: IAudioEndpointVolumeCallback = AudioCallback.into();
            endpoint.RegisterControlChangeNotify(&cb)?;

            *GLOBAL_ENDPOINT.lock().unwrap() = Some(SendWrapper(endpoint));
            *GLOBAL_CALLBACK.lock().unwrap() = Some(SendWrapper(cb));
        }
        Ok(())
    }

    pub fn shutdown() {
        let mut endpoint_guard = GLOBAL_ENDPOINT.lock().unwrap();
        let mut cb_guard = GLOBAL_CALLBACK.lock().unwrap();

        if let (Some(wrapper_endpoint), Some(wrapper_cb)) = (endpoint_guard.take(), cb_guard.take()) {
            unsafe {
                let _ = wrapper_endpoint.0.UnregisterControlChangeNotify(&wrapper_cb.0);
            }
        }
    }

    pub fn get_volume() -> u32 {
        if let Ok(guard) = GLOBAL_ENDPOINT.lock() {
            if let Some(wrapper) = guard.as_ref() {
                if let Ok(vol) = unsafe { wrapper.0.GetMasterVolumeLevelScalar() } {
                    return (vol * 100.0).round() as u32;
                }
            }
        }
        50
    }

    pub fn is_muted() -> bool {
        if let Ok(guard) = GLOBAL_ENDPOINT.lock() {
            if let Some(wrapper) = guard.as_ref() {
                if let Ok(m) = unsafe { wrapper.0.GetMute() } {
                    return m.as_bool();
                }
            }
        }
        false
    }

    pub fn set_volume(percent: u32) {
        let percent = percent.clamp(0, 100);
        let scalar = (percent as f32) / 100.0;
        INTERNAL_CHANGE.store(true, Ordering::SeqCst);
        if let Ok(guard) = GLOBAL_ENDPOINT.lock() {
            if let Some(wrapper) = guard.as_ref() {
                unsafe {
                    let _ = wrapper.0.SetMasterVolumeLevelScalar(scalar, std::ptr::null());
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        INTERNAL_CHANGE.store(false, Ordering::SeqCst);
    }

    pub fn set_mute(muted: bool) {
        INTERNAL_CHANGE.store(true, Ordering::SeqCst);
        if let Ok(guard) = GLOBAL_ENDPOINT.lock() {
            if let Some(wrapper) = guard.as_ref() {
                unsafe {
                    let _ = wrapper.0.SetMute(muted, std::ptr::null());
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        INTERNAL_CHANGE.store(false, Ordering::SeqCst);
    }

    pub fn update_settings(
        lock_enabled: bool,
        lock_mode: LockMode,
        exact: u32,
        min: u32,
        max: u32,
        lock_unmuted: bool,
    ) {
        LOCK_ENABLED.store(lock_enabled, Ordering::SeqCst);
        LOCK_MODE_IS_EXACT.store(lock_mode == LockMode::Exact, Ordering::SeqCst);
        EXACT_VOLUME.store(exact.clamp(0, 100), Ordering::SeqCst);
        MIN_VOLUME.store(min.clamp(0, 100), Ordering::SeqCst);
        MAX_VOLUME.store(max.clamp(0, 100), Ordering::SeqCst);
        LOCK_UNMUTED.store(lock_unmuted, Ordering::SeqCst);

        // If lock enabled, enforce it immediately
        if lock_enabled {
            let current = Self::get_volume();
            if lock_mode == LockMode::Exact {
                if current != exact {
                    Self::set_volume(exact);
                }
            } else {
                if current < min {
                    Self::set_volume(min);
                } else if current > max {
                    Self::set_volume(max);
                }
            }
            if lock_unmuted && Self::is_muted() {
                Self::set_mute(false);
            }
        }
    }
}
