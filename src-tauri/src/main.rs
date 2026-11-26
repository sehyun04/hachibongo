#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod input;
mod nametag;
mod settings;
mod single;
mod together;
mod update;

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
// 휠로 맞출 수 있는 배율. 더 작으면 키보드 글자가 뭉개지고, 더 크면 화면을 너무 가린다.
const ZOOM: (f64, f64) = (0.45, 2.0);
// 캐릭터와 그 viewBox 높이. 우사기는 귀가 길어 창이 위로 56 늘어난다.
// 높이는 scripts/measure-chars.py 가 알려 주는 값(274 - chars.js 의 top)과 같아야 한다.
const CHARS: [(&str, &str, f64); 3] = [("hachiware", "하치와레", 274.0), ("chiikawa", "치이카와", 274.0), ("usagi", "우사기", 330.0)];
// 이름표 자리. 그림이 창 위끝까지 차 있어서 창을 이만큼 위로 늘린다(main.js 의 TAG_ROOM 과 같아야 한다).
// 이름표를 꺼도 줄이지 않는다 — 켜고 끌 때마다 창이 들썩이지 않게.
const TAG_ROOM: f64 = 26.0;

struct Shared {
    path: PathBuf,
    settings: Mutex<Settings>,
    dirty: AtomicBool,
    size_items: Vec<CheckMenuItem<Wry>>,
    char_items: Vec<CheckMenuItem<Wry>>,
    pass_item: CheckMenuItem<Wry>,
    together: together::Menu,
    /// 지금 떠 있는 상대 창의 캐릭터
    peer_char: Mutex<Option<String>>,
    /// 상대 창 띄우기·닫기를 한 줄로 세운다. 안의 값은 마지막으로 따른 요청 번호.
    peer_gate: tauri::async_runtime::Mutex<u64>,
    update: update::State,
    nametag: nametag::Menu,
    /// 같이 하기 상대의 이름표. 상대 창이 뜰 때 `ready` 로 가져간다.
    peer_name: Mutex<Option<String>>,
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
    room: Option<String>,
    client: Option<String>,
    relay: Option<String>,
    name: String,
    nametag: bool,
    peer_name: Option<String>,
}

#[tauri::command]
fn ready(shared: tauri::State<'_, Arc<Shared>>) -> Ready {
    let s = shared.settings.lock().unwrap();
    Ready {
        passthrough: s.passthrough,
        character: s.character.clone(),
        room: s.room.clone(),
        client: s.client.clone(),
        relay: s.relay.clone(),
        name: nametag::of(&s),
        nametag: s.nametag,
        peer_name: shared.peer_name.lock().unwrap().clone(),
    }
}

/// 크기 메뉴로 고른 배율, 휠로 맞췄으면 그 배율.
fn scale_of(s: &Settings) -> f64 {
    s.zoom
        .filter(|z| z.is_finite())
        .map(|z| z.clamp(ZOOM.0, ZOOM.1))
        .unwrap_or_else(|| SIZES.iter().find(|x| x.0 == s.size).map(|x| x.2).unwrap_or(0.9))
}

/// 창 모양을 정하는 두 가지. 크기(메뉴·휠)와 캐릭터 메뉴가 각각 하나씩 바꾼다.
struct Look {
    scale: f64,
    character: String,
}

impl Look {
    fn of(s: &Settings) -> Self {
        Self { scale: scale_of(s), character: s.character.clone() }
    }

    fn base_h(&self) -> f64 {
        CHARS.iter().find(|c| c.0 == self.character).map(|c| c.2).unwrap_or(CHARS[0].2) + TAG_ROOM
    }

    fn logical(&self) -> LogicalSize<f64> {
        LogicalSize::new(BASE_W * self.scale, self.base_h() * self.scale)
    }

