//! 같이 하기 — 두 사람의 하치봉고를 방 코드 하나로 잇는다. 상대 캐릭터는 내 화면에 창 하나로 더 뜨고 상대 입력대로 친다.
//!
//! 중계와 주고받는 일은 메인 창 웹뷰(src/together.js)가 맡는다. 여기서는 트레이 메뉴, 방 코드 만들기·읽기(클립보드),
//! 상대 창 띄우고 닫기만 한다. 방 코드는 카톡 같은 데로 보내라고 클립보드에 문장째 넣고, 받는 쪽은 클립보드에서 코드만 골라낸다
//! — 이 앱에는 글을 칠 칸이 없다.

use std::sync::Arc;

use tauri::menu::{IsMenuItem, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{App, AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry};

use crate::settings::Settings;
use crate::{input, on_screen, Look, Shared, CHARS};

/// 헷갈리는 글자(i·l·o·0·1)를 뺐다. 10자면 약 50비트라 남이 맞혀 들어올 수 없다.
const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const CODE_LEN: usize = 10;

pub struct Menu {
    status: MenuItem<Wry>,
    copy: MenuItem<Wry>,
    leave: MenuItem<Wry>,
}

pub fn build(app: &App, s: &Settings) -> tauri::Result<(Submenu<Wry>, Menu)> {
    let status = MenuItem::with_id(app, "together:status", if s.room.is_some() { "연결하는 중" } else { "꺼짐" }, false, None::<&str>)?;
    let new = MenuItem::with_id(app, "together:new", "새 방 만들기 (코드 복사)", true, None::<&str>)?;
    let join = MenuItem::with_id(app, "together:join", "복사한 코드로 들어가기", true, None::<&str>)?;
    let copy = MenuItem::with_id(app, "together:copy", "방 코드 다시 복사", s.room.is_some(), None::<&str>)?;
    let leave = MenuItem::with_id(app, "together:leave", "나가기", s.room.is_some(), None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let items: [&dyn IsMenuItem<Wry>; 6] = [&status, &sep, &new, &join, &copy, &leave];
    let sub = Submenu::with_id_and_items(app, "together", "같이 하기", true, &items)?;
    Ok((sub, Menu { status, copy, leave }))
}

fn random(n: usize) -> String {
    let mut out = String::with_capacity(n);
    let mut buf = [0u8; 64];
    while out.len() < n {
        if getrandom::fill(&mut buf).is_err() {
            break;
        }
        // 31 의 배수 아래만 받아야 글자마다 확률이 같다
        for b in buf {
            if out.len() < n && (b as usize) < ALPHABET.len() * 8 {
                out.push(ALPHABET[b as usize % ALPHABET.len()] as char);
            }
        }
    }
    out
}

fn new_code() -> String {
    // 숫자와 글자가 다 섞인 것만 쓴다. 클립보드 문장에서 코드를 골라낼 때 보통 낱말과 헷갈리지 않게.
    loop {
        let c = random(CODE_LEN);
        if c.len() == CODE_LEN && is_code(&c) {
            return c;
        }
    }
}

fn is_code(t: &str) -> bool {
    t.len() == CODE_LEN
        && t.bytes().all(|b| ALPHABET.contains(&b))
        && t.bytes().any(|b| b.is_ascii_digit())
        && t.bytes().any(|b| b.is_ascii_lowercase())
}

/// 붙여 넣은 문장에서 방 코드만 골라낸다. 대문자로 바뀌어 왔어도 받는다.
pub fn find_code(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .find(|t| is_code(t))
}

pub fn client_id() -> String {
    random(16)
}

fn clipboard_set(text: &str) -> bool {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())).is_ok()
}

fn clipboard_get() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok()
}

fn set_room(app: &AppHandle, shared: &Shared, room: Option<String>) {
    {
        let mut s = shared.settings.lock().unwrap();
        s.room = room.clone();
    }
    shared.save_now();
    let _ = shared.together.copy.set_enabled(room.is_some());
    let _ = shared.together.leave.set_enabled(room.is_some());
    if room.is_none() {
        close_peer(app, shared);
    }
    let _ = app.emit_to("main", "room", room);
}

fn set_status(shared: &Shared, text: &str) {
    let _ = shared.together.status.set_text(text);
}

pub fn on_menu(app: &AppHandle, shared: &Arc<Shared>, id: &str) {
    match id {
        "together:new" => {
            let code = new_code();
            let copied = clipboard_set(&share_text(&code));
            set_room(app, shared, Some(code));
            set_status(shared, if copied { "코드를 복사했어요 · 상대를 기다리는 중" } else { "코드 복사 실패 · 다시 복사를 눌러 주세요" });
        }
        "together:join" => match clipboard_get().as_deref().and_then(find_code) {
            Some(code) => {
                set_room(app, shared, Some(code));
                set_status(shared, "연결하는 중");
            }
            None => set_status(shared, "클립보드에 방 코드가 없어요"),
        },
        "together:copy" => {
            let room = shared.settings.lock().unwrap().room.clone();
            if let Some(code) = room {
                if clipboard_set(&share_text(&code)) {
                    set_status(shared, "코드를 복사했어요");
                }
            }
        }
        "together:leave" => {
            set_room(app, shared, None);
            set_status(shared, "꺼짐");
        }
        _ => {}
    }
}

fn share_text(code: &str) -> String {
    format!("하치봉고 같이 하기 방 코드: {code}")
}

/// 웹뷰가 중계 상태를 알린다(연결 중·혼자·상대 있음). 트레이 메뉴 맨 위 줄에 그대로 보인다.
#[tauri::command]
pub fn together_status(shared: tauri::State<'_, Arc<Shared>>, text: String) {
    set_status(&shared, &text);
}

