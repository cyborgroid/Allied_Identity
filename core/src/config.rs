//! `config.json` 설정. 파일이 없거나 일부 항목이 빠져 있으면 기본값을 쓴다.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EngineMode {
    /// 온라인이고 API 키가 있으면 Claude, 아니면 로컬 엔진.
    Auto,
    /// 항상 로컬(오프라인) 엔진.
    Local,
    /// 항상 Claude 엔진.
    Claude,
}

impl EngineMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "local" | "offline" => Some(Self::Local),
            "claude" | "online" => Some(Self::Claude),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Local => "local",
            Self::Claude => "claude",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    /// llama.cpp `llama-server` 주소.
    pub endpoint: String,
    pub model: String,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8080".into(),
            model: "local".into(),
            max_tokens: 2048,
            timeout_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeConfig {
    pub model: String,
    /// low | medium | high | xhigh | max
    pub effort: String,
    pub max_tokens: u32,
    /// API 키를 읽을 환경 변수 이름. 키 자체는 설정 파일에 저장하지 않는다.
    pub api_key_env: String,
    pub base_url: String,
    pub timeout_secs: u64,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-5-5".into(),
            effort: "high".into(),
            max_tokens: 16000,
            api_key_env: "ANTHROPIC_API_KEY".into(),
            base_url: "https://api.anthropic.com".into(),
            timeout_secs: 600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetConfig {
    /// 온라인 여부 판단용 TCP 접속 대상 (host:port).
    pub probe_targets: Vec<String>,
    pub probe_timeout_ms: u64,
    /// 네트워크 상태를 다시 확인하는 주기(초).
    pub rescan_secs: u64,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            probe_targets: vec!["api.anthropic.com:443".into(), "1.1.1.1:443".into()],
            probe_timeout_ms: 1500,
            rescan_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    pub enabled: bool,
    pub dir: Option<PathBuf>,
    pub top_k: usize,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: None,
            top_k: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub engine: EngineMode,
    pub local: LocalConfig,
    pub claude: ClaudeConfig,
    pub net: NetConfig,
    pub knowledge: KnowledgeConfig,
    pub identity_file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            engine: EngineMode::Auto,
            local: LocalConfig::default(),
            claude: ClaudeConfig::default(),
            net: NetConfig::default(),
            knowledge: KnowledgeConfig::default(),
            identity_file: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = paths::config_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = paths::config_file();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::fsutil::write_atomic(&path, json.as_bytes())?;
        Ok(path)
    }

    pub fn identity_path(&self) -> PathBuf {
        self.identity_file.clone().unwrap_or_else(paths::identity_file)
    }

    pub fn knowledge_path(&self) -> PathBuf {
        self.knowledge.dir.clone().unwrap_or_else(paths::knowledge_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_config_fills_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"engine":"local","claude":{"effort":"max"}}"#).unwrap();
        assert_eq!(cfg.engine, EngineMode::Local);
        assert_eq!(cfg.claude.effort, "max");
        assert_eq!(cfg.claude.model, "claude-opus-5-5");
        assert_eq!(cfg.local.endpoint, "http://127.0.0.1:8080");
    }

    #[test]
    fn engine_mode_aliases() {
        assert_eq!(EngineMode::parse("offline"), Some(EngineMode::Local));
        assert_eq!(EngineMode::parse("Claude"), Some(EngineMode::Claude));
        assert_eq!(EngineMode::parse("x"), None);
    }
}
