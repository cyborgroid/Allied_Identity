//! 오프라인 지식 팩: 인터넷 대신 쓰는 내장 "데이터".
//!
//! 지식 디렉터리의 `*.jsonl` 파일을 읽는다. 한 줄이 문서 조각 하나:
//! `{"id": "...", "title": "...", "source": "...", "text": "..."}`
//! 팩은 `tools/knowledge_builder.py` 로 만든다. 검색은 BM25이며,
//! 한글/CJK 단어는 글자 2-gram도 색인해 조사가 붙은 형태도 찾을 수 있게 한다.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct PackInfo {
    pub name: String,
    pub docs: usize,
    pub skipped_lines: usize,
}

pub struct Hit<'a> {
    pub doc: &'a Doc,
    pub score: f64,
}

#[derive(Default)]
pub struct KnowledgeBase {
    docs: Vec<Doc>,
    term_freqs: Vec<HashMap<String, u32>>,
    lens: Vec<usize>,
    doc_freq: HashMap<String, u32>,
    avg_len: f64,
    pub packs: Vec<PackInfo>,
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 // 한글
        | 0x3040..=0x30FF                                   // 가나
        | 0x4E00..=0x9FFF)                                  // 한자
}

pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        let word = word.to_lowercase();
        let chars: Vec<char> = word.chars().collect();
        if chars.len() >= 2 && chars.iter().any(|&c| is_cjk(c)) {
            for pair in chars.windows(2) {
                out.push(pair.iter().collect());
            }
        }
        out.push(word);
    }
    out
}

impl KnowledgeBase {
    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        let mut kb = Self::default();
        if !dir.exists() {
            return Ok(kb);
        }
        let mut files: Vec<_> = fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        files.sort();

        for file in files {
            let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let mut info = PackInfo {
                name: file.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                docs: 0,
                skipped_lines: 0,
            };
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<Doc>(line) {
                    Ok(doc) => {
                        kb.add(doc);
                        info.docs += 1;
                    }
                    Err(_) => info.skipped_lines += 1,
                }
            }
            kb.packs.push(info);
        }
        kb.finish();
        Ok(kb)
    }

    #[cfg(test)]
    pub fn from_docs(docs: Vec<Doc>) -> Self {
        let mut kb = Self::default();
        for d in docs {
            kb.add(d);
        }
        kb.finish();
        kb
    }

    fn add(&mut self, doc: Doc) {
        let tokens = tokenize(&format!("{} {}", doc.title, doc.text));
        let mut tf: HashMap<String, u32> = HashMap::new();
        for t in &tokens {
            *tf.entry(t.clone()).or_default() += 1;
        }
        for term in tf.keys() {
            *self.doc_freq.entry(term.clone()).or_default() += 1;
        }
        self.lens.push(tokens.len());
        self.term_freqs.push(tf);
        self.docs.push(doc);
    }

    fn finish(&mut self) {
        self.avg_len = if self.lens.is_empty() {
            0.0
        } else {
            self.lens.iter().sum::<usize>() as f64 / self.lens.len() as f64
        };
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub fn search(&self, query: &str, k: usize) -> Vec<Hit<'_>> {
        let mut terms = tokenize(query);
        terms.sort();
        terms.dedup();
        let n = self.docs.len() as f64;

        let mut hits: Vec<Hit> = self
            .term_freqs
            .iter()
            .enumerate()
            .filter_map(|(i, tf)| {
                let len_norm = 1.0 - B + B * self.lens[i] as f64 / self.avg_len.max(1.0);
                let score: f64 = terms
                    .iter()
                    .filter_map(|t| {
                        let f = *tf.get(t)? as f64;
                        let df = *self.doc_freq.get(t)? as f64;
                        let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                        Some(idf * f * (K1 + 1.0) / (f + K1 * len_norm))
                    })
                    .sum();
                (score > 0.0).then(|| Hit { doc: &self.docs[i], score })
            })
            .collect();

        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(k);
        hits
    }
}

/// 검색 결과를 사용자 메시지 앞에 붙일 <참고자료> 블록으로 만든다.
pub fn format_context(hits: &[Hit]) -> String {
    let mut out = String::from("<참고자료>\n");
    for (i, h) in hits.iter().enumerate() {
        let title = if h.doc.title.is_empty() { &h.doc.id } else { &h.doc.title };
        out.push_str(&format!("[{}] {} ({})\n{}\n\n", i + 1, title, h.doc.source, h.doc.text.trim()));
    }
    out.push_str("</참고자료>\n\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, title: &str, text: &str) -> Doc {
        Doc { id: id.into(), title: title.into(), source: "test".into(), text: text.into() }
    }

    #[test]
    fn tokenize_korean_bigrams() {
        let t = tokenize("오프라인에서");
        assert!(t.contains(&"오프".to_string()));
        assert!(t.contains(&"오프라인에서".to_string()));
    }

    #[test]
    fn search_ranks_relevant_doc_first() {
        let kb = KnowledgeBase::from_docs(vec![
            doc("a", "요리", "김치찌개는 돼지고기와 김치로 끓인다."),
            doc("b", "네트워크", "와이파이 신호가 약하면 공유기와 가까이 이동한다."),
            doc("c", "rust", "Cargo builds Rust crates."),
        ]);
        let hits = kb.search("와이파이가 약해요", 2);
        assert_eq!(hits[0].doc.id, "b");
        let hits = kb.search("cargo", 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc.id, "c");
    }

    #[test]
    fn empty_query_or_kb() {
        let kb = KnowledgeBase::default();
        assert!(kb.search("anything", 3).is_empty());
        let kb = KnowledgeBase::from_docs(vec![doc("a", "", "hello")]);
        assert!(kb.search("", 3).is_empty());
    }
}
