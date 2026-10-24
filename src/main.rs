#![windows_subsystem = "windows"]
//! Kuro Nesos - "A living notch for Windows."
//!
//! Pages: Music (with LRCLIB lyrics), Pomodoro, Water reminder, File shelf, To-do list.
//! Plus system alerts (Caps Lock, battery, network, Bluetooth, keyboard layout, Windows
//! notifications) and the idle cat mascot.
//!
//! Test flags: --demo alerts|lyrics|mascot|all, --pin (stay expanded), --page N,
//!             --test-lyrics "Title|Artist" (writes the LRCLIB result to log.txt and exits),
//!             --test-media (logs the current media session and album-art size, then exits),
//!             --test-roman "line|line" (logs the romanization of each line, then exits).

mod gfx;
mod json;
mod lyrics;
mod media;
mod model;
mod roman;
mod shelf;
mod sys;
mod ui;

use std::cell::RefCell;
use std::sync::mpsc::channel;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{IDropTarget, OleInitialize, RegisterDragDrop, RevokeDragDrop};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ui::{App, Demo, Effect};

/// Tray callback message and menu command IDs.
const WM_TRAY: u32 = WM_APP + 9;
const ID_EXIT: u32 = 1;
const ID_AUTOSTART: u32 = 2;
const ID_MASCOT: u32 = 3;
const ID_ABOUT: u32 = 4;
const ID_MINIMAL: u32 = 5;
const ID_ROMAJI: u32 = 6;
const ID_RESET_POS: u32 = 7;
const TIMER_ID: usize = 1;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => None,
    })
}

/// Side effects that must run without the App borrowed (modal loops, focus changes).
fn run_effect(hwnd: HWND) {
    match with_app(|a| a.effect.take()).flatten() {
        Some(Effect::DragOut(p)) => shelf::drag_out(&p),
        Some(Effect::Activate) => ui::activate(hwnd, true),
        Some(Effect::Deactivate) => ui::activate(hwnd, false),
        None => {}
    }
}

fn lparam_xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xffff) as i16 as i32, ((lp.0 >> 16) & 0xffff) as i16 as i32)
}

/// The logo icon embedded by build.rs (resource 1), at the system small-icon size.
fn app_icon() -> HICON {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return HICON::default() };
        let (cx, cy) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
        match LoadImageW(module, PCWSTR(1 as *const u16), IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR) {
            Ok(h) => HICON(h.0),
            Err(_) => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        }
    }
}

