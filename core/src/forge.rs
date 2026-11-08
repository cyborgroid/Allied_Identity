//! 데이터 생성기(Forge).
//!
//! 실행 중에 외부에서 데이터를 가져오지 않는다. 시작할 때마다 프로젝트 환경에서
//! 데이터를 새로 만든다:
//!   - data/seed/knowledge/*.md   → 지식 조각
//!   - 호스트가 관찰한 환경 문서    → 지식 조각 (이 컴퓨터, 네트워크, 프로젝트 구조, README)
//!   - 인격체 자기 소개            → 지식 조각
//!   - 사용자가 가르친 내용         → 지식 조각
//!   - data/seed/intents.json     → 변형 규칙으로 학습 예문을 늘린 뒤 의도 분류기 학습
//!   - 사용자가 맞다고 확인한 예문  → 학습 예문
//! 결과는 data/generated/ 에 기록해 사람이 확인할 수 있게 한다.

use crate::identity::Identity;
use crate::intent::IntentModel;
use crate::knowledge::{chunk_document, Doc, KnowledgeBase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const CHUNK_CHARS: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    pub intent: String,
    /// reply | tool | knowledge | calc | memory_recall | more
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub responses: Vec<String>,
}

fn default_kind() -> String {
    "reply".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learned {
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub intent: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ForgeStats {
    pub knowledge_docs: usize,
    pub seed_docs: usize,
    pub environment_docs: usize,
    pub learned_docs: usize,
    pub training_examples: usize,
    pub intents: usize,
    pub vocabulary: usize,
}

/// 프로젝트 안의 데이터 디렉터리 구조.
#[derive(Debug, Clone)]
pub struct DataDir {
    pub root: PathBuf,
}

impl DataDir {
    pub fn seed(&self) -> PathBuf {
        self.root.join("seed")
    }
    pub fn generated(&self) -> PathBuf {
        self.root.join("generated")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }
}

pub fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    fs::read_to_string(path)
        .map(|s| s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
        .unwrap_or_default()
}

pub fn write_jsonl<T: Serialize>(path: &Path, items: &[T]) -> Result<(), String> {
    let mut out = String::new();
    for it in items {
        out.push_str(&serde_json::to_string(it).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    write_file(path, &out)
}

pub fn append_jsonl<T: Serialize>(path: &Path, item: &T) -> Result<(), String> {
    use std::io::Write;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(item).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_intents(data: &DataDir) -> Result<Vec<IntentSpec>, String> {
    let path = data.seed().join("intents.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// 어미·띄어쓰기 변형으로 예문을 늘린다.
const ENDING_RULES: &[(&str, &[&str])] = &[
    ("알려줘", &["알려 줘", "알려주세요", "알려 주세요", "가르쳐줘", "말해줘", "말해 줘"]),
    ("해줘", &["해 줘", "해주세요", "해 주세요", "해줄래", "해 줄래"]),
    ("보여줘", &["보여 줘", "보여주세요", "보여줄래"]),
    ("찾아줘", &["찾아 줘", "찾아주세요", "찾아줄래"]),
    ("뭐야", &["뭐예요", "뭔가요", "뭐지", "뭐니"]),
    ("어때", &["어때요", "어떤가요"]),
    ("있어", &["있어요", "있나요", "있니"]),
    ("몇 시야", &["몇시야", "몇 시예요", "몇 시니"]),
    ("고마워", &["고마워요", "고맙다"]),
    ("잊어줘", &["잊어 줘", "잊어주세요", "잊어버려"]),
    ("불러줘", &["불러 줘", "불러주세요"]),
    ("얼마야", &["얼마예요", "몇이야"]),
];

pub fn augment(example: &str) -> Vec<String> {
    let core = example.trim().trim_end_matches(['?', '!', '.', '~']).trim();
    let mut forms = BTreeSet::from([core.to_string()]);
    for (key, variants) in ENDING_RULES {
        if let Some(stem) = core.strip_suffix(key) {
            for v in *variants {
                forms.insert(format!("{stem}{v}"));
            }
        }
    }
    let mut out = BTreeSet::new();
    for f in forms {
        out.insert(format!("{f}?"));
        out.insert(f.replace(' ', ""));
        out.insert(format!("혹시 {f}"));
        out.insert(f);
    }
    out.into_iter().collect()
}

pub struct Forged {
    pub kb: KnowledgeBase,
    pub model: IntentModel,
    pub stats: ForgeStats,
}

pub fn forge(
    data: &DataDir,
    specs: &[IntentSpec],
    identity: &Identity,
    environment: &[Doc],
    learned: &[Learned],
    confirmed: &[Example],
) -> Result<Forged, String> {
    let mut stats = ForgeStats::default();
    let mut docs = Vec::new();

    // 1) 씨앗 지식
    let seed_dir = data.seed().join("knowledge");
    let mut files: Vec<PathBuf> = fs::read_dir(&seed_dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    files.retain(|p| p.extension().is_some_and(|x| x == "md" || x == "txt"));
    files.sort();
    for f in files {
        let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let stem = f.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let body = fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        let chunks = chunk_document(&format!("seed:{name}"), &name, &stem, &body, CHUNK_CHARS);
        stats.seed_docs += chunks.len();
        docs.extend(chunks);
    }

    // 2) 자기 인식
    docs.extend(chunk_document("self", "인격체", "나에 대해", &identity.self_description(), CHUNK_CHARS));

    // 3) 환경 관찰
    for d in environment {
        let chunks = chunk_document(&d.id, &d.source, &d.title, &d.text, CHUNK_CHARS);
        stats.environment_docs += chunks.len();
        docs.extend(chunks);
    }

    // 4) 가르침
    for (i, l) in learned.iter().enumerate() {
        docs.push(Doc { id: format!("learned:{i}"), title: l.question.clone(), source: "가르침".into(), text: l.answer.clone() });
    }
    stats.learned_docs = learned.len();

    // 5) 학습 예문
    let mut training: Vec<Example> = Vec::new();
    for spec in specs {
        for ex in &spec.examples {
            training.extend(augment(ex).into_iter().map(|text| Example { intent: spec.intent.clone(), text }));
        }
    }
    for c in confirmed {
        training.extend(augment(&c.text).into_iter().map(|text| Example { intent: c.intent.clone(), text }));
    }
    let model = IntentModel::train(training.iter().map(|e| (e.intent.as_str(), e.text.as_str())));

    stats.knowledge_docs = docs.len();
    stats.training_examples = training.len();
    stats.intents = model.intents.len();
    stats.vocabulary = model.vocab_size();

    let gen = data.generated();
    write_jsonl(&gen.join("knowledge.jsonl"), &docs)?;
    write_jsonl(&gen.join("training.jsonl"), &training)?;
    write_file(&gen.join("intent_model.json"), &serde_json::to_string(&model).map_err(|e| e.to_string())?)?;
    write_file(&gen.join("manifest.json"), &serde_json::to_string_pretty(&stats).map_err(|e| e.to_string())?)?;

    Ok(Forged { kb: KnowledgeBase::from_docs(docs), model, stats })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augment_endings_and_spacing() {
        let v = augment("시간 알려줘?");
        assert!(v.contains(&"시간 알려주세요".to_string()));
        assert!(v.contains(&"시간알려줘".to_string()));
        assert!(v.contains(&"혹시 시간 알려줘".to_string()));
        assert!(v.contains(&"시간 알려줘?".to_string()));
    }
}
