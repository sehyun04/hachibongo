//! Windows 갈고리: WH_KEYBOARD_LL·WH_MOUSE_LL. 키 번호는 이미 가상 키 번호라 그대로 넘긴다.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::{btn, key, log_line, wheel, Pointer, Spot};
use std::sync::atomic::Ordering;

const BTN_VK: [i32; 3] = [0x01, 0x02, 0x04];

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => key(k.vkCode, true),
            WM_KEYUP | WM_SYSKEYUP => key(k.vkCode, false),
            _ => {}
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam as u32;
        if msg != WM_MOUSEMOVE {
            let m = &*(lparam as *const MSLLHOOKSTRUCT);
            match msg {
                WM_LBUTTONDOWN => btn(0, true),
                WM_LBUTTONUP => btn(0, false),
                WM_RBUTTONDOWN => btn(1, true),
                WM_RBUTTONUP => btn(1, false),
                WM_MBUTTONDOWN => btn(2, true),
                WM_MBUTTONUP => btn(2, false),
                WM_MOUSEWHEEL => wheel(((m.mouseData >> 16) as i16) as i32),
                _ => {}
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

pub(super) fn spawn_hooks() {
    std::thread::Builder::new()
        .name("input-hook".into())
        .spawn(|| unsafe {
            let hmod = GetModuleHandleW(std::ptr::null());
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), hmod, 0);
            let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0);
            log_line(&format!("hooks keyboard={} mouse={}\n", !kb.is_null(), !ms.is_null()));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("input-hook thread");
}

pub(super) fn key_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

pub(super) fn btn_down(b: u8) -> bool {
    unsafe { (GetAsyncKeyState(BTN_VK[b as usize]) as u16 & 0x8000) != 0 }
}

/// 가상 화면 전체 기준으로 정규화한다.
pub(super) fn pointer() -> Option<Pointer> {
    unsafe {
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) == 0 {
            return None;
        }
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN) as f64;
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN) as f64;
        let vw = (GetSystemMetrics(SM_CXVIRTUALSCREEN) as f64).max(1.0);
        let vh = (GetSystemMetrics(SM_CYVIRTUALSCREEN) as f64).max(1.0);
        Some(Pointer {
            nx: (p.x as f64 - vx) / vw,
            ny: (p.y as f64 - vy) / vh,
            x: p.x as f64,
            y: p.y as f64,
            key: (p.x as i64, p.y as i64),
        })
    }
}

/// 커서를 창 기준 논리 픽셀로. 창 자리는 그때그때 창에 묻는다(끄는 중에도 정확하다).
pub(super) fn local(p: &Pointer, spot: &Spot) -> Option<(f64, f64, (i64, i64))> {
    let hwnd = spot.hwnd.load(Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return None;
    }
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return None;
        }
        let dpi = GetDpiForWindow(hwnd);
        let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
        Some(((p.x - r.left as f64) / scale, (p.y - r.top as f64) / scale, (r.left as i64, r.top as i64)))
    }
}
