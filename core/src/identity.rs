//! 인격체: 이름, 페르소나, 원칙(헌법), 사용자, 기억.
//! 파일 저장은 C# 호스트가 맡고, 엔진은 사본을 받아 생각에 쓴다.

use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Identity {
    pub schema_version: u32,
    pub name: String,
    pub name_ko: String,
    pub callsign: String,
    pub persona: Persona,
    pub principles: Vec<String>,
    pub owner: Owner,
    pub memories: Vec<Memory>,
}

impl Identity {
    /// 호칭: 이름을 알면 " 민준님", 모르면 빈 문자열.
    pub fn owner_call(&self) -> String {
        self.owner.name.as_deref().map(|n| format!(" {n}님")).unwrap_or_default()
    }

    /// 인격체 자신에 대한 지식 문서 (자기 인식).
    pub fn self_description(&self) -> String {
        let mut s = format!(
            "저는 {}({})이고 호출명은 {}입니다. {}\n말투는 {}입니다.\n",
            self.name, self.name_ko, self.callsign, self.persona.description, self.persona.tone
        );
        if !self.principles.is_empty() {
            s.push_str("\n제 원칙은 다음과 같습니다.\n");
            for p in &self.principles {
                s.push_str(&format!("{p}\n"));
            }
        }
        s
    }
}
