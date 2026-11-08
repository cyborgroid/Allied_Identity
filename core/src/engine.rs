//! 본체 인지 엔진 — Claude의 구조를 본떠 재설계한 추론 루프.
//!
//!   메시지 → [생각] 의도 추정·근거 검색·계획 → (필요하면) tool_use 로 호스트에 관찰 요청
//!          → tool_result 수신 → [작성] 답변 → [원칙 점검] 근거 없는 말은 하지 않음
//!
//! 응답 형식도 Claude Messages API 와 같은 모양이다:
//!   { stop_reason: "end_turn" | "tool_use", content: [thinking, text | tool_use] }

use crate::calc;
use crate::compose;
use crate::forge::{self, DataDir, Example, ForgeStats, IntentSpec, Learned};
use crate::identity::{Identity, Memory};
use crate::intent::IntentModel;
use crate::knowledge::{Doc, KnowledgeBase};
use crate::text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// 이보다 확신이 낮으면 의도 대신 지식 검색으로 답한다.
const CONFIDENCE_MIN: f64 = 0.5;
/// 가르친 질문과 이만큼 비슷하면 가르친 답을 쓴다.
const LEARNED_MIN: f64 = 0.6;
/// 지식 검색 점수가 이보다 낮으면 "모른다"고 답한다.
const EVIDENCE_MIN: f64 = 1.5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

