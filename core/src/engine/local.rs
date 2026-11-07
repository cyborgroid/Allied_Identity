//! 오프라인 엔진: llama.cpp `llama-server` (기본 http://127.0.0.1:8080).
//! 인터넷 없이 기기 안의 GGUF 모델로 답한다. `scripts/start-local-engine.sh` 참고.

use super::{Engine, EngineError, Reply, Role, Turn};
use crate::config::LocalConfig;
use serde_json::{json, Value};
use std::time::Duration;

pub struct LocalEngine {
    cfg: LocalConfig,
    agent: ureq::Agent,
}

impl LocalEngine {
    pub fn new(cfg: LocalConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(2))
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build();
        Self { cfg, agent }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.endpoint.trim_end_matches('/'), path)
    }

    /// llama-server 의 `/health` 가 응답하면 사용 가능.
    pub fn health(&self) -> Result<(), String> {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(2))
            .build()
            .get(&self.url("/health"))
            .call()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn endpoint(&self) -> &str {
        &self.cfg.endpoint
    }
}

impl Engine for LocalEngine {
    fn label(&self) -> String {
        format!("local ({})", self.cfg.endpoint)
    }

    fn reply(&self, system: &str, history: &[Turn]) -> Result<Reply, EngineError> {
        let mut messages = vec![json!({"role": "system", "content": system})];
        messages.extend(history.iter().map(|t| {
            json!({
                "role": if t.role == Role::User { "user" } else { "assistant" },
                "content": t.text,
            })
        }));
        let body = json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "messages": messages,
        });

        let resp: Value = match self.agent.post(&self.url("/v1/chat/completions")).send_json(body) {
            Ok(r) => r.into_json().map_err(|e| EngineError::Api(e.to_string()))?,
            Err(ureq::Error::Status(code, r)) => {
                return Err(EngineError::Api(format!("HTTP {code}: {}", r.into_string().unwrap_or_default())))
            }
            Err(ureq::Error::Transport(t)) => {
                return Err(EngineError::Unavailable(format!(
                    "로컬 엔진이 {} 에서 실행 중이 아닙니다 ({t}). scripts/start-local-engine.sh 로 시작하세요.",
                    self.cfg.endpoint
                )))
            }
        };

        let text = resp["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| EngineError::Api(format!("예상하지 못한 응답: {resp}")))?
            .trim()
            .to_string();
        let note = (resp["choices"][0]["finish_reason"] == "length")
            .then(|| "응답이 max_tokens 에서 잘렸습니다.".to_string());
        Ok(Reply { text, claude_content: None, note })
    }
}
