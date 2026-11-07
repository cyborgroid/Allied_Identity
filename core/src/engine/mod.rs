//! 추론 엔진.
//!
//! - `local`  : llama.cpp `llama-server` 로 기기 안에서 돌리는 오픈 웨이트 모델 (오프라인)
//! - `claude` : Anthropic Claude API (온라인)
//!
//! 두 엔진은 같은 인격체 프로필(시스템 프롬프트)과 대화 기록을 공유한다.

pub mod claude;
pub mod local;

use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub role: Role,
    pub text: String,
    /// Claude가 만든 응답의 원본 content 블록. 다음 요청에 그대로 다시 보낸다(대화는 덧붙이기만 함).
    pub claude_content: Option<Value>,
}

impl Turn {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, text: text.into(), claude_content: None }
    }
}

pub struct Reply {
    pub text: String,
    pub claude_content: Option<Value>,
    pub note: Option<String>,
}

#[derive(Debug)]
pub enum EngineError {
    /// 엔진을 쓸 수 없음(키 없음, 로컬 서버 꺼짐 등). 다음 엔진으로 넘어간다.
    Unavailable(String),
    /// 네트워크 문제. 다음 엔진으로 넘어간다.
    Network(String),
    /// API가 오류를 돌려줌.
    Api(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) => write!(f, "사용 불가: {m}"),
            Self::Network(m) => write!(f, "네트워크 오류: {m}"),
            Self::Api(m) => write!(f, "API 오류: {m}"),
        }
    }
}

pub trait Engine {
    fn label(&self) -> String;
    fn reply(&self, system: &str, history: &[Turn]) -> Result<Reply, EngineError>;
}
