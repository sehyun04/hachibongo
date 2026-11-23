//! macOS 갈고리: 듣기만 하는 CGEventTap. 키를 바꾸거나 막지 않는다.
//!
//! 키보드를 들으려면 '입력 모니터링' 권한이 있어야 한다. 처음 켜면 한 번 묻고(시스템 설정의 개인정보 보호 창),
//! 허락될 때까지 2초마다 다시 본다 — 허락한 뒤 앱을 다시 켜지 않아도 붙는다. 그동안에도 커서 위치는 읽힌다.
//! 맥 키 번호(kVK_*)는 Windows 가상 키 번호로 바꿔 넘긴다. 화면 키 배열이 그 번호로 짜여 있다.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU16, Ordering};
use std::time::{Duration, Instant};

use super::{btn, key, log_line, wheel, Pointer, Spot};

type Ref = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type TapCallback = unsafe extern "C" fn(proxy: Ref, ty: u32, event: Ref, info: Ref) -> Ref;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, info: Ref) -> Ref;
    fn CGEventTapEnable(tap: Ref, enable: bool);
    fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    fn CGEventGetFlags(event: Ref) -> u64;
    fn CGEventCreate(source: Ref) -> Ref;
    fn CGEventGetLocation(event: Ref) -> CGPoint;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: Ref;
    static kCFBooleanTrue: Ref;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFMachPortCreateRunLoopSource(alloc: Ref, port: Ref, order: isize) -> Ref;
    fn CFRunLoopGetCurrent() -> Ref;
    fn CFRunLoopAddSource(rl: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRun();
    fn CFRelease(cf: Ref);
    fn CFDictionaryCreate(
        alloc: Ref,
        keys: *const Ref,
        values: *const Ref,
        n: isize,
        key_cb: *const c_void,
        value_cb: *const c_void,
    ) -> Ref;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: Ref;
    fn AXIsProcessTrustedWithOptions(options: Ref) -> bool;
}

// CGEventTapLocation·Placement·Options
const SESSION_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const LISTEN_ONLY: u32 = 1;

// CGEventType
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const SCROLL: u32 = 22;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const TAP_OFF_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_OFF_USER: u32 = 0xFFFF_FFFF;

// CGEventField
const F_BUTTON: u32 = 3;
const F_KEYCODE: u32 = 9;
const F_SCROLL_Y: u32 = 11;

const COMBINED_STATE: i32 = 0;

static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
/// 가상 키 번호마다 마지막으로 눌린 맥 키 번호. 눌린 채 박힌 키를 실제 상태와 대조할 때 되돌려 쓴다.
static CODE_OF: [AtomicU16; 256] = [const { AtomicU16::new(u16::MAX) }; 256];

const fn mask(types: &[u32]) -> u64 {
    let mut m = 0u64;
    let mut i = 0;
    while i < types.len() {
        m |= 1 << types[i];
        i += 1;
    }
    m
}

const MASK: u64 = mask(&[KEY_DOWN, KEY_UP, FLAGS_CHANGED, LEFT_DOWN, LEFT_UP, RIGHT_DOWN, RIGHT_UP, OTHER_DOWN, OTHER_UP, SCROLL]);

