#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod input;
mod settings;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use settings::Settings;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, WebviewWindow, WindowEvent, Wry};

// 그림의 viewBox 크기. 창은 이 비율 그대로 배율만 바꾼다.
const BASE_W: f64 = 320.0;
const SIZES: [(&str, &str, f64); 3] = [("s", "작게", 0.7), ("m", "보통", 0.9), ("l", "크게", 1.2)];
// 캐릭터와 그 viewBox 높이. 우사기는 귀가 길어 창이 위로 56 늘어난다.
// 높이는 scripts/measure-chars.py 가 알려 주는 값(274 - chars.js 의 top)과 같아야 한다.
const CHARS: [(&str, &str, f64); 3] = [("hachiware", "하치와레", 274.0), ("chiikawa", "치이카와", 274.0), ("usagi", "우사기", 330.0)];

struct Shared {
    path: PathBuf,
    settings: Mutex<Settings>,
    dirty: AtomicBool,
    size_items: Vec<CheckMenuItem<Wry>>,
    char_items: Vec<CheckMenuItem<Wry>>,
    pass_item: CheckMenuItem<Wry>,
}

impl Shared {
    fn save_now(&self) {
        self.dirty.store(false, Ordering::Relaxed);
        let s = self.settings.lock().unwrap().clone();
        s.save(&self.path);
    }
}

#[derive(Serialize)]
struct Ready {
    passthrough: bool,
    character: String,
}

#[tauri::command]
fn ready(shared: tauri::State<'_, Arc<Shared>>) -> Ready {
    let s = shared.settings.lock().unwrap();
    Ready { passthrough: s.passthrough, character: s.character.clone() }
}

/// 창 모양을 정하는 두 가지. 크기 메뉴와 캐릭터 메뉴가 각각 하나씩 바꾼다.
struct Look {
    size: String,
    character: String,
}

impl Look {
    fn of(s: &Settings) -> Self {
        Self { size: s.size.clone(), character: s.character.clone() }
    }

    fn scale(&self) -> f64 {
        SIZES.iter().find(|s| s.0 == self.size).map(|s| s.2).unwrap_or(0.9)
    }

    fn base_h(&self) -> f64 {
        CHARS.iter().find(|c| c.0 == self.character).map(|c| c.2).unwrap_or(CHARS[0].2)
    }

    fn logical(&self) -> LogicalSize<f64> {
        LogicalSize::new(BASE_W * self.scale(), self.base_h() * self.scale())
    }

    fn physical(&self, win: &WebviewWindow) -> (i32, i32) {
        let f = win.scale_factor().unwrap_or(1.0) * self.scale();
        ((BASE_W * f).round() as i32, (self.base_h() * f).round() as i32)
    }
}

/// 기본 자리는 주 모니터 작업 영역의 오른쪽 아래(작업 표시줄 바로 위).
fn default_position(win: &WebviewWindow, look: &Look) -> Option<PhysicalPosition<i32>> {
    let m = win.primary_monitor().ok().flatten()?;
    let wa = m.work_area();
    let (w, h) = look.physical(win);
    let margin = (16.0 * m.scale_factor()) as i32;
    Some(PhysicalPosition::new(
        wa.position.x + wa.size.width as i32 - w - margin,
        wa.position.y + wa.size.height as i32 - h,
    ))
}

/// 모니터를 뺀 뒤 화면 밖에서 켜져 못 찾는 일을 막는다.
fn on_screen(win: &WebviewWindow, x: i32, y: i32, look: &Look) -> bool {
    let (w, h) = look.physical(win);
    let (cx, cy) = (x + w / 2, y + h / 2);
    win.available_monitors()
        .map(|ms| {
            ms.iter().any(|m| {
                let p = m.position();
                let s = m.size();
                cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32
            })
        })
        .unwrap_or(false)
}

