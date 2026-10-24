//! System watchers and helpers: network, Bluetooth, Windows notifications, keyboard
//! layout, fullscreen detection, battery, synthesized chime and the tray icon.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
use windows::UI::Notifications::{KnownNotificationBindings, NotificationKinds};
use windows::Win32::Foundation::*;
use windows::Win32::Globalization::LCIDToLocaleName;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::*;

pub mod glyph {
    pub const CHEV_L: u16 = 0xE76B;
    pub const CHEV_R: u16 = 0xE76C;
    pub const REFRESH: u16 = 0xE72C;
    pub const SLIDERS: u16 = 0xE9E9;
    pub const DELETE: u16 = 0xE74D;
    pub const ADD: u16 = 0xE710;
    pub const REMOVE: u16 = 0xE738;
    pub const CLOCK: u16 = 0xE823;
    pub const BELL: u16 = 0xEA8F;
    pub const EDIT: u16 = 0xE70F;
    pub const CHECKLIST: u16 = 0xE9D5;
    pub const DOWNLOAD: u16 = 0xE896;
    pub const CAFE: u16 = 0xEC32;
    pub const STOPWATCH: u16 = 0xE916;
    pub const MESSAGE: u16 = 0xE8BD;
    pub const MUSIC: u16 = 0xE8D6;
    pub const BLUETOOTH: u16 = 0xE702;
    pub const WIFI: u16 = 0xE701;
    pub const ETHERNET: u16 = 0xE839;
    pub const KEYBOARD: u16 = 0xE765;
    pub const ADD_CIRCLE: u16 = 0xECC8;
    pub const TARGET: u16 = 0xF272;
    pub const DROP: u16 = 0xEB42;
    pub const DOC: u16 = 0xE8A5;
    pub const FOLDER: u16 = 0xE8B7;
    pub const LINK: u16 = 0xE71B;
    pub const PICTURE: u16 = 0xEB9F;
    pub const CHECK: u16 = 0xE73E;
}

pub fn log(msg: &str) {
    let path = crate::model::data_dir("LOCALAPPDATA").join("log.txt");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let n = crate::model::now();
        let _ = writeln!(f, "{} {}:{:02} {}", n.day, crate::model::hhmm(n.minutes), n.second, msg);
    }
}

/// An alert produced by a background watcher.
#[derive(Clone, Debug)]
pub struct SysAlert {
    pub icon: u16,
    pub title: String,
    pub value: String,
    pub color: u32,
}

fn alert(icon: u16, title: &str, value: &str, color: u32) -> SysAlert {
    SysAlert { icon, title: title.into(), value: value.into(), color }
}

// ------------------------------------------------------------------ background watcher

pub fn spawn_watchers(tx: Sender<SysAlert>) {
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut net_prev: Option<(u8, String)> = None;
        let mut bt_prev: Option<HashMap<String, String>> = None;
        let mut notif = NotificationWatch::new();
        let mut tick: u64 = 0;
        loop {
            if tick % 3 == 0 {
                let cur = network_state();
                if let Some(prev) = &net_prev {
                    if *prev != cur {
                        let _ = tx.send(network_alert(&cur));
                    }
                }
                net_prev = Some(cur);
            }
            if tick % 6 == 1 {
                if let Some(cur) = bluetooth_connected() {
                    if let Some(prev) = &bt_prev {
                        for (id, name) in &cur {
                            if !prev.contains_key(id) {
                                let _ = tx.send(alert(glyph::BLUETOOTH, "Connected", name, 0x4DB6FF));
                            }
                        }
                        for (id, name) in prev {
                            if !cur.contains_key(id) {
                                let _ = tx.send(alert(glyph::BLUETOOTH, "Disconnected", name, 0x8A8A8A));
                            }
                        }
                    }
                    bt_prev = Some(cur);
                }
            }
            if tick % 2 == 0 {
                for a in notif.poll() {
                    let _ = tx.send(a);
                }
            }
            tick += 1;
            std::thread::sleep(Duration::from_millis(1000));
        }
    });
}

/// (0 = disconnected, 1 = no internet, 2 = connected, name)
fn network_state() -> (u8, String) {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else { return (0, String::new()) };
    let level = profile.GetNetworkConnectivityLevel().unwrap_or(NetworkConnectivityLevel::None);
    let wlan = profile.IsWlanConnectionProfile().unwrap_or(false);
    let name = if wlan {
        profile
            .WlanConnectionProfileDetails()
            .and_then(|d| d.GetConnectedSsid())
            .map(|s| s.to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Wi-Fi".into())
    } else {
        "Ethernet".into()
    };
    let state = if level == NetworkConnectivityLevel::InternetAccess {
        2
    } else if level == NetworkConnectivityLevel::None {
        0
    } else {
        1
    };
    (state, name)
}

fn network_alert(s: &(u8, String)) -> SysAlert {
    let icon = if s.1 == "Ethernet" { glyph::ETHERNET } else { glyph::WIFI };
    match s.0 {
        2 => alert(icon, "Connected", &s.1, 0x40B854),
        1 => alert(icon, "No internet access", &s.1, 0xFF9F0A),
        _ => alert(glyph::WIFI, "Disconnected", "", 0xFF4F4F),
    }
}

fn bluetooth_connected() -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    let selectors = [
        BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected).ok(),
        BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected).ok(),
    ];
    for sel in selectors.into_iter().flatten() {
        let list = DeviceInformation::FindAllAsyncAqsFilter(&sel).ok()?.get().ok()?;
        for d in list {
            let id = d.Id().map(|s| s.to_string()).unwrap_or_default();
            let name = d.Name().map(|s| s.to_string()).unwrap_or_default();
            if !name.is_empty() {
                out.insert(id, name);
            }
        }
    }
    Some(out)
}

