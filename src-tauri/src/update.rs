//! 자체 업데이트. 새 판은 GitHub 릴리스에 올라가고(.github/workflows/release.yml), 앱은 맨 마지막 릴리스의 latest.json 을 본다.
//! 새 판이 있으면 묻지 않고 받아서 바꿔 끼운다. 이 앱은 트레이에만 있어서 알림을 띄워도 볼 사람이 없다.
//!
//! CI 가 구운 판만 업데이트한다. 직접 구운 판까지 하면 개발하며 띄운 exe 가 20초 뒤 릴리스 판을 깔고 꺼져 버린다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::menu::MenuItem;
use tauri::{App, AppHandle, Manager, Wry};
use tauri_plugin_updater::UpdaterExt;

use crate::input::log_line;
use crate::Shared;

const ENDPOINT: &str = "https://github.com/sehyun04/hachibongo/releases/latest/download/latest.json";
const RELEASE: bool = option_env!("HACHIBONGO_RELEASE").is_some();
/// 켜자마자 묻지 않는다. 켜는 중에 다시 켜지면 고장 난 것처럼 보인다.
const FIRST: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(6 * 3600);

pub struct State {
    item: MenuItem<Wry>,
    busy: AtomicBool,
}

fn version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

pub fn build(app: &App) -> tauri::Result<State> {
    let v = app.package_info().version.to_string();
    let item = if RELEASE {
        MenuItem::with_id(app, "update", format!("업데이트 확인 ({v})"), true, None::<&str>)?
    } else {
        MenuItem::with_id(app, "update", format!("직접 구운 판 ({v})"), false, None::<&str>)?
    };
    Ok(State { item, busy: AtomicBool::new(false) })
}

impl State {
    pub fn item(&self) -> &MenuItem<Wry> {
        &self.item
    }

    fn show(&self, text: &str, enabled: bool) {
        let _ = self.item.set_text(text);
        let _ = self.item.set_enabled(enabled);
    }
}

pub fn start(app: &AppHandle) {
    if !RELEASE {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST);
        loop {
            check(&app);
            std::thread::sleep(EVERY);
        }
    });
}

pub fn check(app: &AppHandle) {
    if !RELEASE {
        return;
    }
    let shared = app.state::<Arc<Shared>>().inner().clone();
    if shared.update.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let v = version(&app);
        shared.update.show("업데이트 확인 중", false);
        match run(&app, &shared).await {
            // 바꿔 끼우는 쪽이 메뉴와 busy 를 맡는다. 끝나면 앱이 다시 켜진다.
            Ok(true) => return,
            Ok(false) => shared.update.show(&format!("최신 버전이에요 ({v})"), true),
            Err(e) => {
                log_line(&format!("update failed: {e}\n"));
                shared.update.show(&format!("업데이트 실패 · 다시 확인 ({v})"), true);
            }
        }
        shared.update.busy.store(false, Ordering::SeqCst);
    });
}

/// 새 판을 받아 바꿔 끼우는 데까지 넘겼으면 true.
async fn run(app: &AppHandle, shared: &Arc<Shared>) -> Result<bool, Box<dyn std::error::Error>> {
    let (saver, handle) = (shared.clone(), app.clone());
    let updater = app
        .updater_builder()
        .endpoints(vec![ENDPOINT.parse()?])?
        .timeout(Duration::from_secs(120))
        // 윈도우에서는 설치 프로그램을 띄운 직후 프로세스를 바로 끝낸다. 그 전에 설정을 쓰고 트레이 아이콘을 내려야
        // 알림 영역에 유령 아이콘이 안 남는다.
        .on_before_exit(move || {
            saver.save_now();
            handle.cleanup_before_exit();
        })
        .build()?;
    let Some(update) = updater.check().await? else {
        log_line(&format!("update: none newer than {}\n", version(app)));
        return Ok(false);
    };
    log_line(&format!("update: {} -> {}\n", update.current_version, update.version));
    shared.update.show(&format!("새 버전 {} 받는 중", update.version), false);
    let bytes = update.download(|_, _| {}, || {}).await?;
    log_line(&format!("update: downloaded {} bytes\n", bytes.len()));

    // 트레이 아이콘은 메인 스레드 것이라 내리는 일(on_before_exit)도 거기서 해야 한다.
    let (app2, shared2) = (app.clone(), shared.clone());
    app.run_on_main_thread(move || match update.install(&bytes) {
        // 윈도우는 install 안에서 설치 프로그램을 띄우고 끝나므로 여기 오는 건 맥뿐이다. 맥은 앱 묶음을 바꿔 끼웠으니 새 판으로 다시 켠다.
        Ok(()) => {
            shared2.save_now();
            app2.restart();
        }
        Err(e) => {
            log_line(&format!("update install failed: {e}\n"));
            let v = version(&app2);
            shared2.update.show(&format!("업데이트 실패 · 다시 확인 ({v})"), true);
            shared2.update.busy.store(false, Ordering::SeqCst);
        }
    })?;
    Ok(true)
}