fn relayout(win: &WebviewWindow, shared: &Shared, new: &Look) {
    let old = Look::of(&shared.settings.lock().unwrap());
    let (ow, oh) = old.physical(win);
    let (nw, nh) = new.physical(win);
    let _ = win.set_size(new.logical());
    // 기본 자리가 화면 오른쪽 아래라서 그 모서리를 붙든 채 줄이고 늘린다. 캐릭터를 바꿔도 책상은 제자리에 있고
    // 귀가 긴 캐릭터만 위로 솟는다.
    if let Ok(p) = win.outer_position() {
        let _ = win.set_position(PhysicalPosition::new(p.x + ow - nw, p.y + oh - nh));
    }
    for (item, (k, _, _)) in shared.size_items.iter().zip(SIZES.iter()) {
        let _ = item.set_checked(*k == new.size);
    }
    for (item, (k, _, _)) in shared.char_items.iter().zip(CHARS.iter()) {
        let _ = item.set_checked(*k == new.character);
    }
    {
        let mut s = shared.settings.lock().unwrap();
        s.size = new.size.clone();
        s.character = new.character.clone();
    }
    shared.save_now();
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let Some(win) = app.get_webview_window("main") else { return };
    let id = event.id().as_ref();
    let look = Look::of(&shared.settings.lock().unwrap());
    if let Some(key) = id.strip_prefix("size:") {
        relayout(&win, &shared, &Look { size: key.to_string(), ..look });
    } else if let Some(key) = id.strip_prefix("char:") {
        relayout(&win, &shared, &Look { character: key.to_string(), ..look });
        let _ = app.emit_to("main", "char", key);
    } else if id == "pass" {
        let on = {
            let mut s = shared.settings.lock().unwrap();
            s.passthrough = !s.passthrough;
            s.passthrough
        };
        let _ = shared.pass_item.set_checked(on);
        // 끌 때도 일단 통과로 둔다. 커서가 그림 위로 오면 웹뷰가 다시 연다.
        let _ = win.set_ignore_cursor_events(true);
        let _ = app.emit_to("main", "lock", on);
        shared.save_now();
    } else if id == "reset" {
        if let Some(p) = default_position(&win, &look) {
            let _ = win.set_position(p);
        }
    } else if id == "quit" {
        shared.save_now();
        app.exit(0);
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ready])
        .setup(|app| {
            let handle = app.handle().clone();
            let path = settings::path(&handle);
            let mut s = Settings::load(&path);
            let win = app.get_webview_window("main").expect("main window");

            let size_items = SIZES
                .iter()
                .map(|(k, label, _)| CheckMenuItem::with_id(app, format!("size:{k}"), *label, true, s.size == *k, None::<&str>))
                .collect::<tauri::Result<Vec<_>>>()?;
            let size_refs: Vec<&dyn IsMenuItem<Wry>> = size_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
            let size_menu = Submenu::with_id_and_items(app, "size", "크기", true, &size_refs)?;
            // 저장된 이름이 목록에 없으면(손으로 고쳤거나 옛 판) 웹뷰는 하치와레를 그리므로 메뉴도 하치와레에 표시한다
            if !CHARS.iter().any(|c| c.0 == s.character) {
                s.character = CHARS[0].0.to_string();
            }
            let char_items = CHARS
                .iter()
                .map(|(k, label, _)| CheckMenuItem::with_id(app, format!("char:{k}"), *label, true, s.character == *k, None::<&str>))
                .collect::<tauri::Result<Vec<_>>>()?;
            let char_refs: Vec<&dyn IsMenuItem<Wry>> = char_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
            let char_menu = Submenu::with_id_and_items(app, "char", "캐릭터", true, &char_refs)?;
            let pass_item = CheckMenuItem::with_id(app, "pass", "클릭 통과 (고정)", true, s.passthrough, None::<&str>)?;
            let reset = MenuItem::with_id(app, "reset", "위치 초기화", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&char_menu, &size_menu, &pass_item, &reset, &sep, &quit])?;

            let look = Look::of(&s);
            let saved = s.x.zip(s.y);
            let shared = Arc::new(Shared {
                path,
                settings: Mutex::new(s),
                dirty: AtomicBool::new(false),
                size_items,
                char_items,
                pass_item,
            });
            app.manage(shared.clone());

            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().expect("icon").clone())
                .tooltip("하치봉고")
                .menu(&menu)
                .on_menu_event(on_menu)
                .build(app)?;

            win.set_size(look.logical())?;
            let pos = saved
                .map(|(x, y)| PhysicalPosition::new(x, y))
                .filter(|p| on_screen(&win, p.x, p.y, &look))
                .or_else(|| default_position(&win, &look));
            if let Some(p) = pos {
                win.set_position(p)?;
            }
            // 처음엔 전부 통과. 커서가 그림 위에 올라올 때만 웹뷰가 마우스를 받는다.
            win.set_ignore_cursor_events(true)?;

            {
                let shared = shared.clone();
                win.on_window_event(move |e| {
                    if let WindowEvent::Moved(p) = e {
                        let mut s = shared.settings.lock().unwrap();
                        s.x = Some(p.x);
                        s.y = Some(p.y);
                        shared.dirty.store(true, Ordering::Relaxed);
                    }
                });
            }
            // 끄는 동안 Moved 가 수십 번 오므로 바로 쓰지 않고 2초마다 모아 쓴다.
            {
                let shared = shared.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(2));
                    if shared.dirty.load(Ordering::Relaxed) {
                        shared.save_now();
                    }
                });
            }

            input::start(handle, win.hwnd()?.0 as isize);
            win.show()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("tauri app")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                if let Some(shared) = app.try_state::<Arc<Shared>>() {
                    if shared.dirty.load(Ordering::Relaxed) {
                        shared.save_now();
                    }
                }
            }
        });
}
