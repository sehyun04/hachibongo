//! 머리 위 이름표. 이름은 처음엔 컴퓨터 계정 이름이고, 트레이 '이름표 → 이름 바꾸기' 의 작은 창에서 바꾼다
//! — 오버레이 창은 포커스를 안 받아서 글을 칠 수 없다. 같이 하기에서는 인사(hello)에 실어 보내 상대 화면의 내 캐릭터 위에도 뜬다.

use std::sync::Arc;

use tauri::menu::{CheckMenuItem, IsMenuItem, MenuItem, Submenu};
use tauri::{App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use crate::settings::Settings;
use crate::Shared;

/// 머리 위에 작게 띄우는 거라 길면 머리보다 넓어진다. 웹뷰(main.js 의 cleanName)도 같은 수로 자른다.
const MAX: usize = 10;

/// 줄바꿈·제어 문자를 빼고 공백을 하나로 모아 열 글자까지.
pub fn clean(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX).collect()
}

/// 이름을 안 정했을 때 쓰는 이름. 맥은 사용자 전체 이름, Windows 는 표시 이름이 비어 있기 쉬워 로그인 이름.
fn account_name() -> String {
    #[cfg(target_os = "macos")]
    if let Ok(o) = std::process::Command::new("id").arg("-F").output() {
        let n = clean(&String::from_utf8_lossy(&o.stdout));
        if !n.is_empty() {
            return n;
        }
    }
    let n = clean(&std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default());
    if n.is_empty() { "나".into() } else { n }
}

pub fn of(s: &Settings) -> String {
    s.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(account_name)
}

pub struct Menu {
    show: CheckMenuItem<Wry>,
    edit: MenuItem<Wry>,
}

fn edit_label(name: &str) -> String {
    format!("이름 바꾸기 ({name})")
}

pub fn build(app: &App, s: &Settings) -> tauri::Result<(Submenu<Wry>, Menu)> {
    let show = CheckMenuItem::with_id(app, "tag:show", "보이기", true, s.nametag, None::<&str>)?;
    let edit = MenuItem::with_id(app, "tag:edit", edit_label(&of(s)), true, None::<&str>)?;
    let items: [&dyn IsMenuItem<Wry>; 2] = [&show, &edit];
    let sub = Submenu::with_id_and_items(app, "tag", "이름표", true, &items)?;
    Ok((sub, Menu { show, edit }))
}

pub fn on_menu(app: &AppHandle, shared: &Arc<Shared>, id: &str) {
    match id {
        "tag:show" => {
            let on = {
                let mut s = shared.settings.lock().unwrap();
                s.nametag = !s.nametag;
                s.nametag
            };
            let _ = shared.nametag.show.set_checked(on);
            let _ = app.emit_to("main", "tag", on);
            let _ = app.emit_to("peer", "tag", on);
            shared.save_now();
        }
        "tag:edit" => open_editor(app),
        _ => {}
    }
}

/// 이름 바꾸는 작은 창. 창 만들기는 메뉴 처리 중에 바로 하지 않고 따로 돌린다(Windows 에서 멈출 수 있다, together.rs 의 peer 참고).
fn open_editor(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("name") {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, "name", WebviewUrl::App("name.html".into()))
            .title("이름표")
            .inner_size(300.0, 176.0)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .always_on_top(true)
            .center()
            .focused(true)
            .build();
        if let Ok(w) = built {
            let _ = w.set_focus();
        }
    });
}

/// 이름 창에서 저장. 비우면 컴퓨터 계정 이름으로 돌아간다.
#[tauri::command]
pub fn set_name(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>, name: String) {
    let name = clean(&name);
    let shown = {
        let mut s = shared.settings.lock().unwrap();
        s.name = (!name.is_empty()).then_some(name);
        of(&s)
    };
    shared.save_now();
    let _ = shared.nametag.edit.set_text(edit_label(&shown));
    let _ = app.emit_to("main", "name", &shown);
    if let Some(w) = app.get_webview_window("name") {
        let _ = w.close();
    }
}

/// 같이 하기 상대의 이름. 메인 창이 상대 인사에서 읽어 넘긴다. 상대 창이 뜨기 전에 오면 들고 있다가 창이 `ready` 로 가져간다.
#[tauri::command]
pub fn peer_name(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>, name: Option<String>) {
    let name = name.map(|n| clean(&n)).filter(|n| !n.is_empty());
    *shared.peer_name.lock().unwrap() = name.clone();
    let _ = app.emit_to("peer", "name", name.unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_one_short_line() {
        assert_eq!(clean("  세현\n\t이 "), "세현 이");
        assert_eq!(clean("가나다라마바사아자차카타"), "가나다라마바사아자차");
        assert_eq!(clean("\u{7}\u{1b}[31m"), "[31m");
        assert_eq!(clean("   "), "");
    }
}
