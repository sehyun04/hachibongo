//! 같은 설정으로 앱이 둘 뜨지 않게 하고, 밖에서 앱을 곱게 끌 수 있게 한다.
//!
//! 둘이 뜨면 캐릭터·트레이 아이콘이 둘이 될 뿐 아니라, 같이 하기에서 같은 이름(client id)으로 중계에 붙어
//! 서로를 끊고 다시 붙기를 끝없이 되풀이한다. 설정 파일 경로마다 하나만 허락해서, 검증 때 격리 설정
//! (`HACHIBONGO_CONFIG`)으로 띄우는 앱은 사용자 앱과 같이 뜰 수 있다.
//! 맥은 앱을 다시 열면 떠 있는 것을 앞으로 가져올 뿐 새로 띄우지 않아서 막을 것이 없다.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::AppHandle;

/// 이 설정으로 처음 뜬 앱이면 true. 이미 하나 떠 있으면 false.
#[cfg(windows)]
pub fn claim(config: &Path) -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    // 경로를 그대로 이름에 넣으면 길이·글자 제한에 걸린다
    let key = config.to_string_lossy().to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let name: Vec<u16> = format!("Local\\hachibongo-{h:016x}").encode_utf16().chain(Some(0)).collect();
    unsafe {
        let m = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if m.is_null() {
            // 물어볼 수 없으면 막지 않는다. 둘이 뜨는 것보다 안 뜨는 쪽이 더 곤란하다.
            return true;
        }
        // 핸들은 닫지 않는다. 앱이 꺼질 때 OS 가 거둬 가야 다음 실행이 다시 차지할 수 있다.
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

#[cfg(not(windows))]
pub fn claim(_config: &Path) -> bool {
    true
}

/// 설정 파일 옆에 `.quit` 파일이 생기면 메뉴의 종료와 똑같이 끈다.
/// 프로세스를 강제로 끄면 트레이 아이콘이 지워지지 않고 마우스가 지나갈 때까지 유령으로 남는다.
/// 검증하거나 새 빌드로 바꿔 띄울 때 이걸로 끈다.
pub fn watch_quit(app: AppHandle, config: &Path, on_quit: impl Fn() + Send + 'static) {
    let file: PathBuf = config.with_extension("quit");
    let _ = std::fs::remove_file(&file);
    let _ = std::thread::Builder::new().name("quit-watch".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        if file.exists() {
            let _ = std::fs::remove_file(&file);
            on_quit();
            app.exit(0);
            return;
        }
    });
}
