//! 한국어 중심 텍스트 처리: 토큰화, 조사 떼기, 핵심어, 문장 나누기.

use std::collections::HashSet;

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 // 한글
        | 0x3040..=0x30FF                                   // 가나
        | 0x4E00..=0x9FFF)                                  // 한자
}

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0xAC00..=0xD7A3)
}

/// 긴 것부터 검사한다 (첫 일치가 가장 긴 조사).
const JOSA: &[&str] = &[
    "에서는", "에게는", "으로는", "이라는", "이라고", "에서", "에게", "한테", "으로", "까지", "부터", "처럼",
    "보다", "이랑", "라는", "라고", "하고", "은", "는", "이", "가", "을", "를", "의", "에", "로", "와", "과",
    "도", "만", "랑",
];

/// 질문에서 의미가 약한 낱말. 핵심어 추출에서 뺀다.
const STOPWORDS: &[&str] = &[
    "뭐", "뭐야", "뭐예요", "뭔가요", "뭐지", "무엇", "무엇인가요", "어떻게", "왜", "언제", "어디", "알려", "알려줘",
    "알려주세요", "해줘", "해", "줘", "주세요", "좀", "그", "저", "것", "거", "수", "있어", "있나요", "대해", "대해서",
    "관해", "관해서", "뭘", "뭐를", "무엇을", "어떤", "얼마나", "설명", "설명해", "설명해줘", "말해줘", "가르쳐줘", "나", "너", "제", "내", "혹시", "정말",
    "그냥", "지금", "the", "a", "is", "what", "how",
];

pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// 한글 낱말 끝의 조사를 뗀다. ("와이파이가" → "와이파이")
pub fn stem(word: &str) -> String {
    let n = word.chars().count();
    if n < 2 || !word.chars().all(is_hangul) {
        return word.to_string();
    }
    for j in JOSA {
        if word.ends_with(j) && n > j.chars().count() {
            return word[..word.len() - j.len()].to_string();
        }
    }
    word.to_string()
}

/// 긴 것부터 검사하는 종결 어미. ("누구니" → "누구", "몇이에요" → "몇")
const ENDINGS: &[&str] = &["인가요", "이에요", "이니", "예요", "이야", "나요", "니", "야", "요", "지"];

/// 한글 낱말 끝의 종결 어미를 뗀다.
pub fn strip_ending(word: &str) -> String {
    let n = word.chars().count();
    if n < 2 || !word.chars().all(is_hangul) {
        return word.to_string();
    }
    for e in ENDINGS {
        if word.ends_with(e) && n > e.chars().count() {
            return word[..word.len() - e.len()].to_string();
        }
    }
    word.to_string()
}

/// 검색·분류용 토큰: 낱말 + 조사·어미 뗀 낱말 + 한글 글자 2-gram.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in words(text) {
        // 한 낱말에서 나온 토큰은 중복 없이 한 번씩만 센다 (두 글자 낱말이 2-gram 과 겹쳐 두 배로 세지는 것 방지).
        let mut forms: Vec<String> = Vec::new();
        let chars: Vec<char> = w.chars().collect();
        if chars.len() >= 3 && chars.iter().any(|&c| is_cjk(c)) {
            forms.extend(chars.windows(2).map(|p| p.iter().collect::<String>()));
        }
        forms.push(stem(&w));
        forms.push(strip_ending(&w));
        forms.push(w);
        let mut seen = HashSet::new();
        out.extend(forms.into_iter().filter(|f| seen.insert(f.clone())));
    }
    out
}

pub fn token_set(text: &str) -> HashSet<String> {
    tokenize(text).into_iter().collect()
}

/// 질문의 핵심어 (불용어 제외, 조사 뗌).
pub fn keywords(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    words(text)
        .into_iter()
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .map(|w| stem(&w))
        .filter(|w| w.chars().count() >= 2 || w.chars().all(|c| c.is_ascii_digit()))
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

/// 공백을 정리하고 소문자로 바꾼 비교용 문자열.
pub fn normalize(text: &str) -> String {
    text.trim()
        .trim_end_matches(['?', '!', '.', '~'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 문장 나누기. 마침표 뒤에 공백·끝이 올 때와 줄바꿈에서 자른다 (소수점은 유지).
pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '\n' {
            push_sentence(&mut out, &mut buf);
            continue;
        }
        buf.push(c);
        let boundary = matches!(c, '.' | '!' | '?' | '。') && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if boundary {
            push_sentence(&mut out, &mut buf);
        }
    }
    push_sentence(&mut out, &mut buf);
    out
}

fn push_sentence(out: &mut Vec<String>, buf: &mut String) {
    let s = buf
        .trim()
        .trim_start_matches(['#', '-', '*', '>', ' '])
        .trim()
        .to_string();
    if s.chars().count() >= 4 {
        out.push(s);
    }
    buf.clear();
}

/// 문자 수 기준으로 자르고 말줄임표를 붙인다.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_korean_particles() {
        assert_eq!(stem("와이파이가"), "와이파이");
        assert_eq!(stem("네트워크에서는"), "네트워크");
        assert_eq!(stem("wifi"), "wifi");
        assert_eq!(stem("가"), "가");
    }

    #[test]
    fn keywords_drop_question_words() {
        assert_eq!(keywords("개방형 와이파이가 뭐야?"), vec!["개방형", "와이파이"]);
    }

    #[test]
    fn sentences_keep_decimals() {
        let s = sentences("버전은 1.5 이다. 다음 문장!\n# 제목 줄");
        assert_eq!(s, vec!["버전은 1.5 이다.", "다음 문장!", "제목 줄"]);
    }
}