/// Mirrors new Windows toast notifications (needs the user's "notification access" consent).
struct NotificationWatch {
    listener: Option<UserNotificationListener>,
    seen: HashSet<u32>,
    primed: bool,
    tried: bool,
}

impl NotificationWatch {
    fn new() -> Self {
        NotificationWatch { listener: None, seen: HashSet::new(), primed: false, tried: false }
    }

    fn init(&mut self) {
        self.tried = true;
        let Ok(l) = UserNotificationListener::Current() else {
            log("notifications: listener unavailable");
            return;
        };
        let mut status = l.GetAccessStatus().unwrap_or(UserNotificationListenerAccessStatus::Unspecified);
        if status == UserNotificationListenerAccessStatus::Unspecified {
            status = l.RequestAccessAsync().and_then(|op| op.get()).unwrap_or(UserNotificationListenerAccessStatus::Denied);
        }
        log(&format!("notifications: access status {:?}", status.0));
        if status == UserNotificationListenerAccessStatus::Allowed {
            self.listener = Some(l);
        }
    }

    fn poll(&mut self) -> Vec<SysAlert> {
        if !self.tried {
            self.init();
        }
        let mut out = Vec::new();
        let Some(l) = &self.listener else { return out };
        let Ok(list) = l.GetNotificationsAsync(NotificationKinds::Toast).and_then(|op| op.get()) else { return out };
        for n in list {
            let Ok(id) = n.Id() else { continue };
            if !self.seen.insert(id) || !self.primed {
                continue;
            }
            let app = n.AppInfo().and_then(|a| a.DisplayInfo()).and_then(|d| d.DisplayName()).map(|s| s.to_string()).unwrap_or_default();
            let mut texts = Vec::new();
            if let Ok(binding) = n.Notification().and_then(|x| x.Visual()).and_then(|v| v.GetBinding(&KnownNotificationBindings::ToastGeneric().unwrap_or_default())) {
                if let Ok(elems) = binding.GetTextElements() {
                    for e in elems {
                        if let Ok(t) = e.Text() {
                            texts.push(t.to_string());
                        }
                    }
                }
            }
            let body = texts.join(": ");
            out.push(alert(glyph::MESSAGE, if app.is_empty() { "Notification" } else { &app }, &body, 0xFFFFFF));
        }
        self.primed = true;
        out
    }
}

// ------------------------------------------------------------------ polled from the UI thread

pub struct Battery {
    pub plugged: bool,
    pub percent: u8,
    pub present: bool,
}

pub fn battery() -> Battery {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return Battery { plugged: true, percent: 100, present: false };
    }
    Battery { plugged: s.ACLineStatus == 1, percent: if s.BatteryLifePercent <= 100 { s.BatteryLifePercent } else { 100 }, present: s.BatteryFlag & 128 == 0 }
}

/// Keyboard layout of the foreground window as a short name, e.g. "EN-US".
pub fn keyboard_layout() -> String {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let hkl = GetKeyboardLayout(tid);
        let lang = (hkl.0 as usize & 0xffff) as u32;
        let mut buf = [0u16; 85];
        let n = LCIDToLocaleName(lang, Some(&mut buf), 0);
        if n > 1 {
            String::from_utf16_lossy(&buf[..n as usize - 1]).to_uppercase()
        } else {
            format!("{:04X}", lang)
        }
    }
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Hands unused pages back to Windows (SetProcessWorkingSetSize). Pages that are needed
/// again fault back in cheaply.
pub fn trim_memory() {
    unsafe {
        let _ = windows::Win32::System::Threading::SetProcessWorkingSetSize(windows::Win32::System::Threading::GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Class name of the foreground window (for the log).
pub fn foreground_class() -> String {
    class_name(unsafe { GetForegroundWindow() })
}

/// True when the foreground window covers its whole monitor (games, videos, slideshows).
/// The desktop and taskbar ("Progman", "WorkerW", "Shell_TrayWnd") never count.
pub fn foreground_fullscreen(own: HWND) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg == own {
            return false;
        }
        let cls = class_name(fg);
        if matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return false;
        }
        // a maximized window (even one covering the monitor because the taskbar auto-hides)
        // or anything with a caption bar is a normal window, not a fullscreen app
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        if IsZoomed(fg).as_bool() || style & WS_CAPTION.0 == WS_CAPTION.0 {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

// ------------------------------------------------------------------ sound

fn chime_wav() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| {
        let rate = 22050u32;
        let notes = [(1318.5f32, 0.0f32), (1760.0, 0.11)];
        let len = (rate as f32 * 0.55) as usize;
        let mut samples = vec![0f32; len];
        for (freq, start) in notes {
            let s0 = (start * rate as f32) as usize;
            for i in s0..len {
                let t = (i - s0) as f32 / rate as f32;
                let env = (-t * 7.0).exp() * (t * 400.0).min(1.0);
                samples[i] += (t * freq * std::f32::consts::TAU).sin() * env * 0.22 + (t * freq * 2.0 * std::f32::consts::TAU).sin() * env * 0.05;
            }
        }
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::with_capacity(44 + data.len());
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        w
    })
}

pub fn chime() {
    let wav = chime_wav();
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
