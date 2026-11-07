//! AlliedID — Allied Identity (고유 인격체 파트너) 터미널 진입점.
#![allow(non_snake_case)]

mod config;
mod engine;
mod fsutil;
mod identity;
mod knowledge;
mod net;
mod paths;
mod session;

use config::{Config, EngineMode};
use identity::Identity;
use knowledge::KnowledgeBase;
use session::Session;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
AlliedID — Allied Identity (고유 인격체 파트너)

사용법:
  AlliedID                       대화 모드 시작
  AlliedID ask <질문>            한 번 묻고 종료
  AlliedID status                인격체·엔진·네트워크·지식 팩 상태
  AlliedID net [--json]          네트워크 환경 보고서
  AlliedID knowledge list        설치된 지식 팩 목록
  AlliedID knowledge search <q>  지식 팩 검색
  AlliedID identity              인격체 프로필 보기
  AlliedID identity remember <t> 기억 추가
  AlliedID identity forget <id>  기억 삭제
  AlliedID init                  기본 설정·인격체 파일 생성
  AlliedID --engine <auto|local|claude> ...   이번 실행의 엔진 지정
  AlliedID --version | --help

대화 모드 명령:
  /help /status /net /engine <모드> /remember <내용> /forget <id>
  /memories /search <q> /clear /exit
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut engine_override = None;
    if let Some(pos) = args.iter().position(|a| a == "--engine") {
        let Some(value) = args.get(pos + 1).cloned() else {
            eprintln!("--engine 뒤에 auto|local|claude 중 하나가 필요합니다.");
            return ExitCode::from(2);
        };
        match EngineMode::parse(&value) {
            Some(m) => engine_override = Some(m),
            None => {
                eprintln!("알 수 없는 엔진: {value}");
                return ExitCode::from(2);
            }
        }
        args.drain(pos..=pos + 1);
    }

    match run(args, engine_override) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("오류: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_config(engine_override: Option<EngineMode>) -> Result<Config, String> {
    let mut cfg = Config::load()?;
    if let Some(m) = engine_override {
        cfg.engine = m;
    }
    Ok(cfg)
}

fn run(args: Vec<String>, engine_override: Option<EngineMode>) -> Result<(), String> {
    let cmd = args.first().map(String::as_str);
    let rest = || args[1..].join(" ");
    match cmd {
        None => repl(Session::new(load_config(engine_override)?)?),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("-V" | "--version" | "version") => {
            println!("AlliedID {VERSION}");
            Ok(())
        }
        Some("init") => init(),
        Some("ask") => {
            let q = rest();
            if q.trim().is_empty() {
                return Err("질문을 입력하세요: AlliedID ask <질문>".into());
            }
            let mut s = Session::new(load_config(engine_override)?)?;
            let a = s.ask(&q)?;
            print_answer(&a);
            Ok(())
        }
        Some("status") => {
            let s = Session::new(load_config(engine_override)?)?;
            print_status(&s);
            Ok(())
        }
        Some("net") => {
            let cfg = load_config(engine_override)?;
            let r = net::scan(&cfg.net);
            if args.iter().any(|a| a == "--json") {
                println!("{}", serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?);
            } else {
                print_net(&r);
            }
            Ok(())
        }
        Some("knowledge") => {
            let cfg = load_config(engine_override)?;
            let kb = KnowledgeBase::load_dir(&cfg.knowledge_path())?;
            match args.get(1).map(String::as_str) {
                Some("search") => {
                    let q = args[2..].join(" ");
                    for (i, h) in kb.search(&q, 5).iter().enumerate() {
                        println!("[{}] {:.2}  {} — {}", i + 1, h.score, h.doc.title, h.doc.source);
                        println!("    {}", excerpt(&h.doc.text, 160));
                    }
                    Ok(())
                }
                Some("list") | None => {
                    println!("지식 디렉터리: {}", cfg.knowledge_path().display());
                    if kb.packs.is_empty() {
                        println!("  (팩 없음 — tools/knowledge_builder.py 로 만드세요)");
                    }
                    for p in &kb.packs {
                        println!("  {:<24} 문서 {:>6}  (건너뛴 줄 {})", p.name, p.docs, p.skipped_lines);
                    }
                    Ok(())
                }
                Some(other) => Err(format!("알 수 없는 knowledge 명령: {other}")),
            }
        }
        Some("identity") => {
            let cfg = load_config(engine_override)?;
            let path = cfg.identity_path();
            let mut id = Identity::load_or_default(&path)?;
            match args.get(1).map(String::as_str) {
                None | Some("show") => {
                    print_identity(&id);
                    println!("\n파일: {}", path.display());
                    Ok(())
                }
                Some("remember") => {
                    let text = args[2..].join(" ");
                    if text.trim().is_empty() {
                        return Err("기억할 내용을 입력하세요.".into());
                    }
                    let mem_id = id.remember(&text, vec![]).id.clone();
                    id.save(&path)?;
                    println!("기억했습니다: {mem_id}");
                    Ok(())
                }
                Some("forget") => {
                    let target = args.get(2).ok_or("삭제할 기억 id 를 입력하세요.")?;
                    if !id.forget(target) {
                        return Err(format!("기억 {target} 을(를) 찾지 못했습니다."));
                    }
                    id.save(&path)?;
                    println!("삭제했습니다: {target}");
                    Ok(())
                }
                Some(other) => Err(format!("알 수 없는 identity 명령: {other}")),
            }
        }
        Some(other) => Err(format!("알 수 없는 명령: {other}\n\n{HELP}")),
    }
}

