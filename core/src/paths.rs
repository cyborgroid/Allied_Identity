//! 플랫폼별 저장 위치. C# 컴패니언(`Paths.cs`)과 같은 규칙을 따른다.
//!
//! `ALLIEDID_HOME` 환경 변수가 있으면 모든 파일을 그 디렉터리 하나에 둔다.

use std::env;
use std::path::PathBuf;

fn home() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn override_dir() -> Option<PathBuf> {
    env::var_os("ALLIEDID_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = override_dir() {
        return dir;
    }
    if cfg!(windows) {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join("AlliedID")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/AlliedID")
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
            .join("allied-id")
    }
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = override_dir() {
        return dir;
    }
    if cfg!(windows) {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home)
            .join("AlliedID")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/AlliedID")
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local/share"))
            .join("allied-id")
    }
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn identity_file() -> PathBuf {
    config_dir().join("identity.json")
}

pub fn knowledge_dir() -> PathBuf {
    data_dir().join("knowledge")
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}