fn tray(hwnd: HWND, add: bool) {
    unsafe {
        let mut nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        if add {
            nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            nid.uCallbackMessage = WM_TRAY;
            nid.hIcon = app_icon();
            let tip: Vec<u16> = "Kuro Nesos".encode_utf16().collect();
            nid.szTip[..tip.len()].copy_from_slice(&tip);
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        } else {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

fn tray_menu(hwnd: HWND) {
    let Some(s) = with_app(|a| a.settings()) else { return };
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check(model::autostart_enabled()), ID_AUTOSTART as usize, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_STRING | check(s.pet), ID_MASCOT as usize, w!("Mascot"));
        let _ = AppendMenuW(menu, MF_STRING | check(s.minimal), ID_MINIMAL as usize, w!("Minimal notch"));
        let _ = AppendMenuW(menu, MF_STRING | check(s.romaji), ID_ROMAJI as usize, w!("Romaji lyrics"));
        // the notch is moved by dragging its header; this brings it back to the top centre
        let moved = s.dock != model::Dock::Top || (s.offset - 0.5).abs() > 0.001;
        let _ = AppendMenuW(menu, MF_STRING | if moved { MF_ENABLED } else { MF_GRAYED }, ID_RESET_POS as usize, w!("Reset position"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_ABOUT as usize, w!("About Kuro Nesos"));
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
        match cmd.0 as u32 {
            ID_AUTOSTART => model::set_autostart(!model::autostart_enabled()),
            ID_MASCOT => {
                with_app(|a| a.toggle_pet());
            }
            ID_MINIMAL => {
                with_app(|a| a.toggle_minimal());
            }
            ID_ROMAJI => {
                with_app(|a| a.toggle_romaji());
            }
            ID_RESET_POS => {
                with_app(|a| a.reset_position());
            }
            ID_ABOUT => {
                MessageBoxW(
                    hwnd,
                    w!("Kuro Nesos 1.1.0\n\nA living notch for Windows.\nMusic, lyrics, focus timer, water reminder, to-do and file shelf.\n\nLyrics provided by LRCLIB (lrclib.net).\nhttps://github.com/SieghartSaladdin/kuro-nesos"),
                    w!("About Kuro Nesos"),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            ID_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

/// Places the (fixed-size, mostly transparent) window where the settings say.
fn position(hwnd: HWND) -> f32 {
    let s = with_app(|a| a.settings()).unwrap_or_else(model::Settings::load);
    ui::place_window(hwnd, s.dock, s.offset)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => {
                // backup tick: keeps the notch alive inside modal loops (tray menu, drag & drop)
                with_app(|a| a.tick());
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let (x, y) = lparam_xy(lp);
                with_app(|a| a.mouse_move(x, y));
                run_effect(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = lparam_xy(lp);
                SetCapture(hwnd);
                with_app(|a| a.mouse_down(x, y));
                run_effect(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let (x, y) = lparam_xy(lp);
                let _ = ReleaseCapture();
                with_app(|a| a.mouse_up(x, y));
                run_effect(hwnd);
                LRESULT(0)
            }
            WM_SETCURSOR if with_app(|a| a.grab_cursor()).unwrap_or(false) => {
                if let Ok(c) = LoadCursorW(None, IDC_SIZEALL) {
                    SetCursor(c);
                }
                LRESULT(1)
            }
            WM_MOUSEWHEEL => {
                let delta = ((wp.0 >> 16) & 0xffff) as i16;
                with_app(|a| a.wheel(delta));
                LRESULT(0)
            }
            WM_CHAR => {
                with_app(|a| a.key_char(wp.0 as u32));
                run_effect(hwnd);
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                with_app(|a| a.focus_lost());
                run_effect(hwnd);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if ex & WS_EX_NOACTIVATE.0 as isize != 0 {
                    LRESULT(MA_NOACTIVATE as isize)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            shelf::WM_SHELF_DRAG => {
                with_app(|a| a.shelf_drag(wp.0 != 0));
                LRESULT(0)
            }
            shelf::WM_SHELF_DROP => {
                with_app(|a| a.shelf_dropped());
                LRESULT(0)
            }
            WM_TRAY => {
                let event = (lp.0 & 0xffff) as u32;
                if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_CONTEXTMENU {
                    tray_menu(hwnd);
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                position(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                tray(hwnd, false);
                let _ = RevokeDragDrop(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => {
                if msg == RegisterWindowMessageW(w!("TaskbarCreated")) {
                    tray(hwnd, true);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
    }
}

fn parse_args() -> (Demo, Option<String>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut demo = Demo::default();
    let mut test_lyrics = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {
                let what = args.get(i + 1).map(String::as_str).unwrap_or("all");
                i += 1;
                demo.alerts |= matches!(what, "alerts" | "all");
                demo.lyrics |= matches!(what, "lyrics" | "all");
                demo.mascot |= matches!(what, "mascot" | "all");
            }
            "--pin" => demo.pin = true,
            "--page" => {
                demo.page = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 1;
            }
            "--test-media" => test_lyrics = Some("\u{1}media".into()),
            "--test-roman" => {
                test_lyrics = args.get(i + 1).map(|t| format!("\u{2}{}", t));
                i += 1;
            }
            "--test-lyrics" => {
                test_lyrics = args.get(i + 1).cloned();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    (demo, test_lyrics)
}

fn main() {
    let (demo, test_lyrics) = parse_args();
    unsafe {
        if let Some(q) = test_lyrics {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
            if let Some(text) = q.strip_prefix('\u{2}') {
                let mut lines: Vec<lyrics::Line> = text.split('|').map(|t| lyrics::Line { t: 0.0, text: t.to_string(), roman: None }).collect();
                lyrics::romanize(&mut lines);
                for l in &lines {
                    sys::log(&format!("test-roman: {} => {}", l.text, l.roman.as_deref().unwrap_or("(unchanged)")));
                }
                return;
            }
            if q == "\u{1}media" {
                sys::log(&format!("test-media: {}", media::describe_current()));
                return;
            }
            let (title, artist) = q.split_once('|').unwrap_or((q.as_str(), ""));
            match lyrics::fetch(title, artist, 0.0) {
                Some(lines) => {
                    sys::log(&format!("test-lyrics '{}' by '{}': {} lines", title, artist, lines.len()));
                    for l in lines.iter().take(6) {
                        sys::log(&format!("  [{:06.2}] {}", l.t, l.text));
                    }
                }
                None => sys::log(&format!("test-lyrics '{}' by '{}': not found", title, artist)),
            }
            return;
        }

        let mutex = CreateMutexW(None, true, w!("Local\\KuroNesos.SingleInstance"));
        if mutex.is_err() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let _ = OleInitialize(None);
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let class = w!("KuroNesosNotch");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            // layered + transparent: clicks go through the closed notch (see ui::set_click_through)
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            class,
            w!("Notch"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            hinst,
            None,
        ) else {
            return;
        };
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        let scale = position(hwnd);
        let (ww, wh) = ui::window_size(model::Settings::load().dock);
        let (pw, ph) = ((ww * scale).round() as u32, (wh * scale).round() as u32);
        let gfx = match gfx::Gfx::new(hwnd, pw, ph, scale) {
            Ok(g) => g,
            Err(e) => {
                sys::log(&format!("graphics init failed: {e:?}"));
                let msg: Vec<u16> = format!("Kuro Nesos could not start Direct2D:\n{e:?}").encode_utf16().chain(std::iter::once(0)).collect();
                MessageBoxW(None, PCWSTR(msg.as_ptr()), w!("Kuro Nesos"), MB_OK | MB_ICONERROR);
                return;
            }
        };

        let (tx, rx) = channel();
        sys::spawn_watchers(tx);
        let app = App::new(hwnd, gfx, (pw, ph), scale, rx, demo);
        APP.with(|a| *a.borrow_mut() = Some(app));

        let target: IDropTarget = shelf::DropTarget { hwnd }.into();
        let _ = RegisterDragDrop(hwnd, &target);
        tray(hwnd, true);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_ID, 250, None);

        // Frame loop: while something animates, frames are paced by the compositor
        // (DwmFlush returns once per display refresh, so 120 Hz screens get 120 fps);
        // otherwise the thread sleeps until input arrives or the next slow tick is due.
        let mut msg = MSG::default();
        'run: loop {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'run;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let interval = with_app(|a| {
                a.tick();
                a.interval
            })
            .unwrap_or(100);
            if interval <= 16 {
                if DwmFlush().is_err() {
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
            } else {
                MsgWaitForMultipleObjects(None, false, interval, QS_ALLINPUT);
            }
        }
        drop(target);
    }
}
