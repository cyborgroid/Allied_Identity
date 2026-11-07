//! 온라인 엔진: Anthropic Claude Messages API (raw HTTP — Rust 공식 SDK가 없음).
//!
//! - 모델 기본값 `claude-opus-5-5`, adaptive thinking, `output_config.effort`
//! - 안전 분류기가 요청을 거절하면 서버 측 fallback(`fallbacks: "default"`)이 다른 모델로 재시도
//! - 429/5xx 는 재시도, 연결 실패는 즉시 반환해 로컬 엔진으로 넘어가게 한다

use super::{Engine, EngineError, Reply, Role, Turn};
use crate::config::ClaudeConfig;
use serde_json::{json, Value};
use std::thread;
use std::time::Duration;

const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_ATTEMPTS: u32 = 3;

pub struct ClaudeEngine {
    cfg: ClaudeConfig,
    api_key: String,
    agent: ureq::Agent,
}

impl ClaudeEngine {
    /// API 키 환경 변수가 없으면 `None`.
    pub fn from_config(cfg: ClaudeConfig) -> Option<Self> {
        let api_key = std::env::var(&cfg.api_key_env).ok().filter(|k| !k.trim().is_empty())?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build();
        Some(Self { cfg, api_key, agent })
    }

    pub fn build_body(&self, system: &str, history: &[Turn]) -> Value {
        let messages: Vec<Value> = history
            .iter()
            .map(|t| match t.role {
                Role::User => json!({"role": "user", "content": t.text}),
                Role::Assistant => json!({
                    "role": "assistant",
                    "content": t.claude_content.clone()
                        .unwrap_or_else(|| json!([{"type": "text", "text": t.text}])),
                }),
            })
            .collect();
        json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "system": system,
            "messages": messages,
            "thinking": {"type": "adaptive"},
            "output_config": {"effort": self.cfg.effort},
            "fallbacks": "default",
            "cache_control": {"type": "ephemeral"},
        })
    }

    fn send(&self, body: &Value) -> Result<Value, EngineError> {
        let url = format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'));
        let mut attempt = 0;
        loop {
            attempt += 1;
            let result = self
                .agent
                .post(&url)
                .set("x-api-key", &self.api_key)
                .set("anthropic-version", API_VERSION)
                .set("anthropic-beta", FALLBACK_BETA)
                .send_json(body.clone());
            match result {
                Ok(resp) => return resp.into_json().map_err(|e| EngineError::Api(e.to_string())),
                Err(ureq::Error::Status(code, resp)) => {
                    let retry_after = resp.header("retry-after").and_then(|v| v.parse::<u64>().ok());
                    let text = resp.into_string().unwrap_or_default();
                    let retryable = code == 429 || code >= 500;
                    if retryable && attempt < MAX_ATTEMPTS {
                        let wait = retry_after.unwrap_or(2u64.pow(attempt)).min(30);
                        thread::sleep(Duration::from_secs(wait));
                        continue;
                    }
                    let message = serde_json::from_str::<Value>(&text)
                        .ok()
                        .and_then(|v| v["error"]["message"].as_str().map(String::from))
                        .unwrap_or(text);
                    return Err(match code {
                        401 | 403 => EngineError::Unavailable(format!("인증 실패 (HTTP {code}): {message}")),
                        _ => EngineError::Api(format!("HTTP {code}: {message}")),
                    });
                }
                Err(ureq::Error::Transport(t)) => return Err(EngineError::Network(t.to_string())),
            }
        }
    }
}

impl Engine for ClaudeEngine {
    fn label(&self) -> String {
        format!("claude ({}, effort {})", self.cfg.model, self.cfg.effort)
    }

    fn reply(&self, system: &str, history: &[Turn]) -> Result<Reply, EngineError> {
        let resp = self.send(&self.build_body(system, history))?;
        let content = resp["content"].clone();
        let text: String = content
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let note = match resp["stop_reason"].as_str() {
            Some("refusal") => Some(format!(
                "Claude가 이 요청을 거절했습니다 (분류: {}).",
                resp["stop_details"]["category"].as_str().unwrap_or("미상")
            )),
            Some("max_tokens") => Some("응답이 max_tokens 에서 잘렸습니다.".into()),
            _ => None,
        };
        Ok(Reply { text: text.trim().to_string(), claude_content: Some(content), note })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ClaudeEngine {
        ClaudeEngine {
            cfg: ClaudeConfig::default(),
            api_key: "test".into(),
            agent: ureq::agent(),
        }
    }

    #[test]
    fn body_replays_claude_content_and_wraps_local_text() {
        let raw = json!([{"type": "thinking", "thinking": "", "signature": "s"}, {"type": "text", "text": "안녕"}]);
        let history = vec![
            Turn::user("hi"),
            Turn { role: Role::Assistant, text: "안녕".into(), claude_content: Some(raw.clone()) },
            Turn::user("again"),
            Turn { role: Role::Assistant, text: "local reply".into(), claude_content: None },
            Turn::user("third"),
        ];
        let body = engine().build_body("sys", &history);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["messages"][1]["content"], raw);
        assert_eq!(body["messages"][3]["content"][0]["text"], "local reply");
        assert_eq!(body["messages"].as_array().unwrap().len(), 5);
    }
}
