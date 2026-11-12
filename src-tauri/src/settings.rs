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
}

impl Default for Settings {
    fn default() -> Self {
        Self { x: None, y: None, size: "m".into(), passthrough: false, character: "hachiware".into() }
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