/// 맥 키 번호(Carbon kVK_*) -> Windows 가상 키 번호. 맥 배열의 같은 자리에 있는 키로 고른다.
fn vk_of(code: u16) -> Option<u32> {
    Some(match code {
        0x00 => 0x41, // A
        0x01 => 0x53, // S
        0x02 => 0x44, // D
        0x03 => 0x46, // F
        0x04 => 0x48, // H
        0x05 => 0x47, // G
        0x06 => 0x5A, // Z
        0x07 => 0x58, // X
        0x08 => 0x43, // C
        0x09 => 0x56, // V
        0x0A => 0xC0, // ISO 배열 왼쪽 위 키
        0x0B => 0x42, // B
        0x0C => 0x51, // Q
        0x0D => 0x57, // W
        0x0E => 0x45, // E
        0x0F => 0x52, // R
        0x10 => 0x59, // Y
        0x11 => 0x54, // T
        0x12 => 0x31,
        0x13 => 0x32,
        0x14 => 0x33,
        0x15 => 0x34,
        0x16 => 0x36,
        0x17 => 0x35,
        0x18 => 0xBB, // =
        0x19 => 0x39,
        0x1A => 0x37,
        0x1B => 0xBD, // -
        0x1C => 0x38,
        0x1D => 0x30,
        0x1E => 0xDD, // ]
        0x1F => 0x4F, // O
        0x20 => 0x55, // U
        0x21 => 0xDB, // [
        0x22 => 0x49, // I
        0x23 => 0x50, // P
        0x24 => 0x0D, // Return
        0x25 => 0x4C, // L
        0x26 => 0x4A, // J
        0x27 => 0xDE, // '
        0x28 => 0x4B, // K
        0x29 => 0xBA, // ;
        0x2A => 0xDC, // \
        0x2B => 0xBC, // ,
        0x2C => 0xBF, // /
        0x2D => 0x4E, // N
        0x2E => 0x4D, // M
        0x2F => 0xBE, // .
        0x30 => 0x09, // Tab
        0x31 => 0x20, // Space
        0x32 => 0xC0, // `
        0x33 => 0x08, // Delete(백스페이스 자리)
        0x35 => 0x1B, // Esc
        0x36 => 0x5C, // 오른쪽 Command
        0x37 => 0x5B, // Command
        0x38 => 0xA0, // Shift
        0x39 => 0x14, // Caps Lock(한글 배열에서는 한/영)
        0x3A => 0xA4, // Option
        0x3B => 0xA2, // Control
        0x3C => 0xA1, // 오른쪽 Shift
        0x3D => 0xA5, // 오른쪽 Option
        0x3E => 0xA3, // 오른쪽 Control
        0x41 => 0x6E, // 키패드 .
        0x43 => 0x6A, // 키패드 *
        0x45 => 0x6B, // 키패드 +
        0x47 => 0x90, // 키패드 Clear(Num Lock 자리)
        0x4B => 0x6F, // 키패드 /
        0x4C => 0x0D, // 키패드 Enter
        0x4E => 0x6D, // 키패드 -
        0x51 => 0xBB, // 키패드 =
        0x52 => 0x60,
        0x53 => 0x61,
        0x54 => 0x62,
        0x55 => 0x63,
        0x56 => 0x64,
        0x57 => 0x65,
        0x58 => 0x66,
        0x59 => 0x67,
        0x5B => 0x68,
        0x5C => 0x69,
        0x5D => 0xDC, // JIS ¥
        0x5E => 0xBF, // JIS _
        0x60 => 0x74, // F5
        0x61 => 0x75, // F6
        0x62 => 0x76, // F7
        0x63 => 0x72, // F3
        0x64 => 0x77, // F8
        0x65 => 0x78, // F9
        0x66 => 0x15, // JIS 英数(한/영 자리)
        0x67 => 0x7A, // F11
        0x68 => 0x19, // JIS かな(한자 자리)
        0x6D => 0x79, // F10
        0x6F => 0x7B, // F12
        0x72 => 0x2D, // Help(Insert 자리)
        0x73 => 0x24, // Home
        0x74 => 0x21, // Page Up
        0x75 => 0x2E, // 앞으로 지우기
        0x76 => 0x73, // F4
        0x77 => 0x23, // End
        0x78 => 0x71, // F2
        0x79 => 0x22, // Page Down
        0x7A => 0x70, // F1
        0x7B => 0x25, // ←
        0x7C => 0x27, // →
        0x7D => 0x28, // ↓
        0x7E => 0x26, // ↑
        _ => return None,
    })
}

/// 보조 키는 누름·뗌이 따로 오지 않고 '보조 키 상태가 바뀌었다'로만 온다. 왼쪽·오른쪽을 가르는 장치별 비트로 판단한다
/// (둘 다 누른 채 한쪽만 떼도 맞게 나온다).
fn modifier_bit(code: u16) -> Option<u64> {
    Some(match code {
        0x3B => 0x0000_0001, // 왼쪽 Control
        0x38 => 0x0000_0002, // 왼쪽 Shift
        0x3C => 0x0000_0004, // 오른쪽 Shift
        0x37 => 0x0000_0008, // 왼쪽 Command
        0x36 => 0x0000_0010, // 오른쪽 Command
        0x3A => 0x0000_0020, // 왼쪽 Option
        0x3D => 0x0000_0040, // 오른쪽 Option
        0x3E => 0x0000_2000, // 오른쪽 Control
        _ => return None,
    })
}

unsafe extern "C" fn on_event(_proxy: Ref, ty: u32, ev: Ref, _info: Ref) -> Ref {
    match ty {
        // 콜백이 늦거나 사용자가 보안 입력을 켜면 시스템이 탭을 끈다. 다시 켠다.
        TAP_OFF_TIMEOUT | TAP_OFF_USER => {
            let tap = TAP.load(Ordering::Relaxed);
            if !tap.is_null() {
                CGEventTapEnable(tap, true);
            }
        }
        KEY_DOWN | KEY_UP => {
            let code = CGEventGetIntegerValueField(ev, F_KEYCODE) as u16;
            if let Some(vk) = vk_of(code) {
                CODE_OF[vk as usize].store(code, Ordering::Relaxed);
                key(vk, ty == KEY_DOWN);
            }
        }
        FLAGS_CHANGED => {
            let code = CGEventGetIntegerValueField(ev, F_KEYCODE) as u16;
            if code == 0x39 {
                // Caps Lock 은 누를 때마다 한 번 켜짐·꺼짐이 바뀔 뿐 떼는 신호가 없다. 한 번 친 것으로 넘긴다.
                key(0x14, true);
                key(0x14, false);
            } else if let (Some(vk), Some(bit)) = (vk_of(code), modifier_bit(code)) {
                CODE_OF[vk as usize].store(code, Ordering::Relaxed);
                key(vk, CGEventGetFlags(ev) & bit != 0);
            }
        }
        LEFT_DOWN => btn(0, true),
        LEFT_UP => btn(0, false),
        RIGHT_DOWN => btn(1, true),
        RIGHT_UP => btn(1, false),
        OTHER_DOWN | OTHER_UP => {
            if CGEventGetIntegerValueField(ev, F_BUTTON) == 2 {
                btn(2, ty == OTHER_DOWN);
            }
        }
        SCROLL => wheel(CGEventGetIntegerValueField(ev, F_SCROLL_Y) as i32),
        _ => {}
    }
    ev
}