/// 상대 창을 띄우거나(캐릭터 이름) 닫는다(None). 웹뷰가 상대의 인사·나감을 받을 때 부른다.
/// 창을 만드는 일은 동기 명령에서 하면 Windows 에서 멈출 수 있어 async 로 둔다.
/// async 명령은 저마다 따로 돌아서 닫기 바로 뒤의 띄우기가 먼저 끝날 수 있다. 한 줄로 세우고,
/// 웹뷰가 붙인 번호(`seq`)로 이미 더 나중 요청을 따랐으면 늦게 온 것은 버린다.
#[tauri::command]
pub async fn peer(app: AppHandle, character: Option<String>, seq: u64) -> Result<(), String> {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    let mut last = shared.peer_gate.lock().await;
    if seq <= *last {
        return Ok(());
    }
    *last = seq;
    let Some(c) = character else {
        close_peer(&app, &shared);
        return Ok(());
    };
    let c = if CHARS.iter().any(|x| x.0 == c) { c } else { CHARS[0].0.to_string() };
    let size = shared.settings.lock().unwrap().size.clone();
    let look = Look { size, character: c.clone() };
    let before = shared.peer_char.lock().unwrap().replace(c.clone());

    if let Some(w) = app.get_webview_window("peer") {
        if let Some(old) = before {
            crate::fit(&w, &Look { size: look.size.clone(), character: old }, &look);
        }
        let _ = app.emit_to("peer", "char", &c);
        return Ok(());
    }

    // 창 이름은 웹뷰가 맨 처음 읽어야 해서 스크립트로 먼저 심는다(주소 뒤 물음표는 앱 주소에서 떨어져 나간다).
    let init = format!("window.__HACHI = {{ role: 'peer', char: {} }};", serde_json::to_string(&c).unwrap_or_default());
    let sz = look.logical();
    let w = WebviewWindowBuilder::new(&app, "peer", WebviewUrl::App("index.html".into()))
        .title("hachibongo")
        .initialization_script(&init)
        .inner_size(sz.width, sz.height)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    let saved = {
        let s = shared.settings.lock().unwrap();
        s.peer_x.zip(s.peer_y)
    };
    let pos = saved
        .map(|(x, y)| PhysicalPosition::new(x, y))
        .filter(|p| on_screen(&w, p.x, p.y, &look))
        .or_else(|| beside_main(&app, &w, &look));
    if let Some(p) = pos {
        let _ = w.set_position(p);
    }
    let _ = w.set_ignore_cursor_events(true);
    input::PEER.attach(&w);
    {
        let shared = shared.clone();
        w.on_window_event(move |e| {
            if let WindowEvent::Moved(p) = e {
                let mut s = shared.settings.lock().unwrap();
                s.peer_x = Some(p.x);
                s.peer_y = Some(p.y);
                shared.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }
    let _ = w.show();
    Ok(())
}

pub fn close_peer(app: &AppHandle, shared: &Shared) {
    shared.peer_char.lock().unwrap().take();
    shared.peer_name.lock().unwrap().take();
    if let Some(w) = app.get_webview_window("peer") {
        input::PEER.detach();
        let _ = w.destroy();
    }
}

/// 처음 뜨는 자리: 내 캐릭터 왼쪽에 책상 높이를 맞춰 나란히. 왼쪽이 화면 밖이면 오른쪽.
pub fn beside_main(app: &AppHandle, peer: &WebviewWindow, look: &Look) -> Option<PhysicalPosition<i32>> {
    let main = app.get_webview_window("main")?;
    let mp = main.outer_position().ok()?;
    let ms = main.outer_size().ok()?;
    let (pw, ph) = look.physical(peer);
    let gap = (8.0 * peer.scale_factor().unwrap_or(1.0)) as i32;
    let y = mp.y + ms.height as i32 - ph;
    let left = PhysicalPosition::new(mp.x - pw - gap, y);
    if on_screen(peer, left.x, left.y, look) {
        return Some(left);
    }
    Some(PhysicalPosition::new(mp.x + ms.width as i32 + gap, y))
}

/// 위치 초기화 때 상대 창도 내 옆으로 다시 붙인다.
pub fn reset_peer(app: &AppHandle, shared: &Shared) {
    let Some(w) = app.get_webview_window("peer") else { return };
    let Some(c) = shared.peer_char.lock().unwrap().clone() else { return };
    let look = Look { size: shared.settings.lock().unwrap().size.clone(), character: c };
    if let Some(p) = beside_main(app, &w, &look) {
        let _ = w.set_position(p);
    }
}

/// 크기 메뉴를 바꾸면 상대 창도 같은 배율로.
pub fn resize_peer(app: &AppHandle, shared: &Shared, old_size: &str, new_size: &str) {
    let Some(w) = app.get_webview_window("peer") else { return };
    let Some(c) = shared.peer_char.lock().unwrap().clone() else { return };
    crate::fit(
        &w,
        &Look { size: old_size.to_string(), character: c.clone() },
        &Look { size: new_size.to_string(), character: c },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_survive_the_round_trip() {
        for _ in 0..200 {
            let c = new_code();
            assert_eq!(c.len(), CODE_LEN);
            assert_eq!(find_code(&share_text(&c)).as_deref(), Some(c.as_str()));
            assert_eq!(find_code(&share_text(&c.to_ascii_uppercase())).as_deref(), Some(c.as_str()));
        }
    }

    #[test]
    fn plain_words_are_not_codes() {
        assert_eq!(find_code("하치봉고 같이 하기 방 코드"), None);
        assert_eq!(find_code("helloworld"), None);
        assert_eq!(find_code("1234567890"), None);
        assert_eq!(find_code("abcdefghj2x"), None);
        assert_eq!(find_code("  k7m2xq9fta \n"), Some("k7m2xq9fta".into()));
    }
}
