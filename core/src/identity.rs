//! 인격체 프로필(identity.json): 페르소나, 원칙, 사용자 정보, 기억.
//! 형식은 `schemas/identity.schema.json`, 기본값은 `schemas/identity.default.json`.

use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const DEFAULT_IDENTITY: &str = include_str!("../../schemas/identity.default.json");

/// 시스템 프롬프트에 넣을 최근 기억 개수.
const PROMPT_MEMORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Persona {
    pub description: String,
    pub tone: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Owner {
    pub name: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub text: String,
    pub created_at: i64,
    #[serde(default)]
    pub tags: Vec<String>,
}

// 컨테이너 수준 `#[serde(default)]` 는 쓰지 않는다: `Default` 가 이 타입을 다시
// 역직렬화하므로 무한 재귀가 된다. 필드별 기본값만 사용한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    #[serde(default = "schema_v1")]
    pub schema_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub name_ko: String,
    #[serde(default)]
    pub callsign: String,
    #[serde(default)]
    pub persona: Persona,
    #[serde(default)]
    pub principles: Vec<String>,
    #[serde(default)]
    pub owner: Owner,
    #[serde(default)]
    pub memories: Vec<Memory>,
}

fn schema_v1() -> u32 {
    1
}

impl Default for Identity {
    fn default() -> Self {
        serde_json::from_str(DEFAULT_IDENTITY).expect("schemas/identity.default.json must be valid")
    }
}

/// 시스템 프롬프트에 들어갈 실행 상태.
pub struct RuntimeContext<'a> {
    pub engine: &'a str,
    pub network_summary: &'a str,
    pub warnings: &'a [String],
    pub knowledge_available: bool,
}

impl Identity {
    pub fn load_or_default(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fsutil::write_atomic(path, json.as_bytes())
    }

    pub fn remember(&mut self, text: &str, tags: Vec<String>) -> &Memory {
        self.memories.push(Memory {
            id: format!("m{:x}", fsutil::unix_nanos()),
            text: text.trim().to_string(),
            created_at: fsutil::unix_now(),
            tags,
        });
        self.memories.last().expect("just pushed")
    }

    pub fn forget(&mut self, id: &str) -> bool {
        let before = self.memories.len();
        self.memories.retain(|m| m.id != id);
        self.memories.len() != before
    }

    pub fn system_prompt(&self, ctx: &RuntimeContext) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "당신은 {} ({}), 호출명 {} 입니다.\n{}\n말투: {}. 기본 언어: {}.\n",
            self.name,
            self.name_ko,
            self.callsign,
            self.persona.description,
            self.persona.tone,
            self.persona.language
        ));

        if !self.principles.is_empty() {
            out.push_str("\n## 원칙\n");
            for p in &self.principles {
                out.push_str(&format!("- {p}\n"));
            }
        }

        if self.owner.name.is_some() || self.owner.notes.is_some() {
            out.push_str("\n## 사용자\n");
            if let Some(name) = &self.owner.name {
                out.push_str(&format!("- 이름: {name}\n"));
            }
            if let Some(notes) = &self.owner.notes {
                out.push_str(&format!("- 메모: {notes}\n"));
            }
        }

        if !self.memories.is_empty() {
            out.push_str("\n## 기억\n");
            let start = self.memories.len().saturating_sub(PROMPT_MEMORY_LIMIT);
            for m in &self.memories[start..] {
                out.push_str(&format!("- {}\n", m.text));
            }
        }

        out.push_str("\n## 현재 상태\n");
        out.push_str(&format!("- 엔진: {}\n", ctx.engine));
        out.push_str(&format!("- 네트워크: {}\n", ctx.network_summary));
        for w in ctx.warnings {
            out.push_str(&format!("- 주의: {w}\n"));
        }
        if ctx.knowledge_available {
            out.push_str(
                "- 사용자 메시지 앞의 <참고자료> 블록은 기기에 내장된 오프라인 지식 팩에서 찾은 내용이다. \
                 관련 있을 때만 사용하고, 사용했다면 [번호]로 출처를 밝힌다.\n",
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_identity_parses() {
        let id = Identity::default();
        assert_eq!(id.callsign, "AlliedID");
        assert!(!id.principles.is_empty());
    }

    #[test]
    fn remember_and_forget() {
        let mut id = Identity::default();
        let mem_id = id.remember("  커피는 아메리카노  ", vec![]).id.clone();
        assert_eq!(id.memories[0].text, "커피는 아메리카노");
        assert!(id.forget(&mem_id));
        assert!(!id.forget(&mem_id));
    }

    #[test]
    fn prompt_contains_state() {
        let mut id = Identity::default();
        id.owner.name = Some("민준".into());
        id.remember("매일 아침 러닝을 한다", vec![]);
        let warnings = vec!["개방형 Wi-Fi".to_string()];
        let p = id.system_prompt(&RuntimeContext {
            engine: "local",
            network_summary: "오프라인",
            warnings: &warnings,
            knowledge_available: true,
        });
        assert!(p.contains("민준"));
        assert!(p.contains("매일 아침 러닝"));
        assert!(p.contains("오프라인"));
        assert!(p.contains("개방형 Wi-Fi"));
    }
}
