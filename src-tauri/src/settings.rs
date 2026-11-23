use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub size: String,
    pub passthrough: bool,
    pub character: String,
    /// 같이 하기 방 코드. 남아 있으면 켤 때 다시 들어간다.
    pub room: Option<String>,
    /// 중계가 같은 앱의 재접속을 알아보는 이름. 처음 켤 때 한 번 정한다.
    pub client: Option<String>,
    /// 중계 주소를 바꾸는 손잡이(검증할 때 이 PC 의 wrangler dev 로). 비면 기본 중계.
    pub relay: Option<String>,
    pub peer_x: Option<i32>,
    pub peer_y: Option<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            size: "m".into(),
            passthrough: false,
            character: "hachiware".into(),
            room: None,
            client: None,
            relay: None,
            peer_x: None,
            peer_y: None,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // 쓰는 도중 꺼져도 반쯤 쓴 파일이 남지 않게 옆에 쓰고 바꿔 끼운다.
        let tmp = path.with_extension("json.tmp");
        if let Ok(text) = serde_json::to_vec_pretty(self) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }
}

/// `HACHIBONGO_CONFIG` 로 경로를 바꿀 수 있다. 검증할 때 실제 설정을 건드리지 않으려고 둔 손잡이.
pub fn path(app: &AppHandle) -> PathBuf {
    if let Some(p) = std::env::var_os("HACHIBONGO_CONFIG") {
        return PathBuf::from(p);
    }
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("hachibongo"))
        .join("settings.json")
}