fn init() -> Result<(), String> {
    let cfg_path = paths::config_file();
    if cfg_path.exists() {
        println!("설정 파일이 이미 있습니다: {}", cfg_path.display());
    } else {
        println!("설정 파일 생성: {}", Config::default().save()?.display());
    }
    let cfg = Config::load()?;
    let id_path = cfg.identity_path();
    if id_path.exists() {
        println!("인격체 파일이 이미 있습니다: {}", id_path.display());
    } else {
        Identity::default().save(&id_path)?;
        println!("인격체 파일 생성: {}", id_path.display());
    }
    for dir in [cfg.knowledge_path(), paths::models_dir()] {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        println!("디렉터리 준비: {}", dir.display());
    }
    Ok(())
}

fn excerpt(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

fn print_answer(a: &session::Answer) {
    println!("{}", a.text);
    if !a.sources.is_empty() {
        println!("\n  참고: {}", a.sources.join(" · "));
    }
    for n in &a.notes {
        println!("  ※ {n}");
    }
    println!("  — {}", a.engine);
}

fn print_identity(id: &Identity) {
    println!("{} ({}) — 호출명 {}", id.name, id.name_ko, id.callsign);
    println!("{}", id.persona.description);
    println!("말투: {} · 언어: {}", id.persona.tone, id.persona.language);
    if let Some(name) = &id.owner.name {
        println!("사용자: {name}");
    }
    println!("\n원칙:");
    for p in &id.principles {
        println!("  - {p}");
    }
    println!("\n기억 ({}개):", id.memories.len());
    for m in &id.memories {
        println!("  {}  {}", m.id, m.text);
    }
}

fn print_net(r: &net::NetReport) {
    println!("네트워크: {}   (수집: {})", r.summary(), r.source);
    if !r.reachable.is_empty() {
        println!("도달 가능: {}", r.reachable.join(", "));
    }
    for i in &r.interfaces {
        println!(
            "  {:<14} {:<10} {:<5} {}",
            i.name,
            i.kind,
            if i.up { "up" } else { "down" },
            i.addresses.join(" ")
        );
    }
    if let Some(w) = &r.wifi {
        let signal = w.signal.map(|s| format!("{s}%")).unwrap_or_else(|| "?".into());
        println!("Wi-Fi: \"{}\"  보안 {}  신호 {}", w.ssid, w.security, signal);
    }
    if !r.cellular.is_empty() {
        println!("셀룰러 모뎀:");
        for m in &r.cellular {
            println!("  {m}");
        }
    }
    for w in &r.warnings {
        println!("  ⚠ {w}");
    }
}

fn print_status(s: &Session) {
    println!("AlliedID {VERSION} — {} ({})", s.identity.name, s.identity.name_ko);
    println!("인격체 파일: {}", s.cfg.identity_path().display());
    println!("기억: {}개", s.identity.memories.len());
    println!("엔진 모드: {}", s.mode.as_str());
    match s.local().health() {
        Ok(()) => println!("  local : 준비됨 ({})", s.local().endpoint()),
        Err(e) => println!("  local : 꺼짐 ({}) — {e}", s.local().endpoint()),
    }
    if s.has_claude() {
        println!("  claude: API 키 있음 ({})", s.cfg.claude.model);
    } else {
        println!("  claude: API 키 없음 ({} 미설정)", s.cfg.claude.api_key_env);
    }
    println!(
        "지식 팩: {}개 팩, 문서 {}개 ({})",
        s.kb.packs.len(),
        s.kb.len(),
        s.cfg.knowledge_path().display()
    );
    println!();
    print_net(&s.net);
}

fn repl(mut s: Session) -> Result<(), String> {
    println!("{} ({}) — AlliedID {VERSION}", s.identity.name, s.identity.name_ko);
    println!("네트워크: {} · 엔진 모드: {} · /help 로 명령 보기", s.net.summary(), s.mode.as_str());
    for w in &s.net.warnings {
        println!("  ⚠ {w}");
    }

    let stdin = io::stdin();
    loop {
        print!("\n> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            println!();
            return Ok(());
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(cmd) = line.strip_prefix('/') {
            let (name, arg) = cmd.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((cmd, ""));
            match name {
                "exit" | "quit" | "q" => return Ok(()),
                "help" => print!("{HELP}"),
                "status" => print_status(&s),
                "net" => {
                    s.rescan();
                    print_net(&s.net);
                }
                "engine" => match EngineMode::parse(arg) {
                    Some(m) => {
                        s.mode = m;
                        println!("엔진 모드: {}", m.as_str());
                    }
                    None => println!("현재 엔진 모드: {} (auto|local|claude)", s.mode.as_str()),
                },
                "remember" if !arg.is_empty() => {
                    let id = s.identity.remember(arg, vec![]).id.clone();
                    match s.save_identity() {
                        Ok(()) => println!("기억했습니다: {id}"),
                        Err(e) => println!("저장 실패: {e}"),
                    }
                }
                "forget" if !arg.is_empty() => {
                    if s.identity.forget(arg) {
                        match s.save_identity() {
                            Ok(()) => println!("삭제했습니다: {arg}"),
                            Err(e) => println!("저장 실패: {e}"),
                        }
                    } else {
                        println!("기억 {arg} 을(를) 찾지 못했습니다.");
                    }
                }
                "memories" => {
                    for m in &s.identity.memories {
                        println!("  {}  {}", m.id, m.text);
                    }
                }
                "search" => {
                    for (i, h) in s.kb.search(arg, 5).iter().enumerate() {
                        println!("[{}] {} — {}\n    {}", i + 1, h.doc.title, h.doc.source, excerpt(&h.doc.text, 160));
                    }
                }
                "clear" => {
                    s.clear();
                    println!("대화 기록을 비웠습니다. (기억은 유지됩니다)");
                }
                _ => println!("알 수 없는 명령입니다. /help 를 입력하세요."),
            }
            continue;
        }

        match s.ask(line) {
            Ok(a) => {
                println!();
                print_answer(&a);
            }
            Err(e) => println!("\n응답을 받지 못했습니다:\n{e}"),
        }
    }
}
