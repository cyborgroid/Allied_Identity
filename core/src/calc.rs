//! 계산 능력: 문장에서 수식을 찾아 계산한다. (+ - * / % ^ 괄호, "더하기·빼기·곱하기·나누기")

const WORD_OPS: &[(&str, &str)] = &[
    ("더하기", "+"), ("플러스", "+"), ("빼기", "-"), ("마이너스", "-"), ("곱하기", "*"), ("나누기", "/"),
    ("의 제곱", "^2"), ("제곱", "^"), ("×", "*"), ("÷", "/"), ("x", "*"),
];

/// 문장에서 숫자와 연산자가 함께 있는 가장 긴 구간을 수식으로 뽑는다.
pub fn extract(text: &str) -> Option<String> {
    let mut s = text.to_lowercase();
    for (w, op) in WORD_OPS {
        s = s.replace(w, op);
    }
    let allowed = |c: char| c.is_ascii_digit() || "+-*/%^(). ,".contains(c);
    s.split(|c: char| !allowed(c))
        .map(|run| run.replace(',', "").trim().to_string())
        .filter(|run| run.chars().any(|c| c.is_ascii_digit()) && run.chars().any(|c| "+-*/%^".contains(c)))
        .max_by_key(|run| run.len())
}

pub fn eval(expr: &str) -> Result<f64, String> {
    let tokens: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let mut p = Parser { t: &tokens, i: 0 };
    let v = p.expr()?;
    if p.i != tokens.len() {
        return Err(format!("'{}' 근처를 이해하지 못했어요", tokens[p.i..].iter().collect::<String>()));
    }
    if !v.is_finite() {
        return Err("결과가 정의되지 않아요 (0으로 나누기 등)".into());
    }
    Ok(v)
}

pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

struct Parser<'a> {
    t: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.t.get(self.i).copied()
    }

    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if op == '+' { v + r } else { v - r };
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.power()?;
        while let Some(op @ ('*' | '/' | '%')) = self.peek() {
            self.i += 1;
            let r = self.power()?;
            v = match op {
                '*' => v * r,
                '/' => v / r,
                _ => v % r,
            };
        }
        Ok(v)
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let exp = self.power()?; // 오른쪽 결합
            return Ok(base.powf(exp));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<f64, String> {
        if self.peek() == Some('-') {
            self.i += 1;
            return Ok(-self.unary()?);
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some('(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(')') {
                    return Err("괄호가 닫히지 않았어요".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.i;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
                    self.i += 1;
                }
                let s: String = self.t[start..self.i].iter().collect();
                s.parse().map_err(|_| format!("'{s}'는 숫자가 아니에요"))
            }
            Some(c) => Err(format!("'{c}' 근처를 이해하지 못했어요")),
            None => Err("식이 끝나 버렸어요".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_evaluates() {
        assert_eq!(extract("(3+5)*2 얼마야?").as_deref(), Some("(3+5)*2"));
        assert_eq!(extract("12 곱하기 7은?").as_deref(), Some("12 * 7"));
        assert_eq!(extract("안녕"), None);
        assert_eq!(eval("(3+5)*2").unwrap(), 16.0);
        assert_eq!(eval("2^3^2").unwrap(), 512.0);
        assert_eq!(eval("-4+10/4").unwrap(), -1.5);
        assert!(eval("1/0").is_err());
        assert!(eval("(1+2").is_err());
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(16.0), "16");
    }
}
