//! 대화 세션: 인격체 + 지식 팩 + 네트워크 상태 + 엔진 라우팅.

use crate::config::{Config, EngineMode};
use crate::engine::claude::ClaudeEngine;
use crate::engine::local::LocalEngine;
use crate::engine::{Engine, Role, Turn};
use crate::identity::{Identity, RuntimeContext};
use crate::knowledge::{self, KnowledgeBase};
use crate::net::{self, NetReport};
use std::time::{Duration, Instant};

pub struct Answer {
    pub engine: String,
    pub text: String,
    pub notes: Vec<String>,
    pub sources: Vec<String>,
}

pub struct Session {
    pub cfg: Config,
    pub identity: Identity,
    pub kb: KnowledgeBase,
    pub net: NetReport,
    pub mode: EngineMode,
    history: Vec<Turn>,
    claude: Option<ClaudeEngine>,
    local: LocalEngine,
    last_scan: Instant,
}

impl Session {
    pub fn new(cfg: Config) -> Result<Self, String> {
        let identity = Identity::load_or_default(&cfg.identity_path())?;
        let kb = if cfg.knowledge.enabled {
            KnowledgeBase::load_dir(&cfg.knowledge_path())?
        } else {
            KnowledgeBase::default()
        };
        let net = net::scan(&cfg.net);
        Ok(Self {
            claude: ClaudeEngine::from_config(cfg.claude.clone()),
            local: LocalEngine::new(cfg.local.clone()),
            mode: cfg.engine,
            identity,
            kb,
            net,
            history: Vec::new(),
            last_scan: Instant::now(),
            cfg,
        })
    }

    pub fn has_claude(&self) -> bool {
        self.claude.is_some()
    }

    pub fn local(&self) -> &LocalEngine {
        &self.local
    }

    pub fn rescan(&mut self) {
        self.net = net::scan(&self.cfg.net);
        self.last_scan = Instant::now();
    }

    pub fn clear(&mut self) {
        self.history.clear();
    }

    pub fn save_identity(&self) -> Result<(), String> {
        self.identity.save(&self.cfg.identity_path())
    }

    /// 이번 차례에 시도할 엔진 순서.
    fn engines(&self) -> Vec<&dyn Engine> {
        let claude = self.claude.as_ref().map(|c| c as &dyn Engine);
        let local = &self.local as &dyn Engine;
        match self.mode {
            EngineMode::Local => vec![local],
            EngineMode::Claude => claude.into_iter().collect(),
            EngineMode::Auto if self.net.online => claude.into_iter().chain([local]).collect(),
            EngineMode::Auto => vec![local],
        }
    }

    pub fn ask(&mut self, input: &str) -> Result<Answer, String> {
        if self.last_scan.elapsed() > Duration::from_secs(self.cfg.net.rescan_secs) {
            self.rescan();
        }

        let hits = self.kb.search(input, self.cfg.knowledge.top_k);
        let sources: Vec<String> = hits
            .iter()
            .enumerate()
            .map(|(i, h)| format!("[{}] {} — {}", i + 1, h.doc.title, h.doc.source))
            .collect();
        let content = if hits.is_empty() {
            input.to_string()
        } else {
            format!("{}{input}", knowledge::format_context(&hits))
        };

        self.history.push(Turn::user(content));
        let mut notes = Vec::new();
        let summary = self.net.summary();

        for engine in self.engines() {
            let label = engine.label();
            let system = self.identity.system_prompt(&RuntimeContext {
                engine: &label,
                network_summary: &summary,
                warnings: &self.net.warnings,
                knowledge_available: !self.kb.is_empty(),
            });
            match engine.reply(&system, &self.history) {
                Ok(reply) => {
                    notes.extend(reply.note);
                    self.history.push(Turn {
                        role: Role::Assistant,
                        text: reply.text.clone(),
                        claude_content: reply.claude_content,
                    });
                    return Ok(Answer { engine: label, text: reply.text, notes, sources });
                }
                // 어떤 오류든 기록하고 다음 엔진으로 넘어간다.
                Err(e) => notes.push(format!("{label}: {e}")),
            }
        }

        self.history.pop();
        if notes.is_empty() {
            notes.push(format!(
                "엔진 모드 '{}' 에서 쓸 수 있는 엔진이 없습니다. Claude는 {} 환경 변수가 필요합니다.",
                self.mode.as_str(),
                self.cfg.claude.api_key_env
            ));
        }
        Err(notes.join("\n"))
    }
}
