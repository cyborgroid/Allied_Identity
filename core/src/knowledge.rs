//! 지식 저장소: 프로젝트 환경에서 만든 문서 조각을 BM25로 찾는다.

use crate::text;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

pub struct Hit<'a> {
    pub index: usize,
    pub doc: &'a Doc,
    pub score: f64,
}

#[derive(Default)]
pub struct KnowledgeBase {
    pub docs: Vec<Doc>,
    term_freqs: Vec<HashMap<String, u32>>,
    lens: Vec<usize>,
    doc_freq: HashMap<String, u32>,
    avg_len: f64,
}

impl KnowledgeBase {
    pub fn from_docs(docs: Vec<Doc>) -> Self {
        let mut kb = Self::default();
        for d in docs {
            kb.add(d);
        }
        kb
    }

    pub fn add(&mut self, doc: Doc) {
        let tokens = text::tokenize(&format!("{} {}", doc.title, doc.text));
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
        self.avg_len = self.lens.iter().sum::<usize>() as f64 / self.lens.len() as f64;
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn search(&self, query: &str, k: usize) -> Vec<Hit<'_>> {
        let mut terms = text::tokenize(query);
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
                (score > 0.0).then(|| Hit { index: i, doc: &self.docs[i], score })
            })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(k);
        hits
    }
}

/// 마크다운/텍스트 문서를 제목 단위·최대 길이로 조각낸다.
/// 코드 울타리(```)와 표 구분선은 버리고, 표의 칸은 공백으로 펼친다.
pub fn chunk_document(prefix: &str, source: &str, default_title: &str, body: &str, max_chars: usize) -> Vec<Doc> {
    let mut chunker = Chunker { prefix, source, title: default_title.to_string(), max: max_chars, docs: vec![], buf: String::new(), para: String::new() };
    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with("```") || (line.starts_with('|') && line.contains("---")) {
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            chunker.end_paragraph();
            chunker.flush();
            chunker.title = h.trim_start_matches('#').trim().to_string();
            continue;
        }
        if line.is_empty() {
            chunker.end_paragraph();
            continue;
        }
        let clean = line.replace("**", "").replace('`', "").replace('|', " ");
        if !chunker.para.is_empty() {
            chunker.para.push('\n');
        }
        chunker.para.push_str(clean.trim());
    }
    chunker.end_paragraph();
    chunker.flush();
    chunker.docs
}

struct Chunker<'a> {
    prefix: &'a str,
    source: &'a str,
    title: String,
    max: usize,
    docs: Vec<Doc>,
    buf: String,
    para: String,
}

impl Chunker<'_> {
    fn end_paragraph(&mut self) {
        let para = std::mem::take(&mut self.para);
        if para.is_empty() {
            return;
        }
        let pieces = if para.chars().count() > self.max { text::sentences(&para) } else { vec![para] };
        for piece in pieces {
            if !self.buf.is_empty() && self.buf.chars().count() + piece.chars().count() + 1 > self.max {
                self.flush();
            }
            if !self.buf.is_empty() {
                self.buf.push('\n');
            }
            self.buf.push_str(&piece);
        }
    }

    fn flush(&mut self) {
        let t = self.buf.trim();
        if !t.is_empty() {
            self.docs.push(Doc {
                id: format!("{}#{}", self.prefix, self.docs.len()),
                title: self.title.clone(),
                source: self.source.to_string(),
                text: t.to_string(),
            });
        }
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, title: &str, text: &str) -> Doc {
        Doc { id: id.into(), title: title.into(), source: "t".into(), text: text.into() }
    }

    #[test]
    fn search_ranks_relevant_doc_first() {
        let kb = KnowledgeBase::from_docs(vec![
            doc("a", "요리", "김치찌개는 돼지고기와 김치로 끓인다."),
            doc("b", "네트워크", "와이파이 신호가 약하면 공유기와 가까이 이동한다."),
        ]);
        let hits = kb.search("와이파이가 약해요", 2);
        assert_eq!(hits[0].doc.id, "b");
    }

    #[test]
    fn chunks_by_heading_and_skips_code_fences() {
        let md = "# 하나\n첫 문단.\n\n```bash\n```\n# 둘\n| 칸 | 값 |\n|---|---|\n| a | b |\n";
        let docs = chunk_document("p", "s.md", "기본", md, 600);
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].title, "하나");
        assert_eq!(docs[1].title, "둘");
        assert!(!docs[1].text.contains("---"));
        assert!(docs[1].text.contains("a   b"));
    }

    #[test]
    fn long_paragraph_is_split() {
        let para = "가나다라마바사. ".repeat(50);
        let docs = chunk_document("p", "s", "t", &para, 100);
        assert!(docs.len() > 1);
        assert!(docs.iter().all(|d| d.text.chars().count() <= 100));
    }
}
