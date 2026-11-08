//! 말 만들기: 템플릿 채우기, 도구 결과(관찰)를 문장으로 옮기기.

use serde_json::Value;
use std::collections::HashMap;

/// 받침이 있으면 "이에요", 없으면 "예요". (한글이 아니면 "이에요")
pub fn eyo(word: &str) -> &'static str {
    match word.chars().last().map(|c| c as u32) {
        Some(c @ 0xAC00..=0xD7A3) if (c - 0xAC00) % 28 == 0 => "예요",
        _ => "이에요",
    }
}

pub fn fill(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// 객체의 문자열·숫자 필드를 템플릿 변수로 쓴다.
pub fn vars_from(value: &Value) -> HashMap<String, String> {
    value
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| match v {
                    Value::String(s) => Some((k.clone(), s.clone())),
                    Value::Number(n) => Some((k.clone(), n.to_string())),
                    Value::Bool(b) => Some((k.clone(), b.to_string())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn network(r: &Value) -> String {
    let online = r["online"].as_bool().unwrap_or(false);
    let mut out = String::from(if online {
        "지금 인터넷에 닿아 있어요."
    } else {
        "지금은 인터넷에 닿지 않아요. 그래도 저는 이 기기 안에서 만든 데이터로 그대로 동작해요."
    });

    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for i in r["interfaces"].as_array().into_iter().flatten() {
        let kind = i["kind"].as_str().unwrap_or("other");
        if !i["up"].as_bool().unwrap_or(false) || kind == "virtual" || !seen.insert(kind.to_string()) {
            continue;
        }
        paths.push(match kind {
            "wifi" => match r["wifi"].as_object() {
                Some(w) => {
                    let ssid = w.get("ssid").and_then(Value::as_str).unwrap_or("?");
                    let open = w.get("open").and_then(Value::as_bool).unwrap_or(false);
                    let sec = w.get("security").and_then(Value::as_str).unwrap_or("?");
                    let signal = w.get("signal").and_then(Value::as_i64).map(|s| format!(", 신호 {s}%")).unwrap_or_default();
                    format!("Wi-Fi \"{ssid}\" ({}{signal})", if open { "암호 없음" } else { sec })
                }
                None => "Wi-Fi".into(),
            },
            "ethernet" => "유선".into(),
            "cellular" => "셀룰러".into(),
            "tethering" => "테더링".into(),
            "vpn" => "VPN".into(),
            other => other.to_string(),
        });
    }
    if paths.is_empty() {
        out.push_str(" 활성화된 네트워크 연결이 없어요.");
    } else {
        out.push_str(&format!(" 연결 경로: {}.", paths.join(", ")));
    }

    let warnings: Vec<&str> = r["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if warnings.is_empty() {
        out.push_str("\n특별히 위험해 보이는 점은 없어요.");
    } else {
        out.push_str("\n주의할 점:");
        for w in warnings {
            out.push_str(&format!("\n- {w}"));
        }
    }
    out
}

pub fn system(s: &Value) -> String {
    let g = |k: &str| match &s[k] {
        Value::String(v) => v.clone(),
        Value::Number(n) => n.to_string(),
        _ => "?".into(),
    };
    format!(
        "이 컴퓨터는 {} ({})예요. CPU 코어 {}개, 메모리 {}GB, 프로젝트 디스크 여유 공간 {}GB이고, 켜진 지 {}시간 됐어요.\n컴퓨터 이름은 {}, 사용자는 {}예요.",
        g("os"), g("arch"), g("cpus"), g("memory_gb"), g("disk_free_gb"), g("uptime_hours"), g("machine"), g("user")
    )
}

pub fn files(r: &Value) -> String {
    let query = r["query"].as_str().unwrap_or("");
    let matches: Vec<&str> = r["matches"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let total = r["total"].as_u64().unwrap_or(matches.len() as u64);
    if matches.is_empty() {
        return format!("프로젝트에서 '{query}'와 관련된 파일을 찾지 못했어요.");
    }
    let head = if query.is_empty() {
        "프로젝트 구조예요:".to_string()
    } else {
        format!("'{query}'와 관련된 파일 {total}개를 찾았어요:")
    };
    let mut out = head;
    for m in &matches {
        out.push_str(&format!("\n- {m}"));
    }
    if total as usize > matches.len() {
        out.push_str(&format!("\n… 외 {}개", total as usize - matches.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn eyo_by_final_consonant() {
        assert_eq!(eyo("테스터"), "예요");
        assert_eq!(eyo("민준"), "이에요");
    }

    #[test]
    fn network_sentence() {
        let r = json!({
            "online": false,
            "interfaces": [{"name":"wlan0","kind":"wifi","up":true}, {"name":"docker0","kind":"virtual","up":true}],
            "wifi": {"ssid":"Cafe","security":"없음","signal":40,"open":true},
            "warnings": ["개방형 Wi-Fi"]
        });
        let s = network(&r);
        assert!(s.contains("닿지 않아요"));
        assert!(s.contains("Wi-Fi \"Cafe\" (암호 없음, 신호 40%)"));
        assert!(s.contains("- 개방형 Wi-Fi"));
        assert!(!s.contains("virtual"));
    }
}
