//! 실제 data/seed 로 엔진 전체를 돌려 보는 통합 테스트.
//! 학습 예문에 없는 표현으로 묻고, 생각 → 도구 → 답 흐름을 확인한다.

use allied_core::engine::{Config, Engine, Request};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            fs::copy(&p, &dest).unwrap();
        }
    }
}

fn engine() -> (Engine, PathBuf) {
    let seed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/seed");
    let root = std::env::temp_dir().join(format!("allied-test-{}-{}", std::process::id(), rand_suffix()));
    copy_dir(&seed, &root.join("seed"));
    let mut e = Engine::open(Config { data_dir: root.clone(), effort: Default::default(), identity: None }).unwrap();
    let env = vec![serde_json::from_value(json!({
        "id": "env:system", "title": "이 컴퓨터", "source": "환경 관찰",
        "text": "이 컴퓨터의 운영체제는 Ubuntu이고 CPU 코어는 16개다."
    }))
    .unwrap()];
    e.handle(Request::Forge { environment: env }).unwrap();
    (e, root)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn say(e: &mut Engine, text: &str) -> Value {
    e.handle(Request::Message { text: text.into() }).unwrap()
}

fn reply_text(v: &Value) -> String {
    v["content"].as_array().unwrap().iter().filter(|b| b["type"] == "text").map(|b| b["text"].as_str().unwrap()).collect()
}

fn tool_use(v: &Value) -> &Value {
    v["content"].as_array().unwrap().iter().find(|b| b["type"] == "tool_use").expect("tool_use 블록")
}

#[test]
fn understands_unseen_phrasings() {
    let (mut e, root) = engine();
    let cases = [
        ("안녕 반가워요", "greeting"),
        ("지금 몇 시인지 알려 줄래", "time_now"),
        ("오늘 무슨 요일이에요", "date_today"),
        ("와이파이 보안 상태 점검해 줘", "network_status"),
        ("이 컴퓨터 메모리 얼마야", "system_info"),
        ("너는 누구니", "identity_who"),
        ("7 곱하기 8 얼마야", "calc"),
        ("고마워요 정말", "thanks"),
    ];
    for (text, want) in cases {
        let r = say(&mut e, text);
        assert_eq!(r["intent"], want, "\"{text}\" → {r}");
    }
    fs::remove_dir_all(root).ok();
}

#[test]
fn tool_loop_like_claude() {
    let (mut e, root) = engine();
    let r = say(&mut e, "지금 몇 시야?");
    assert_eq!(r["stop_reason"], "tool_use");
    let t = tool_use(&r);
    assert_eq!(t["name"], "time.now");
    let id = t["id"].as_str().unwrap().to_string();
    let r = e
        .handle(Request::ToolResult { tool_use_id: id, content: json!({"time": "오후 3시 12분", "date": "x"}), is_error: false })
        .unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
    assert_eq!(reply_text(&r), "지금은 오후 3시 12분이에요.");
    fs::remove_dir_all(root).ok();
}

#[test]
fn memory_and_owner_tools() {
    let (mut e, root) = engine();
    let r = say(&mut e, "기억해 줘: 나는 매일 아침 달리기를 해");
    let t = tool_use(&r).clone();
    assert_eq!(t["name"], "memory.save");
    assert_eq!(t["input"]["text"], "나는 매일 아침 달리기를 해");
    e.handle(Request::ToolResult {
        tool_use_id: t["id"].as_str().unwrap().into(),
        content: json!({"memory": {"id": "m1", "text": "나는 매일 아침 달리기를 해", "created_at": 0}}),
        is_error: false,
    })
    .unwrap();
    let r = say(&mut e, "나에 대해 뭘 기억해?");
    assert!(reply_text(&r).contains("달리기"), "{r}");

    let r = say(&mut e, "내 이름은 서연이야");
    assert_eq!(tool_use(&r)["input"]["name"], "서연");
    fs::remove_dir_all(root).ok();
}

#[test]
fn answers_from_generated_knowledge_with_sources() {
    let (mut e, root) = engine();
    let r = say(&mut e, "캡티브 포털이 뭐야?");
    let t = reply_text(&r);
    assert!(t.contains("로그인"), "{t}");
    assert!(t.contains("출처:"), "{t}");

    // 환경 관찰에서 만든 지식
    let r = say(&mut e, "이 컴퓨터 운영체제가 Ubuntu야?");
    let t = reply_text(&r);
    assert!(t.contains("Ubuntu") || r["stop_reason"] == "tool_use", "{r}");
    fs::remove_dir_all(root).ok();
}

#[test]
fn honest_when_no_evidence_then_learns() {
    let (mut e, root) = engine();
    let r = say(&mut e, "양자역학의 슈뢰딩거 방정식");
    assert!(reply_text(&r).contains("모르겠어요"), "{r}");

    e.handle(Request::Teach { question: "슈뢰딩거 방정식이 뭐야".into(), answer: "양자 상태의 시간 변화를 기술하는 방정식이에요.".into() })
        .unwrap();
    let r = say(&mut e, "슈뢰딩거 방정식이 뭐야?");
    assert_eq!(reply_text(&r), "양자 상태의 시간 변화를 기술하는 방정식이에요.");
    assert!(root.join("state/learned.jsonl").exists());
    fs::remove_dir_all(root).ok();
}

#[test]
fn forge_writes_generated_data() {
    let (mut e, root) = engine();
    for f in ["knowledge.jsonl", "training.jsonl", "intent_model.json", "manifest.json"] {
        assert!(root.join("generated").join(f).exists(), "{f}");
    }
    let s = e.handle(Request::Status).unwrap();
    assert!(s["training_examples"].as_u64().unwrap() > 500, "{s}");
    fs::remove_dir_all(root).ok();
}
