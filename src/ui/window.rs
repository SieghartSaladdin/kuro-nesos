//! Win32 helpers for the notch window: placement, click-through, cursor and focus.

use super::*;


pub fn open_path(p: &std::path::Path) {
    let wide: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(None, windows::core::w!("open"), windows::core::PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

/// Places the window for a dock: hanging from the top edge, or against the left/right edge,
/// centred at `offset` (fraction of the screen) along that edge. Returns the DPI scale.
pub fn place_window(hwnd: HWND, dock: Dock, offset: f32) -> f32 {
    unsafe {
        let scale = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let (ww, wh) = window_size(dock);
        let (w, h) = ((ww * scale).round() as i32, (wh * scale).round() as i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let (x, y) = match dock {
            Dock::Top => ((offset * sw as f32) as i32 - w / 2, 0),
            Dock::Left => (0, (offset * sh as f32) as i32 - h / 2),
            Dock::Right => (sw - w, (offset * sh as f32) as i32 - h / 2),
        };
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
        scale
    }
}

/// Lets clicks go through the window (it stays visible) or makes it take them again.
pub(super) fn set_click_through(hwnd: HWND, on: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let t = WS_EX_TRANSPARENT.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, if on { ex | t } else { ex & !t });
    }
}

pub(super) fn cursor_screen() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    p
}

/// (a mouse button is held, a button is held or was pressed since the last check)
pub(super) fn mouse_buttons() -> (bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON};
    let mut held = false;
    let mut pressed = false;
    for k in [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON] {
        let s = unsafe { GetAsyncKeyState(k.0 as i32) } as u16;
        held |= s & 0x8000 != 0;
        pressed |= s & 0x8001 != 0;
    }
    (held, pressed)
}

pub fn activate(hwnd: HWND, on: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if on {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
        }
    }
}