/// '손쉬운 사용' 허락 창을 띄운다. 입력 모니터링만으로 탭이 안 붙는 macOS 판이 있어서, 그때만 한 번 묻는다.
unsafe fn ask_accessibility() {
    let keys = [kAXTrustedCheckOptionPrompt];
    let values = [kCFBooleanTrue];
    let dict = CFDictionaryCreate(
        std::ptr::null_mut(),
        keys.as_ptr(),
        values.as_ptr(),
        1,
        &kCFTypeDictionaryKeyCallBacks as *const c_void,
        &kCFTypeDictionaryValueCallBacks as *const c_void,
    );
    if !dict.is_null() {
        AXIsProcessTrustedWithOptions(dict);
        CFRelease(dict);
    }
}

pub(super) fn spawn_hooks() {
    std::thread::Builder::new()
        .name("input-tap".into())
        .spawn(|| unsafe {
            if !CGPreflightListenEventAccess() {
                CGRequestListenEventAccess();
            }
            let since = Instant::now();
            let mut asked_ax = false;
            loop {
                if CGPreflightListenEventAccess() {
                    let tap = CGEventTapCreate(SESSION_TAP, HEAD_INSERT, LISTEN_ONLY, MASK, on_event, std::ptr::null_mut());
                    if !tap.is_null() {
                        TAP.store(tap, Ordering::Relaxed);
                        let src = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
                        CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
                        CGEventTapEnable(tap, true);
                        log_line("tap ok\n");
                        CFRunLoopRun();
                        return;
                    }
                    if !asked_ax && since.elapsed() > Duration::from_secs(6) {
                        asked_ax = true;
                        log_line("tap refused, asking accessibility\n");
                        ask_accessibility();
                    }
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("input-tap thread");
}

pub(super) fn key_down(vk: u32) -> bool {
    let code = CODE_OF[(vk & 0xff) as usize].load(Ordering::Relaxed);
    // 대조할 맥 키 번호를 모르면 떼지 않는다. 눌린 채 남는 쪽이 아무 키나 떼는 것보다 낫다.
    code == u16::MAX || unsafe { CGEventSourceKeyState(COMBINED_STATE, code) }
}

pub(super) fn btn_down(b: u8) -> bool {
    unsafe { CGEventSourceButtonState(COMBINED_STATE, b as u32) }
}

/// 연결된 모니터를 다 합친 넓이(포인트, 주 모니터 왼쪽 위가 원점) 기준으로 정규화한다.
pub(super) fn pointer() -> Option<Pointer> {
    unsafe {
        let e = CGEventCreate(std::ptr::null_mut());
        if e.is_null() {
            return None;
        }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        let mut ids = [0u32; 16];
        let mut n = 0u32;
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        if CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut n) == 0 {
            for id in &ids[..n as usize] {
                let r = CGDisplayBounds(*id);
                x0 = x0.min(r.origin.x);
                y0 = y0.min(r.origin.y);
                x1 = x1.max(r.origin.x + r.size.width);
                y1 = y1.max(r.origin.y + r.size.height);
            }
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Pointer {
            nx: (p.x - x0) / (x1 - x0),
            ny: (p.y - y0) / (y1 - y0),
            x: p.x,
            y: p.y,
            key: ((p.x * 4.0).round() as i64, (p.y * 4.0).round() as i64),
        })
    }
}

/// 커서를 창 기준 논리 좌표로. 창 자리는 물리 픽셀(포인트 x 배율)로 들고 있으니 포인트로 되돌려 뺀다.
pub(super) fn local(p: &Pointer, spot: &Spot) -> Option<(f64, f64, (i64, i64))> {
    if !spot.is_live() {
        return None;
    }
    let s = spot.scale().max(0.1);
    let (wx, wy) = (spot.x.load(Ordering::Relaxed), spot.y.load(Ordering::Relaxed));
    Some((p.x - wx as f64 / s, p.y - wy as f64 / s, (wx as i64, wy as i64)))
}