impl Effort {
    /// (참고할 문서 수, 답에 쓸 문장 수)
    fn depth(self) -> (usize, usize) {
        match self {
            Effort::Low => (1, 2),
            Effort::Medium => (2, 3),
            Effort::High => (4, 5),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    #[serde(default)]
    pub effort: Effort,
    #[serde(default)]
    pub identity: Option<Identity>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Forge {
        #[serde(default)]
        environment: Vec<Doc>,
    },
    Message {
        text: String,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
    Teach {
        question: String,
        answer: String,
    },
    Feedback {
        good: bool,
    },
    Identity {
        identity: Identity,
    },
    Effort {
        effort: Effort,
    },
    Status,
    Reset,
}

struct Pending {
    id: String,
    tool: String,
    intent: String,
    text: String,
    confidence: f64,
}

/// "더 알려줘"에 이어서 답하기 위한 직전 지식 답변.
struct LastKnowledge {
    question: String,
    query: HashSet<String>,
    docs: Vec<usize>,
    used: HashSet<String>,
}

struct LastTurn {
    text: String,
    intent: String,
}

pub struct Engine {
    data: DataDir,
    effort: Effort,
    identity: Identity,
    specs: Vec<IntentSpec>,
    model: IntentModel,
    kb: KnowledgeBase,
    learned: Vec<Learned>,
    confirmed: Vec<Example>,
    stats: ForgeStats,
    pending: Option<Pending>,
    last_knowledge: Option<LastKnowledge>,
    last_turn: Option<LastTurn>,
    seq: u64,
    rng: u64,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Engine {
    pub fn open(cfg: Config) -> Result<Self, String> {
        let data = DataDir { root: cfg.data_dir };
        let specs = forge::load_intents(&data)?;
        let identity = match cfg.identity {
            Some(id) => id,
            None => {
                let p = data.seed().join("identity.default.json");
                let t = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display()))?
            }
        };
        let learned = forge::read_jsonl(&data.state().join("learned.jsonl"));
        let confirmed = forge::read_jsonl(&data.state().join("confirmed.jsonl"));
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            | 1;
        Ok(Self {
            data,
            effort: cfg.effort,
            identity,
            specs,
            model: IntentModel::default(),
            kb: KnowledgeBase::default(),
            learned,
            confirmed,
            stats: ForgeStats::default(),
            pending: None,
            last_knowledge: None,
            last_turn: None,
            seq: 0,
            rng: seed,
        })
    }

    pub fn handle(&mut self, req: Request) -> Result<Value, String> {
        match req {
            Request::Forge { environment } => {
                let f = forge::forge(&self.data, &self.specs, &self.identity, &environment, &self.learned, &self.confirmed)?;
                self.kb = f.kb;
                self.model = f.model;
                self.stats = f.stats;
                Ok(serde_json::to_value(&self.stats).unwrap_or_default())
            }
            Request::Message { text } => Ok(self.message(&text)),
            Request::ToolResult { tool_use_id, content, is_error } => self.tool_result(&tool_use_id, content, is_error),
            Request::Teach { question, answer } => self.teach(&question, &answer),
            Request::Feedback { good } => self.feedback(good),
            Request::Identity { identity } => {
                self.identity = identity;
                Ok(json!({"ok": true}))
            }
            Request::Effort { effort } => {
                self.effort = effort;
                Ok(json!({"effort": effort}))
            }
            Request::Status => Ok(json!({
                "version": env!("CARGO_PKG_VERSION"),
                "effort": self.effort,
                "knowledge_docs": self.kb.len(),
                "training_examples": self.stats.training_examples,
                "intents": self.model.intents.len(),
                "vocabulary": self.model.vocab_size(),
                "learned": self.learned.len(),
                "confirmed": self.confirmed.len(),
                "memories": self.identity.memories.len(),
            })),
            Request::Reset => {
                self.pending = None;
                self.last_knowledge = None;
                self.last_turn = None;
                Ok(json!({"ok": true}))
            }
        }
    }

    // ── 생각 ────────────────────────────────────────────────────────────

    fn message(&mut self, input: &str) -> Value {
        self.pending = None;
        let input = input.trim();
        let mut think = Vec::new();

        if let Some((i, sim)) = self.match_learned(input) {
            think.push(format!("가르쳐 준 질문 \"{}\"와 일치 (유사도 {sim:.2})", self.learned[i].question));
            think.push("원칙 점검: 사용자가 가르친 근거로 답함".into());
            let answer = self.learned[i].answer.clone();
            return self.end_turn(input, "learned", sim, think, answer);
        }

        let pred = self.model.predict(input);
        let conf = pred.confidence();
        let intent = pred.top().map(|(i, _)| i.to_string()).unwrap_or_default();
        let alts: Vec<String> = pred.ranked.iter().skip(1).take(2).map(|(i, p)| format!("{i} {p:.2}")).collect();
        think.push(format!(
            "의도 추정: {intent} (확신 {conf:.2}, 어휘 적중 {:.0}%){}",
            pred.coverage * 100.0,
            if alts.is_empty() { String::new() } else { format!(" · 다음 후보: {}", alts.join(", ")) }
        ));

        let spec = self.specs.iter().find(|s| s.intent == intent).cloned();
        let Some(spec) = spec.filter(|_| conf >= CONFIDENCE_MIN) else {
            think.push("확신이 낮아 지식에서 근거를 찾음".into());
            return self.answer_from_knowledge(input, "knowledge_query", conf, think);
        };

        // "나에 대해 뭘 기억해?"처럼 저장할 내용 없이 의문어만 있으면 저장이 아니라 떠올리기다.
        if spec.intent == "memory_save" && text::keywords(&strip_markers(input, SAVE_MARKERS)).is_empty() {
            think.push("저장할 내용 없이 묻는 말이라 기억 떠올리기로 판단".into());
            return self.recall(input, conf, think);
        }

        match spec.kind.as_str() {
            "tool" => self.plan_tool(&spec, input, conf, think),
            "calc" => self.calculate(input, conf, think),
            "memory_recall" => self.recall(input, conf, think),
            "more" => self.more(input, conf, think),
            "knowledge" => self.answer_from_knowledge(input, &spec.intent, conf, think),
            _ => {
                think.push("계획: 인격체의 말로 바로 답함".into());
                let reply = self.pick(&spec.responses, &HashMap::new());
                self.end_turn(input, &spec.intent, conf, think, reply)
            }
        }
    }

    fn match_learned(&self, input: &str) -> Option<(usize, f64)> {
        let norm = text::normalize(input);
        let q: HashSet<String> = text::keywords(input).into_iter().collect();
        self.learned
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let sim = if text::normalize(&l.question) == norm {
                    1.0
                } else {
                    text::jaccard(&q, &text::keywords(&l.question).into_iter().collect())
                };
                (i, sim)
            })
            .filter(|(_, s)| *s >= LEARNED_MIN)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    // ── 도구 사용 (호스트에 관찰·행동 요청) ───────────────────────────────

    fn plan_tool(&mut self, spec: &IntentSpec, input: &str, conf: f64, mut think: Vec<String>) -> Value {
        let Some(tool) = spec.tool.clone() else {
            return self.end_turn(input, &spec.intent, conf, think, "이 일에 쓸 도구가 정해져 있지 않아요.".into());
        };
        let tool_input = match tool_input(&spec.intent, input) {
            Ok(v) => v,
            Err(ask) => {
                think.push("필요한 정보가 부족해 되물음".into());
                return self.end_turn(input, &spec.intent, conf, think, ask);
            }
        };
        self.seq += 1;
        let id = format!("tool_{}", self.seq);
        think.push(format!("계획: 도구 {tool} 호출 {tool_input}"));
        let value = json!({
            "stop_reason": "tool_use",
            "intent": spec.intent,
            "confidence": conf,
            "content": [
                {"type": "thinking", "thinking": think.join("\n")},
                {"type": "tool_use", "id": id, "name": tool, "input": tool_input},
            ],
        });
        self.pending = Some(Pending { id, tool, intent: spec.intent.clone(), text: input.into(), confidence: conf });
        value
    }

    fn tool_result(&mut self, id: &str, content: Value, is_error: bool) -> Result<Value, String> {
        let p = self.pending.take().ok_or("대기 중인 도구 호출이 없습니다")?;
        if p.id != id {
            return Err(format!("도구 호출 id 가 다릅니다 (기대 {}, 받음 {id})", p.id));
        }
        // 각 응답의 생각 블록에는 그 단계에서 새로 한 생각만 담는다 (계획은 tool_use 응답에 이미 있음).
        let mut think = vec![format!("관찰: {} 결과 수신{}", p.tool, if is_error { " (오류)" } else { "" })];
        if is_error {
            let msg = content.as_str().map(String::from).unwrap_or_else(|| content.to_string());
            return Ok(self.end_turn(&p.text, &p.intent, p.confidence, think, format!("{} 도구를 실행하지 못했어요: {msg}", p.tool)));
        }

        let reply = match p.tool.as_str() {
            "net.status" => {
                let mut s = compose::network(&content);
                // 관찰 결과에 더해, 질문 자체와 관련된 지식이 있으면 근거로 덧붙인다 ("개방형 와이파이 위험해?").
                if let Some(tip) = self.related_knowledge(&p.text, &mut think) {
                    s.push_str(&format!("\n\n참고: {tip}"));
                } else if content["wifi"]["open"].as_bool().unwrap_or(false) {
                    if let Some(tip) = self.best_sentence("개방형 Wi-Fi 안전 VPN HTTPS") {
                        s.push_str(&format!("\n\n참고: {tip}"));
                    }
                }
                s
            }
            "system.info" => compose::system(&content),
            "project.files" => compose::files(&content),
            "memory.save" => match serde_json::from_value::<Memory>(content["memory"].clone()) {
                Ok(m) => {
                    let t = m.text.clone();
                    self.identity.memories.push(m);
                    format!("기억했어요: \"{t}\"")
                }
                Err(_) => "기억을 저장했다는 확인을 받지 못했어요.".into(),
            },
            "memory.forget" => {
                let removed: Vec<String> = content["removed"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect();
                self.identity.memories.retain(|m| !removed.contains(&m.id));
                if removed.is_empty() {
                    "그런 기억은 찾지 못했어요. 기억 목록은 /memories 로 볼 수 있어요.".into()
                } else {
                    format!("기억 {}개를 잊었어요.", removed.len())
                }
            }
            "identity.set_owner" => {
                let name = content["name"].as_str().unwrap_or("").to_string();
                self.identity.owner.name = Some(name.clone()).filter(|n| !n.is_empty());
                format!("반가워요, {name}님! 앞으로 그렇게 부를게요.")
            }
            _ => {
                let spec = self.specs.iter().find(|s| s.intent == p.intent).cloned();
                let extra = compose::vars_from(&content);
                let extra_ref: HashMap<&str, String> = extra.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                match spec {
                    Some(s) if !s.responses.is_empty() => self.pick(&s.responses, &extra_ref),
                    _ => content.to_string(),
                }
            }
        };
        think.push("원칙 점검: 직접 관찰한 결과로 답함".into());
        Ok(self.end_turn(&p.text, &p.intent, p.confidence, think, reply))
    }

    // ── 내부 능력 ───────────────────────────────────────────────────────

    fn calculate(&mut self, input: &str, conf: f64, mut think: Vec<String>) -> Value {
        let reply = match calc::extract(input) {
            None => "계산할 식을 찾지 못했어요. 예: 12*(3+4), 100 나누기 8".into(),
            Some(expr) => {
                think.push(format!("계획: 식 \"{expr}\" 계산"));
                match calc::eval(&expr) {
                    Ok(v) => format!("{} = {}", expr.split_whitespace().collect::<String>(), calc::format_number(v)),
                    Err(e) => format!("계산하지 못했어요: {e}"),
                }
            }
        };
        self.end_turn(input, "calc", conf, think, reply)
    }

    fn recall(&mut self, input: &str, conf: f64, mut think: Vec<String>) -> Value {
        think.push(format!("기억 {}개 확인", self.identity.memories.len()));
        let mut lines = Vec::new();
        if let Some(name) = &self.identity.owner.name {
            lines.push(format!("당신의 이름은 {name}{}.", compose::eyo(name)));
        }
        for m in self.identity.memories.iter().rev().take(10).rev() {
            lines.push(format!("- {}", m.text));
        }
        let reply = if lines.is_empty() {
            "아직 기억하는 게 없어요. \"기억해: 나는 아침형 인간이야\"처럼 말해 주세요.".into()
        } else {
            format!("제가 기억하는 건 이거예요.\n{}", lines.join("\n"))
        };
        self.end_turn(input, "memory_recall", conf, think, reply)
    }

    fn answer_from_knowledge(&mut self, input: &str, intent: &str, conf: f64, mut think: Vec<String>) -> Value {
        let (k, n_sentences) = self.effort.depth();
        let hits = self.kb.search(input, k);
        let best = hits.first().map(|h| h.score).unwrap_or(0.0);
        think.push(format!("지식 검색: {}건 (최고 점수 {best:.2}, 노력 {:?})", hits.len(), self.effort));

        if best < EVIDENCE_MIN {
            think.push("원칙 점검: 근거가 부족해 지어내지 않음".into());
            self.last_knowledge = None;
            let q = text::clip(input, 40);
            let reply = format!(
                "\"{q}\"에 대해서는 아직 잘 모르겠어요. 제 데이터에 근거가 없는 내용은 지어내지 않을게요.\n가르쳐 주시면 배울게요:  /teach {q} => (답)"
            );
            return self.end_turn(input, intent, conf, think, reply);
        }

        let query = text::token_set(input);
        let docs: Vec<usize> = hits.iter().filter(|h| h.score >= best * 0.4).map(|h| h.index).collect();
        let mut used = HashSet::new();
        let body = self.select_sentences(&query, &docs, n_sentences, &mut used);
        let sources: Vec<String> = docs
            .iter()
            .enumerate()
            .map(|(i, &d)| format!("[{}] {} · {}", i + 1, self.kb.docs[d].title, self.kb.docs[d].source))
            .collect();
        think.push(format!("작성: 문서 {}개에서 문장 {}개 발췌", docs.len(), used.len()));
        think.push("원칙 점검: 출처를 밝힘".into());
        self.last_knowledge = Some(LastKnowledge { question: input.into(), query, docs, used });
        let reply = format!("{body}\n\n출처: {}", sources.join("  "));
        self.end_turn(input, intent, conf, think, reply)
    }

    fn more(&mut self, input: &str, conf: f64, mut think: Vec<String>) -> Value {
        let Some(mut last) = self.last_knowledge.take() else {
            think.push("이어갈 직전 지식 답변이 없음".into());
            return self.end_turn(input, "more", conf, think, "무엇에 대해 더 알려 드릴까요? 먼저 궁금한 것을 물어봐 주세요.".into());
        };
        let (k, n) = self.effort.depth();
        let before = last.used.len();
        let mut body = self.select_sentences(&last.query.clone(), &last.docs.clone(), n, &mut last.used);
        if body.is_empty() {
            // 직전 문서를 다 썼으면 같은 질문으로 더 넓게 찾아 관련 문서를 더한다.
            let wider: Vec<usize> = self
                .kb
                .search(&last.question, k + 3)
                .iter()
                .filter(|h| h.score >= EVIDENCE_MIN && !last.docs.contains(&h.index))
                .map(|h| h.index)
                .collect();
            think.push(format!("직전 문서를 다 써서 관련 문서 {}개로 넓힘", wider.len()));
            body = self.select_sentences(&last.query.clone(), &wider, n, &mut last.used);
            last.docs.extend(wider);
        }
        think.push(format!("새 문장 {}개", last.used.len() - before));
        let reply = if body.is_empty() { "그 주제에 대해서는 제가 가진 내용을 다 말씀드렸어요.".into() } else { body };
        self.last_knowledge = Some(last);
        self.end_turn(input, "more", conf, think, reply)
    }

    /// 질문 토큰과 많이 겹치는 문장을 고른다. 이미 쓴 문장은 건너뛰고, 원래 순서를 지킨다.
    fn select_sentences(&self, query: &HashSet<String>, docs: &[usize], n: usize, used: &mut HashSet<String>) -> String {
        let mut cands: Vec<(f64, usize, usize, String)> = Vec::new();
        for (rank, &d) in docs.iter().enumerate() {
            for (pos, s) in text::sentences(&self.kb.docs[d].text).into_iter().enumerate() {
                if used.contains(&s) {
                    continue;
                }
                let toks = text::token_set(&s);
                let overlap = toks.intersection(query).count() as f64;
                let score = overlap / (toks.len() as f64).sqrt().max(1.0) + 0.3 / (rank + 1) as f64 - 0.01 * pos as f64;
                cands.push((score, rank, pos, s));
            }
        }
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut chosen: Vec<_> = cands.into_iter().take(n).collect();
        chosen.sort_by_key(|c| (c.1, c.2));
        chosen
            .into_iter()
            .map(|c| {
                used.insert(c.3.clone());
                c.3
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// 질문과 강하게 관련된 씨앗·가르침 지식(환경 관찰 제외)을 찾아 한두 문장과 출처로 돌려준다.
    /// 찾으면 "더 알려줘"로 이어갈 수 있게 직전 지식으로 기록한다.
    fn related_knowledge(&mut self, question: &str, think: &mut Vec<String>) -> Option<String> {
        let (k, _) = self.effort.depth();
        let docs: Vec<usize> = self
            .kb
            .search(question, k + 2)
            .iter()
            .filter(|h| h.score >= EVIDENCE_MIN * 2.0 && h.doc.source != "환경 관찰")
            .take(k)
            .map(|h| h.index)
            .collect();
        let first = *docs.first()?;
        let query = text::token_set(question);
        let mut used = HashSet::new();
        let body = self.select_sentences(&query, &docs[..1], 2, &mut used);
        if body.is_empty() {
            return None;
        }
        think.push(format!("질문과 관련된 지식도 덧붙임: {}", self.kb.docs[first].title));
        let source = format!("[{}] {} · {}", 1, self.kb.docs[first].title, self.kb.docs[first].source);
        self.last_knowledge = Some(LastKnowledge { question: question.into(), query, docs, used });
        Some(format!("{body}\n출처: {source}"))
    }

    fn best_sentence(&self, query: &str) -> Option<String> {
        let hits = self.kb.search(query, 1);
        let h = hits.first().filter(|h| h.score >= EVIDENCE_MIN)?;
        let mut used = HashSet::new();
        Some(self.select_sentences(&text::token_set(query), &[h.index], 1, &mut used)).filter(|s| !s.is_empty())
    }

    // ── 배움 ────────────────────────────────────────────────────────────

    fn teach(&mut self, question: &str, answer: &str) -> Result<Value, String> {
        let (q, a) = (question.trim(), answer.trim());
        if q.is_empty() || a.is_empty() {
            return Err("질문과 답이 모두 필요합니다".into());
        }
        let l = Learned { question: q.into(), answer: a.into(), created_at: now() };
        forge::append_jsonl(&self.data.state().join("learned.jsonl"), &l)?;
        self.kb.add(Doc { id: format!("learned:{}", self.learned.len()), title: q.into(), source: "가르침".into(), text: a.into() });
        self.learned.push(l);
        Ok(json!({"text": format!("배웠어요! 이제 \"{q}\"라고 물으면 그렇게 답할게요."), "learned": self.learned.len()}))
    }

    fn feedback(&mut self, good: bool) -> Result<Value, String> {
        let Some(last) = &self.last_turn else {
            return Ok(json!({"text": "평가할 직전 답변이 없어요."}));
        };
        let trainable = self.specs.iter().any(|s| s.intent == last.intent);
        if good && trainable {
            let ex = Example { intent: last.intent.clone(), text: last.text.clone() };
            forge::append_jsonl(&self.data.state().join("confirmed.jsonl"), &ex)?;
            for v in forge::augment(&ex.text) {
                self.model.learn(&ex.intent, &v);
            }
            self.confirmed.push(ex);
            Ok(json!({"text": "좋아요. 이런 말은 이렇게 이해하는 게 맞다고 배웠어요."}))
        } else if good {
            Ok(json!({"text": "고마워요!"}))
        } else {
            forge::append_jsonl(
                &self.data.state().join("feedback.jsonl"),
                &json!({"text": last.text, "intent": last.intent, "good": false, "at": now()}),
            )?;
            Ok(json!({"text": format!("알려 주셔서 고마워요. 올바른 답을 가르쳐 주세요:  /teach {} => (답)", text::clip(&last.text, 40))}))
        }
    }

    // ── 작성 ────────────────────────────────────────────────────────────

    fn vars(&self) -> HashMap<&'static str, String> {
        let id = &self.identity;
        HashMap::from([
            ("name", id.name.clone()),
            ("name_ko", id.name_ko.clone()),
            ("callsign", id.callsign.clone()),
            ("description", id.persona.description.clone()),
            ("owner_call", id.owner_call()),
            ("kb_docs", self.kb.len().to_string()),
            ("examples", self.stats.training_examples.to_string()),
            ("learned", self.learned.len().to_string()),
            ("memories", id.memories.len().to_string()),
        ])
    }

    fn pick(&mut self, options: &[String], extra: &HashMap<&str, String>) -> String {
        if options.is_empty() {
            return "…".into();
        }
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let t = &options[(self.rng % options.len() as u64) as usize];
        let mut vars = self.vars();
        for (k, v) in extra {
            vars.insert(k, v.clone());
        }
        compose::fill(t, &vars)
    }

    fn end_turn(&mut self, input: &str, intent: &str, conf: f64, think: Vec<String>, reply: String) -> Value {
        self.last_turn = Some(LastTurn { text: input.into(), intent: intent.into() });
        json!({
            "stop_reason": "end_turn",
            "intent": intent,
            "confidence": conf,
            "content": [
                {"type": "thinking", "thinking": think.join("\n")},
                {"type": "text", "text": reply},
            ],
        })
    }
}

// ── 입력 해석 ───────────────────────────────────────────────────────────

const SAVE_MARKERS: &[&str] = &["기억해 줘", "기억해줘", "기억해 둬", "기억해둬", "기억해", "외워 줘", "외워줘", "잊지 마", "잊지마"];
const FORGET_MARKERS: &[&str] = &["잊어 줘", "잊어줘", "잊어버려", "잊어", "지워 줘", "지워줘", "삭제해 줘", "삭제해줘", "삭제해", "기억에서"];
const FILE_WORDS: &[&str] = &["파일", "찾아", "찾아줘", "어디", "있어", "폴더", "목록", "보여줘", "프로젝트", "구조", "소스"];

fn strip_markers(input: &str, markers: &[&str]) -> String {
    let mut s = input.to_string();
    for m in markers {
        if let Some(i) = s.find(m) {
            s.replace_range(i..i + m.len(), " ");
            break;
        }
    }
    let s = s.trim_matches(|c: char| c.is_whitespace() || ":,.!?~".contains(c)).to_string();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_name(input: &str) -> Option<String> {
    let raw = if let Some((_, rest)) = input.split_once("이름은") {
        rest.split_whitespace().next()?.to_string()
    } else if let Some(i) = input.find("라고 불러").or_else(|| input.find("이라고 불러")) {
        input[..i].split_whitespace().last()?.to_string()
    } else {
        return None;
    };
    let mut name = raw.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    for suffix in ["입니다", "이에요", "예요", "이라고", "라고", "이야", "야", "이고"] {
        if let Some(s) = name.strip_suffix(suffix).filter(|s| !s.is_empty()) {
            name = s.to_string();
            break;
        }
    }
    (!name.is_empty()).then_some(name)
}

fn tool_input(intent: &str, input: &str) -> Result<Value, String> {
    match intent {
        "memory_save" => {
            let c = strip_markers(input, SAVE_MARKERS);
            if c.is_empty() || ["이거", "이것", "그거", "이걸"].contains(&c.as_str()) {
                Err("무엇을 기억할까요? 예: 기억해: 나는 아침형 인간이야".into())
            } else {
                Ok(json!({"text": c}))
            }
        }
        "memory_forget" => {
            let q = strip_markers(input, FORGET_MARKERS);
            let q = text::keywords(&q).into_iter().filter(|w| !["기억", "얘기", "내용"].contains(&w.as_str())).collect::<Vec<_>>().join(" ");
            if q.is_empty() {
                Err("어떤 기억을 잊을까요? 예: 커피 얘기 잊어줘".into())
            } else {
                Ok(json!({"query": q}))
            }
        }
        "owner_set" => extract_name(input)
            .map(|n| json!({"name": n}))
            .ok_or_else(|| "뭐라고 불러 드릴까요? 예: 내 이름은 민준이야".into()),
        "file_search" => {
            let q: Vec<String> = text::keywords(input).into_iter().filter(|w| !FILE_WORDS.contains(&w.as_str())).collect();
            Ok(json!({"query": q.join(" ")}))
        }
        _ => Ok(json!({})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names() {
        assert_eq!(extract_name("내 이름은 민준이야").as_deref(), Some("민준"));
        assert_eq!(extract_name("나를 지수라고 불러줘").as_deref(), Some("지수"));
        assert_eq!(extract_name("안녕"), None);
    }

    #[test]
    fn memory_content() {
        assert_eq!(strip_markers("기억해 줘: 나는 커피를 좋아해", SAVE_MARKERS), "나는 커피를 좋아해");
        assert_eq!(strip_markers("나는 고양이를 키워 기억해", SAVE_MARKERS), "나는 고양이를 키워");
        assert!(tool_input("memory_save", "이거 기억해").is_err());
        assert_eq!(tool_input("memory_forget", "커피 얘기 잊어줘").unwrap()["query"], "커피");
    }
}