    fn physical(&self, win: &WebviewWindow) -> (i32, i32) {
        let f = win.scale_factor().unwrap_or(1.0) * self.scale;
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

/// 창 크기를 바꾼다. 아래 끝은 늘 붙든다 — 책상은 제자리에 있고 귀가 긴 캐릭터만 위로 솟는다.
/// 가로는 `anchor`(0 왼쪽 ~ 1 오른쪽) 자리를 붙든다. 메뉴로 바꿀 때는 기본 자리(화면 오른쪽 아래)에 맞춰 오른쪽,
/// 휠로 바꿀 때는 가운데 — 커서가 캐릭터 위에 있는데 한쪽 끝을 붙들면 캐릭터가 커서 밑에서 빠져나간다.
fn fit_at(win: &WebviewWindow, old: &Look, new: &Look, anchor: f64) {
    let (ow, oh) = old.physical(win);
    let (nw, nh) = new.physical(win);
    let _ = win.set_size(new.logical());
    if let Ok(p) = win.outer_position() {
        let dx = ((ow - nw) as f64 * anchor).round() as i32;
        let _ = win.set_position(PhysicalPosition::new(p.x + dx, p.y + oh - nh));
    }
}

fn fit(win: &WebviewWindow, old: &Look, new: &Look) {
    fit_at(win, old, new, 1.0);
}

/// 크기 메뉴 체크는 휠로 맞춘 배율이 없을 때만 그 메뉴에 둔다.
fn check_sizes(shared: &Shared) {
    let s = shared.settings.lock().unwrap();
    for (item, (k, _, _)) in shared.size_items.iter().zip(SIZES.iter()) {
        let _ = item.set_checked(s.zoom.is_none() && s.size == *k);
    }
}

fn relayout(app: &AppHandle, win: &WebviewWindow, shared: &Shared, old: &Look, new: &Look) {
    fit(win, old, new);
    if (old.scale - new.scale).abs() > 1e-9 {
        together::resize_peer(app, shared, old.scale, new.scale, 1.0);
    }
    check_sizes(shared);
    for (item, (k, _, _)) in shared.char_items.iter().zip(CHARS.iter()) {
        let _ = item.set_checked(*k == new.character);
    }
    shared.settings.lock().unwrap().character = new.character.clone();
    shared.save_now();
}

/// 캐릭터 위에서 휠을 굴리면 웹뷰가 부른다. 같이 하기 상대 창도 같은 배율로 따라간다.
#[tauri::command]
fn zoom(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>, factor: f64) {
    if !factor.is_finite() || factor <= 0.0 {
        return;
    }
    let Some(win) = app.get_webview_window("main") else { return };
    let old = Look::of(&shared.settings.lock().unwrap());
    let scale = (old.scale * factor).clamp(ZOOM.0, ZOOM.1);
    if (scale - old.scale).abs() < 1e-4 {
        return;
    }
    fit_at(&win, &old, &Look { scale, character: old.character.clone() }, 0.5);
    together::resize_peer(&app, &shared, old.scale, scale, 0.5);
    shared.settings.lock().unwrap().zoom = Some(scale);
    check_sizes(&shared);
    // 휠은 한 번에 여러 번 오므로 바로 쓰지 않고 2초마다 모아 쓰는 쪽에 맡긴다
    shared.dirty.store(true, Ordering::Relaxed);
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let Some(win) = app.get_webview_window("main") else { return };
    let id = event.id().as_ref();
    let look = Look::of(&shared.settings.lock().unwrap());
    if id.starts_with("together:") {
        together::on_menu(app, &shared, id);
    } else if id.starts_with("tag:") {
        nametag::on_menu(app, &shared, id);
    } else if let Some(key) = id.strip_prefix("size:") {
        let new = {
            let mut s = shared.settings.lock().unwrap();
            s.size = key.to_string();
            s.zoom = None;
            Look::of(&s)
        };
        relayout(app, &win, &shared, &look, &new);
    } else if let Some(key) = id.strip_prefix("char:") {
        let new = Look { character: key.to_string(), scale: look.scale };
        relayout(app, &win, &shared, &look, &new);
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
        if let Some(peer) = app.get_webview_window("peer") {
            let _ = peer.set_ignore_cursor_events(true);
            let _ = app.emit_to("peer", "lock", on);
        }
        shared.save_now();
    } else if id == "reset" {
        if let Some(p) = default_position(&win, &look) {
            let _ = win.set_position(p);
        }
        together::reset_peer(app, &shared);
    } else if id == "update" {
        update::check(app);
    } else if id == "quit" {
        shared.save_now();
        app.exit(0);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            ready,
            zoom,
            together::peer,
            together::together_status,
            nametag::set_name,
            nametag::peer_name
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let path = settings::path(&handle);
            // 트레이 아이콘·창을 만들기 전에 나가야 두 번째 앱의 흔적이 안 남는다
            if !single::claim(&path) {
                std::process::exit(0);
            }
            let mut s = Settings::load(&path);
            let win = app.get_webview_window("main").expect("main window");
            // 메뉴 막대 아이콘만 두고 독에는 안 띄운다. 책상 위 소품이지 앱 창이 아니라서.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            if s.client.is_none() {
                s.client = Some(together::client_id());
                s.save(&path);
            }
            // 이름표 자리만큼 창이 위로 늘어났다. 예전 자리를 그대로 쓰면 책상이 그만큼 내려앉으므로 한 번 올려 준다.
            if !s.tag_room {
                let dy = (TAG_ROOM * scale_of(&s) * win.scale_factor().unwrap_or(1.0)).round() as i32;
                s.y = s.y.map(|y| y - dy);
                s.peer_y = s.peer_y.map(|y| y - dy);
                s.tag_room = true;
                s.save(&path);
            }

            let size_items = SIZES
                .iter()
                .map(|(k, label, _)| {
                    CheckMenuItem::with_id(app, format!("size:{k}"), *label, true, s.zoom.is_none() && s.size == *k, None::<&str>)
                })
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
            let (together_menu, together) = together::build(app, &s)?;
            let (tag_menu, tag) = nametag::build(app, &s)?;
            let update = update::build(app)?;
            let menu = Menu::with_items(
                app,
                &[&char_menu, &size_menu, &tag_menu, &together_menu, &pass_item, &reset, &sep, update.item(), &quit],
            )?;

            let look = Look::of(&s);
            let saved = s.x.zip(s.y);
            let shared = Arc::new(Shared {
                path,
                settings: Mutex::new(s),
                dirty: AtomicBool::new(false),
                size_items,
                char_items,
                pass_item,
                together,
                peer_char: Mutex::new(None),
                peer_gate: tauri::async_runtime::Mutex::new(0),
                update,
                nametag: tag,
                peer_name: Mutex::new(None),
            });
            app.manage(shared.clone());
            {
                let s = shared.clone();
                single::watch_quit(handle.clone(), &shared.path, move || s.save_now());
            }

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

            input::MAIN.attach(&win);
            update::start(&handle);
            input::start(handle);
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
