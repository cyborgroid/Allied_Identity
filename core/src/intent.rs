//! 의도 이해: 다항 나이브 베이즈 분류기.
//! 프로젝트 안에서 생성한 학습 예문으로 학습하고, 사용자 피드백으로 계속 배운다.

use crate::text;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ALPHA: f64 = 0.5;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IntentModel {
    pub intents: Vec<String>,
    counts: Vec<HashMap<String, u32>>,
    totals: Vec<u64>,
    /// 전체 어휘 (토큰 → 등장한 의도 수)
    vocab: HashMap<String, u32>,
}

pub struct Prediction {
    pub ranked: Vec<(String, f64)>,
    /// 질문 토큰 중 학습 어휘에 있는 비율 (0~1)
    pub coverage: f64,
}

impl Prediction {
    pub fn top(&self) -> Option<(&str, f64)> {
        self.ranked.first().map(|(i, p)| (i.as_str(), *p))
    }

    /// 확률에 어휘 적중률을 반영한 확신도. 모르는 말이 많을수록 낮아진다.
    pub fn confidence(&self) -> f64 {
        self.top().map(|(_, p)| p * (0.4 + 0.6 * self.coverage)).unwrap_or(0.0)
    }
}

impl IntentModel {
    pub fn train<'a>(examples: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut m = Self::default();
        for (intent, text) in examples {
            m.learn(intent, text);
        }
        m
    }

    /// 예문 하나를 더 배운다 (온라인 학습).
    pub fn learn(&mut self, intent: &str, text: &str) {
        let i = match self.intents.iter().position(|x| x == intent) {
            Some(i) => i,
            None => {
                self.intents.push(intent.to_string());
                self.counts.push(HashMap::new());
                self.totals.push(0);
                self.intents.len() - 1
            }
        };
        for t in text::tokenize(text) {
            let c = self.counts[i].entry(t.clone()).or_default();
            if *c == 0 {
                *self.vocab.entry(t).or_default() += 1;
            }
            *c += 1;
            self.totals[i] += 1;
        }
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab.len()
    }

    pub fn predict(&self, text: &str) -> Prediction {
        let tokens = text::tokenize(text);
        if tokens.is_empty() || self.intents.is_empty() {
            return Prediction { ranked: vec![], coverage: 0.0 };
        }
        let v = self.vocab.len().max(1) as f64;
        let n = self.intents.len() as f64;
        // 처음 보는 토큰은 판단 근거가 없으므로 빼고, 여러 의도에 두루 나오는 토큰("너는" 등)은
        // 가볍게, 한 의도에만 나오는 토큰("누구" 등)은 무겁게 반영한다 (의도 간 IDF 가중).
        let weighted: Vec<(&String, f64)> = tokens
            .iter()
            .filter_map(|t| self.vocab.get(t).map(|&df| (t, ((n + 1.0) / (df as f64 + 0.5)).ln())))
            .collect();
        let logs: Vec<f64> = (0..self.intents.len())
            .map(|i| {
                let denom = self.totals[i] as f64 + ALPHA * v;
                weighted
                    .iter()
                    .map(|(t, w)| w * ((*self.counts[i].get(*t).unwrap_or(&0) as f64 + ALPHA) / denom).ln())
                    .sum()
            })
            .collect();
        let max = logs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = logs.iter().map(|l| (l - max).exp()).collect();
        let sum: f64 = exps.iter().sum();
        let mut ranked: Vec<(String, f64)> =
            self.intents.iter().cloned().zip(exps.iter().map(|e| e / sum)).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

        let known = tokens.iter().filter(|t| self.vocab.contains_key(*t)).count();
        Prediction { ranked, coverage: known as f64 / tokens.len() as f64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_and_predicts() {
        let m = IntentModel::train([
            ("time", "지금 몇 시야"),
            ("time", "현재 시간 알려줘"),
            ("greet", "안녕"),
            ("greet", "안녕하세요 반가워"),
        ]);
        let p = m.predict("몇 시인지 알려줘");
        assert_eq!(p.top().unwrap().0, "time");
        let p = m.predict("양자역학");
        assert!(p.confidence() < 0.5);
    }
}
