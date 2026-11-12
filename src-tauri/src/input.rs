//! 전역 키보드·마우스 입력을 웹뷰로 넘긴다.
//!
//! 키와 버튼은 저수준 훅(WH_*_LL)으로 받는다. 폴링(GetAsyncKeyState)은 한 프레임보다
//! 짧은 타건을 놓치고, 휠은 아예 폴링할 수 없다. 커서 위치만은 훅으로 받으면
//! 고주사율 마우스에서 초당 1000번씩 이벤트가 쏟아지므로 60Hz 로 직접 읽는다.
//!
//! 훅 콜백 안에서는 채널에 넣기만 한다. 저수준 훅이 느리면 시스템 전체 입력이
//! 굼떠지고, 시간 제한을 넘기면 Windows 가 말없이 훅을 떼 버린다.
//!
//! 어떤 키를 눌렀는지는 화면의 키 위치를 고르는 데만 쓰고 어디에도 남기지 않는다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Serialize, Clone, Copy)]
#[serde(tag = "t")]
enum Ev {
    #[serde(rename = "k")]
    Key { vk: u32, d: bool },
    #[serde(rename = "b")]
    Btn { b: u8, d: bool },
    #[serde(rename = "w")]
    Wheel { dy: i32 },
    #[serde(rename = "m")]
    Move { nx: f64, ny: f64, lx: f64, ly: f64 },
}

static TX: OnceLock<Sender<Ev>> = OnceLock::new();
// 키를 누르고 있으면 KEYDOWN 이 반복해서 온다. 상태가 바뀔 때만 넘긴다.
static KEYS: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];
static BTNS: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];
const BTN_VK: [i32; 3] = [0x01, 0x02, 0x04];

fn send(ev: Ev) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(ev);
    }
}

unsafe extern "system" fn kb_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        let vk = (k.vkCode & 0xff) as usize;
        let down = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let Some(d) = down {
            if KEYS[vk].swap(d, Ordering::Relaxed) != d {
                send(Ev::Key { vk: vk as u32, d });
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam as u32;
        if msg != WM_MOUSEMOVE {
            let m = &*(lparam as *const MSLLHOOKSTRUCT);
            let btn = |b: u8, d: bool| {
                if BTNS[b as usize].swap(d, Ordering::Relaxed) != d {
                    send(Ev::Btn { b, d });
                }
            };
            match msg {
                WM_LBUTTONDOWN => btn(0, true),
                WM_LBUTTONUP => btn(0, false),
                WM_RBUTTONDOWN => btn(1, true),
                WM_RBUTTONUP => btn(1, false),
                WM_MBUTTONDOWN => btn(2, true),
                WM_MBUTTONUP => btn(2, false),
                WM_MOUSEWHEEL => send(Ev::Wheel { dy: ((m.mouseData >> 16) as i16) as i32 }),
                _ => {}
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// 훅 스레드와 전달 스레드를 띄운다. `hwnd` 는 커서를 창 기준 좌표로 바꾸는 데 쓴다.
pub fn start(app: AppHandle, hwnd: isize) {
    let (tx, rx) = channel::<Ev>();
    let _ = TX.set(tx);

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

    std::thread::Builder::new()
        .name("input-forward".into())
        .spawn(move || forward(app, rx, hwnd as HWND))
        .expect("input-forward thread");
}

fn forward(app: AppHandle, rx: std::sync::mpsc::Receiver<Ev>, hwnd: HWND) {
    let frame = Duration::from_millis(16);
    let mut last_cursor: Option<(i32, i32, i32, i32)> = None;
    // 길게 눌린 키는 떼는 신호를 놓쳤을 수 있다(관리자 창으로 포커스 이동, Win+L 등).
    // 1.5초 넘게 눌린 것만 실제 키 상태와 대조한다. 방금 누른 키는 훅이 시스템 상태보다
    // 먼저 불리므로 대조하면 오히려 틀린다.
    let mut held: HashMap<u32, Instant> = HashMap::new();
    let mut held_btn: [Option<Instant>; 3] = [None; 3];
    let mut last_check = Instant::now();
    let log = std::env::var_os("HACHIBONGO_LOG").is_some();
    let mut counts = [0u32; 4];
    let mut last_log = Instant::now();

    loop {
        let deadline = Instant::now() + frame;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match rx.recv_timeout(deadline - now) {
                Ok(ev) => {
                    match ev {
                        Ev::Key { vk, d: true } => { held.insert(vk, Instant::now()); counts[0] += 1; }
                        Ev::Key { vk, d: false } => { held.remove(&vk); }
                        Ev::Btn { b, d } => { held_btn[b as usize] = d.then(Instant::now); if d { counts[1] += 1; } }
                        Ev::Wheel { .. } => counts[2] += 1,
                        Ev::Move { .. } => {}
                    }
                    let _ = app.emit_to("main", "input", ev);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        if let Some(ev) = cursor(hwnd, &mut last_cursor) {
            counts[3] += 1;
            let _ = app.emit_to("main", "input", ev);
        }

        if last_check.elapsed() > Duration::from_millis(250) {
            last_check = Instant::now();
            let stale = Duration::from_millis(1500);
            let stuck: Vec<u32> = held
                .iter()
                .filter(|(vk, at)| at.elapsed() > stale && !is_down(**vk as i32))
                .map(|(vk, _)| *vk)
                .collect();
            for vk in stuck {
                held.remove(&vk);
                KEYS[vk as usize].store(false, Ordering::Relaxed);
                let _ = app.emit_to("main", "input", Ev::Key { vk, d: false });
            }
            for b in 0..3 {
                if held_btn[b].is_some_and(|at| at.elapsed() > stale) && !is_down(BTN_VK[b]) {
                    held_btn[b] = None;
                    BTNS[b].store(false, Ordering::Relaxed);
                    let _ = app.emit_to("main", "input", Ev::Btn { b: b as u8, d: false });
                }
            }
        }

        // 훅이 살아 있는지 확인하는 손잡이. 무슨 키였는지는 적지 않고 개수만 남긴다.
        if log && last_log.elapsed() > Duration::from_secs(5) {
            last_log = Instant::now();
            log_line(&format!("keys={} clicks={} wheel={} moves={}\n", counts[0], counts[1], counts[2], counts[3]));
            counts = [0; 4];
        }
    }
}

fn log_line(line: &str) {
    use std::io::Write;
    let Some(path) = std::env::var_os("HACHIBONGO_LOG") else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn is_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

/// 마우스 패드 위치는 가상 화면 전체 기준으로 정규화한다. 모니터별로 자르면 화면 경계를
/// 넘을 때 발이 패드 반대편으로 순간이동한다.
fn cursor(hwnd: HWND, last: &mut Option<(i32, i32, i32, i32)>) -> Option<Ev> {
    unsafe {
        let mut p = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut p) == 0 {
            return None;
        }
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let key = (p.x, p.y, r.left, r.top);
        if *last == Some(key) {
            return None;
        }
        *last = Some(key);

        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN) as f64;
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN) as f64;
        let vw = (GetSystemMetrics(SM_CXVIRTUALSCREEN) as f64).max(1.0);
        let vh = (GetSystemMetrics(SM_CYVIRTUALSCREEN) as f64).max(1.0);
        let dpi = GetDpiForWindow(hwnd);
        let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
        Some(Ev::Move {
            nx: (p.x as f64 - vx) / vw,
            ny: (p.y as f64 - vy) / vh,
            lx: (p.x - r.left) as f64 / scale,
            ly: (p.y - r.top) as f64 / scale,
        })
    }
}
