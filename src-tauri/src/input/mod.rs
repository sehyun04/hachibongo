//! 전역 키보드·마우스 입력을 웹뷰로 넘긴다.
//!
//! 키와 버튼은 OS 의 저수준 입력 갈고리로 받는다(Windows 는 WH_*_LL 훅, macOS 는 듣기만 하는 CGEventTap).
//! 폴링은 한 프레임보다 짧은 타건을 놓치고, 휠은 아예 폴링할 수 없다. 커서 위치만은 갈고리로 받으면
//! 고주사율 마우스에서 초당 1000번씩 이벤트가 쏟아지므로 60Hz 로 직접 읽는다.
//!
//! 갈고리 콜백 안에서는 채널에 넣기만 한다. 콜백이 느리면 시스템 전체 입력이 굼떠지고,
//! 시간 제한을 넘기면 OS 가 말없이 갈고리를 떼 버린다(Windows 는 훅을 떼고, macOS 는 탭을 끈다).
//!
//! 키 번호는 두 OS 모두 Windows 가상 키 번호로 맞춰 넘긴다. 화면 키 배열(main.js 의 ROWS)이 그 번호로 짜여 있다.
//! 어떤 키를 눌렀는지는 화면의 키 위치를 고르는 데만 쓰고 어디에도 남기지 않는다.

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as sys;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as sys;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, WebviewWindow, WindowEvent};

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
    /// 같이 하기의 상대 창에는 커서가 그 창의 어디에 있는지만 준다. 그 창이 끌기·클릭 통과를 가르는 데만 쓰고,
    /// 상대 캐릭터는 내 입력으로 움직이지 않는다.
    #[serde(rename = "h")]
    Hover { lx: f64, ly: f64 },
}

static TX: OnceLock<Sender<Ev>> = OnceLock::new();
// 키를 누르고 있으면 누름 신호가 반복해서 온다. 상태가 바뀔 때만 넘긴다.
static KEYS: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];
static BTNS: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];

fn send(ev: Ev) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(ev);
    }
}

fn key(vk: u32, d: bool) {
    let vk = vk & 0xff;
    if KEYS[vk as usize].swap(d, Ordering::Relaxed) != d {
        send(Ev::Key { vk, d });
    }
}

fn btn(b: u8, d: bool) {
    if BTNS[b as usize].swap(d, Ordering::Relaxed) != d {
        send(Ev::Btn { b, d });
    }
}

fn wheel(dy: i32) {
    send(Ev::Wheel { dy });
}

/// 창 하나의 자리. 커서를 그 창 기준 좌표로 바꾸는 데 쓴다.
/// Windows 는 창 핸들로 그때그때 묻고, macOS 는 창이 옮겨질 때 받은 자리를 들고 있는다
/// (macOS 창 함수는 메인 스레드에서만 불려서, 60Hz 로 묻으면 그때마다 메인 스레드를 기다린다).
pub struct Spot {
    live: AtomicBool,
    gen: AtomicU64,
    #[cfg_attr(not(windows), allow(dead_code))]
    hwnd: AtomicIsize,
    #[cfg_attr(windows, allow(dead_code))]
    x: AtomicI32,
    #[cfg_attr(windows, allow(dead_code))]
    y: AtomicI32,
    #[cfg_attr(windows, allow(dead_code))]
    scale: AtomicU32,
}

pub static MAIN: Spot = Spot::new();
pub static PEER: Spot = Spot::new();

impl Spot {
    const fn new() -> Self {
        Self {
            live: AtomicBool::new(false),
            gen: AtomicU64::new(0),
            hwnd: AtomicIsize::new(0),
            x: AtomicI32::new(0),
            y: AtomicI32::new(0),
            scale: AtomicU32::new(0x3f80_0000), // 1.0
        }
    }

    fn is_live(&self) -> bool {
        self.live.load(Ordering::Relaxed)
    }

    #[cfg_attr(windows, allow(dead_code))]
    fn scale(&self) -> f64 {
        f32::from_bits(self.scale.load(Ordering::Relaxed)) as f64
    }

    fn put(&self, x: i32, y: i32) {
        self.x.store(x, Ordering::Relaxed);
        self.y.store(y, Ordering::Relaxed);
    }

    /// 창을 붙든다. 같은 이름의 창이 닫혔다 다시 열릴 수 있어서, 앞 창이 늦게 보낸 닫힘 신호가
    /// 새 창을 놓아 버리지 않게 세대로 가린다.
    pub fn attach(&'static self, win: &WebviewWindow) {
        let gen = self.gen.fetch_add(1, Ordering::Relaxed) + 1;
        #[cfg(windows)]
        if let Ok(h) = win.hwnd() {
            self.hwnd.store(h.0 as isize, Ordering::Relaxed);
        }
        if let Ok(p) = win.outer_position() {
            self.put(p.x, p.y);
        }
        if let Ok(s) = win.scale_factor() {
            self.scale.store((s as f32).to_bits(), Ordering::Relaxed);
        }
        self.live.store(true, Ordering::Relaxed);
        win.on_window_event(move |e| match e {
            WindowEvent::Moved(p) => self.put(p.x, p.y),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale.store((*scale_factor as f32).to_bits(), Ordering::Relaxed)
            }
            WindowEvent::Destroyed => {
                if self.gen.load(Ordering::Relaxed) == gen {
                    self.detach();
                }
            }
            _ => {}
        });
    }

    pub fn detach(&self) {
        self.live.store(false, Ordering::Relaxed);
        self.hwnd.store(0, Ordering::Relaxed);
    }
}

/// 갈고리 스레드와 전달 스레드를 띄운다. 창 기준 커서 좌표는 `MAIN`·`PEER` 에 붙든 창으로 계산한다.
pub fn start(app: AppHandle) {
    let (tx, rx) = channel::<Ev>();
    let _ = TX.set(tx);
    sys::spawn_hooks();
    std::thread::Builder::new()
        .name("input-forward".into())
        .spawn(move || forward(app, rx))
        .expect("input-forward thread");
}

fn forward(app: AppHandle, rx: std::sync::mpsc::Receiver<Ev>) {
    let frame = Duration::from_millis(16);
    let mut last_main: Option<[i64; 4]> = None;
    let mut last_peer: Option<[i64; 4]> = None;
    // 길게 눌린 키는 떼는 신호를 놓쳤을 수 있다(관리자 창으로 포커스 이동, 잠금 화면, macOS 의 보안 입력 칸 등).
    // 1.5초 넘게 눌린 것만 실제 키 상태와 대조한다. 방금 누른 키는 갈고리가 시스템 상태보다
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
                        Ev::Key { vk, d: true } => {
                            held.insert(vk, Instant::now());
                            counts[0] += 1;
                        }
                        Ev::Key { vk, d: false } => {
                            held.remove(&vk);
                        }
                        Ev::Btn { b, d } => {
                            held_btn[b as usize] = d.then(Instant::now);
                            if d {
                                counts[1] += 1;
                            }
                        }
                        Ev::Wheel { .. } => counts[2] += 1,
                        Ev::Move { .. } | Ev::Hover { .. } => {}
                    }
                    let _ = app.emit_to("main", "input", ev);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        if let Some(p) = sys::pointer() {
            if let Some((lx, ly, w)) = sys::local(&p, &MAIN) {
                let k = [p.key.0, p.key.1, w.0, w.1];
                if last_main != Some(k) {
                    last_main = Some(k);
                    counts[3] += 1;
                    let _ = app.emit_to("main", "input", Ev::Move { nx: p.nx, ny: p.ny, lx, ly });
                }
            }
            match sys::local(&p, &PEER) {
                Some((lx, ly, w)) if PEER.is_live() => {
                    let k = [p.key.0, p.key.1, w.0, w.1];
                    if last_peer != Some(k) {
                        last_peer = Some(k);
                        let _ = app.emit_to("peer", "hover", Ev::Hover { lx, ly });
                    }
                }
                _ => last_peer = None,
            }
        }

        if last_check.elapsed() > Duration::from_millis(250) {
            last_check = Instant::now();
            let stale = Duration::from_millis(1500);
            let stuck: Vec<u32> = held
                .iter()
                .filter(|(vk, at)| at.elapsed() > stale && !sys::key_down(**vk))
                .map(|(vk, _)| *vk)
                .collect();
            for vk in stuck {
                held.remove(&vk);
                KEYS[vk as usize].store(false, Ordering::Relaxed);
                let _ = app.emit_to("main", "input", Ev::Key { vk, d: false });
            }
            for b in 0..3 {
                if held_btn[b].is_some_and(|at| at.elapsed() > stale) && !sys::btn_down(b as u8) {
                    held_btn[b] = None;
                    BTNS[b].store(false, Ordering::Relaxed);
                    let _ = app.emit_to("main", "input", Ev::Btn { b: b as u8, d: false });
                }
            }
        }

        // 갈고리가 살아 있는지 확인하는 손잡이. 무슨 키였는지는 적지 않고 개수만 남긴다.
        if log && last_log.elapsed() > Duration::from_secs(5) {
            last_log = Instant::now();
            log_line(&format!("keys={} clicks={} wheel={} moves={}\n", counts[0], counts[1], counts[2], counts[3]));
            counts = [0; 4];
        }
    }
}

/// 화면 위 커서 한 점. `nx`·`ny` 는 화면 전체(모니터를 다 합친 넓이) 안에서의 비율이다.
/// 모니터별로 자르면 화면 경계를 넘을 때 발이 패드 반대편으로 순간이동한다.
struct Pointer {
    nx: f64,
    ny: f64,
    /// 창 기준 좌표를 셀 때 쓰는 OS 단위의 자리(Windows 는 물리 픽셀, macOS 는 포인트)
    x: f64,
    y: f64,
    /// 바뀌었는지 견주는 값
    key: (i64, i64),
}

fn log_line(line: &str) {
    use std::io::Write;
    let Some(path) = std::env::var_os("HACHIBONGO_LOG") else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}
