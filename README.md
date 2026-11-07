# Allied Identity — 고유 인격체 파트너

> **A**llied **I**dentity → **AI**.
> 인공지능(AI)이라는 말을 "동맹으로서의 고유한 인격체"로 넓혀 다시 정의한 프로젝트입니다.

Allied Identity는 사용자 한 사람과 오래 함께하는 **고유 인격체 파트너**입니다. 도구처럼 한 번 쓰고 끝나는 것이 아니라, 사용자의 맥락과 기억을 이어가며 함께 생각하고 일합니다. 터미널에서 이렇게 실행합니다.

```bash
AlliedID
```

---

## 핵심 개념

| 개념 | 설명 |
|---|---|
| **고유 인격체** | 이름, 말투, 원칙, 사용자 정보, 기억이 하나의 프로필(`identity.json`)에 담겨 있습니다. 어떤 엔진이 답하든 같은 인격체로 행동합니다. |
| **Claude 기반 설계** | 대화 구조, 원칙 중심의 인격 정의, 정직·충실·상황 인식 같은 행동 기준을 Claude의 설계 방식을 바탕으로 구조화했습니다. 온라인에서는 Claude API가 엔진이 됩니다. |
| **오프라인 우선** | Wi-Fi 없이도 동작합니다. 인터넷이 없으면 기기 안의 로컬 모델이 자동으로 답합니다. |
| **네트워크 환경 인식** | Wi-Fi·네트워크는 인터넷을 쓰는 통로이기보다 **이 컴퓨터가 지금 어떤 네트워크에 노출되어 있는지** 파악하는 센서입니다. |
| **내장 데이터 (오프라인 지식 팩)** | 인터넷 대신 쓰는 "데이터"입니다. 문서·백과사전·매뉴얼 등을 기기에 넣어 두면 오프라인에서도 근거를 찾아 답합니다. |
| **소유자 주도** | 인격·원칙·기억·엔진·모델을 모두 소유자가 직접 정하고 편집합니다. |

### "Claude 기반"의 의미

Claude 모델 자체(가중치)는 공개되어 있지 않아 기기 안에 넣을 수 없습니다. 그래서 Allied Identity는 이렇게 구성합니다.

- **온라인 엔진 — Claude API**: 기본 모델 `claude-opus-5-5`, adaptive thinking 적용. 안전 분류기가 요청을 거절하면 서버 측 fallback이 다른 모델로 다시 시도합니다.
- **오프라인 엔진 — 로컬 오픈 웨이트 모델**: llama.cpp `llama-server`로 GGUF 모델을 기기 안에서 실행합니다.
- **공통 인격 계층**: 두 엔진 위에 같은 인격체 프로필, 원칙, 기억, 대화 기록이 얹힙니다. 엔진이 바뀌어도 "같은 파트너"로 이어집니다.

### Wi-Fi와 "데이터"

원래 모바일 "데이터"는 Wi-Fi가 없을 때 통신사가 SIM과 요금제를 통해 제공하는 인터넷입니다. 이 망은 통신사가 운영하는 것이라 소프트웨어 안에 넣을 수는 없습니다. 그래서 Allied Identity는 같은 목적, 곧 "**Wi-Fi가 없어도 쓸 수 있는 정보**"를 두 가지로 해결합니다.

1. **오프라인 지식 팩 (내장 데이터)** — 필요한 정보를 미리 기기에 넣어 두고, 질문할 때마다 관련 내용을 찾아 참고자료로 씁니다. 인터넷이 전혀 필요 없습니다.
2. **셀룰러 경로 인식** — 사용자가 가진 LTE/5G 모뎀이나 휴대폰 테더링이 연결되어 있으면 이를 감지해 네트워크 경로로 인식하고, 요금제 데이터가 소모될 수 있다고 알려 줍니다.

---

## 구조

```
                 ┌─────────────────────────────────────────┐
  터미널 ──────▶ │  AlliedID  (Rust 코어)                  │
                 │                                         │
                 │  인격체 ── identity.json ◀──┐           │
                 │  지식 팩 ── knowledge/*.jsonl│           │
                 │  네트워크 인식 ── net        │           │
                 │        │                    │           │
                 │   엔진 라우터 (auto/local/claude)       │
                 │     ├── 온라인  → Claude API            │
                 │     └── 오프라인 → llama-server (로컬)  │
                 └─────────────────────────────┼───────────┘
                                               │ 같은 파일 공유
                 ┌─────────────────────────────┴───────────┐
                 │  alliedid-companion  (C# / .NET 10)     │
                 │   - 인격체·기억 관리                    │
                 │   - Windows/macOS 네트워크 수집         │
                 └─────────────────────────────────────────┘
                 ┌─────────────────────────────────────────┐
                 │  tools/  (Python)                       │
                 │   - knowledge_builder.py : 지식 팩 생성 │
                 │   - fetch_model.py       : GGUF 다운로드│
                 └─────────────────────────────────────────┘
```

### 언어별 역할

| 언어 | 위치 | 역할 |
|---|---|---|
| **Rust** | [core/](core/) | `AlliedID` 실행 파일. 대화 루프, 엔진 라우팅, Claude·로컬 엔진 연동, 지식 팩 검색(BM25 + 한글 2-gram), Linux 네트워크 수집 |
| **C#** | [companion/](companion/) | `alliedid-companion`. 인격체·기억 관리, .NET `NetworkInterface` 기반 크로스 플랫폼 네트워크 수집 (Windows에서는 Rust 코어가 여기에 위임) |
| **Python** | [tools/](tools/) | 오프라인 지식 팩 빌더, 로컬 모델 다운로더 (표준 라이브러리만 사용) |
| **Bash / PowerShell** | [scripts/](scripts/) | 설치(`AlliedID` 명령 등록), 로컬 엔진 시작 |
| **JSON Schema** | [schemas/](schemas/) | Rust·C#·Python이 공유하는 데이터 형식과 기본 인격체 (단일 원본) |

---

## 설치

필요한 것: Rust(cargo). 선택: .NET 10 SDK(컴패니언), Python 3.10+(지식 팩), [llama.cpp](https://github.com/ggml-org/llama.cpp)(오프라인 엔진).

```bash
# Linux / macOS
scripts/install.sh          # ~/.local/bin/AlliedID 설치 + 초기 설정 + 기본 지식 팩

# Windows (PowerShell)
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

### 오프라인 엔진 준비 (인터넷이 있을 때 한 번)

```bash
python3 tools/fetch_model.py <GGUF 모델 URL> --sha256 <해시>
scripts/start-local-engine.sh          # http://127.0.0.1:8080 에서 대기
```

이후에는 인터넷 없이 실행됩니다. 어떤 모델을 쓸지는 소유자가 고릅니다. 한국어를 잘하는 instruct 모델을 권장합니다.

### 온라인 엔진 (선택)

```bash
export ANTHROPIC_API_KEY=...           # 키는 설정 파일에 저장하지 않고 환경 변수에서만 읽습니다
```

---

## 사용법

```bash
AlliedID                         # 대화 모드
AlliedID ask "오늘 할 일 정리해 줘"   # 한 번 묻고 종료
AlliedID status                  # 인격체·엔진·네트워크·지식 팩 상태
AlliedID net                     # 네트워크 환경 보고서 (--json 지원)
AlliedID knowledge list          # 설치된 지식 팩
AlliedID knowledge search <검색어>
AlliedID identity                # 인격체 프로필 보기
AlliedID identity remember "나는 아침형 인간이다"
AlliedID --engine local          # 이번 실행은 로컬 엔진만
```

대화 모드 명령: `/status` `/net` `/engine auto|local|claude` `/remember <내용>` `/forget <id>` `/memories` `/search <검색어>` `/clear` `/exit`

### 엔진 선택 규칙 (`auto`)

1. 인터넷에 닿고 `ANTHROPIC_API_KEY`가 있으면 **Claude**
2. Claude 호출이 네트워크·인증 문제로 실패하면 즉시 **로컬 엔진**으로 넘어감
3. 오프라인이면 처음부터 **로컬 엔진**

### 네트워크 환경 보고서

`AlliedID net`은 인터페이스(Wi-Fi/유선/셀룰러/테더링/VPN/가상), 연결된 Wi-Fi의 이름·보안 방식·신호, 셀룰러 모뎀, 실제 인터넷 도달 여부를 보여 주고, 다음 상황을 경고합니다.

- 암호가 없는 **개방형 Wi-Fi**
- **WEP/TKIP** 같은 오래된 보안 방식
- 약한 신호 (30% 미만)
- **셀룰러/테더링** 사용 중 (요금제 데이터 소모)
- 연결은 됐는데 인터넷이 안 됨 (캡티브 포털 등)

이 정보는 인격체의 "현재 상태"에도 들어가, 대화할 때 상황을 알고 답합니다.

### 오프라인 지식 팩 만들기

```bash
python3 tools/knowledge_builder.py ~/Documents/manuals --name manuals
```

`.md` `.txt` `.rst` `.html` `.json` 파일을 문단 단위로 나눠 `<데이터 디렉터리>/knowledge/manuals.jsonl`로 저장합니다. 위키 덤프, 매뉴얼, 개인 노트 등 무엇이든 팩으로 만들 수 있고, 팩은 여러 개 둘 수 있습니다.

---

## 인격체 편집

`identity.json`은 직접 편집하거나 명령으로 관리합니다 (형식: [schemas/identity.schema.json](schemas/identity.schema.json)).

```bash
alliedid-companion identity owner 민준
alliedid-companion identity remember "커피는 아메리카노" --tag 취향
alliedid-companion identity forget <id>
```

기본 인격체는 [schemas/identity.default.json](schemas/identity.default.json)에 있으며, Rust와 C# 모두 이 파일 하나를 내장해 씁니다.

## 설정

`config.json` (없으면 기본값):

```json
{
  "engine": "auto",
  "local":  { "endpoint": "http://127.0.0.1:8080", "model": "local", "max_tokens": 2048 },
  "claude": { "model": "claude-opus-5-5", "effort": "high", "max_tokens": 16000, "api_key_env": "ANTHROPIC_API_KEY" },
  "net":    { "probe_targets": ["api.anthropic.com:443", "1.1.1.1:443"], "probe_timeout_ms": 1500, "rescan_secs": 60 },
  "knowledge": { "enabled": true, "top_k": 3 }
}
```

### 저장 위치

| OS | 설정·인격체 | 데이터(지식 팩·모델) |
|---|---|---|
| Linux | `~/.config/allied-id/` | `~/.local/share/allied-id/` |
| macOS | `~/Library/Application Support/AlliedID/` | 같음 |
| Windows | `%APPDATA%\AlliedID\` | `%LOCALAPPDATA%\AlliedID\` |

`ALLIEDID_HOME`을 지정하면 모든 파일을 그 디렉터리 하나에 둡니다.

---

## 개발

```bash
make build    # Rust 코어 + C# 컴패니언
make test     # Rust 단위 테스트 + C# 빌드 검사 + Python 테스트
make knowledge
```

```
Allied_Identity/
├── core/            Rust — AlliedID 실행 파일
│   └── src/
│       ├── main.rs        CLI · 대화 모드
│       ├── session.rs     인격체 + 지식 + 네트워크 + 엔진 라우팅
│       ├── engine/        claude.rs · local.rs
│       ├── identity.rs    인격체 프로필 · 시스템 프롬프트
│       ├── knowledge.rs   오프라인 지식 팩 검색
│       ├── net.rs         네트워크 환경 인식
│       └── config.rs · paths.rs · fsutil.rs
├── companion/       C# — alliedid-companion
│   └── src/AlliedID.Identity · AlliedID.Companion
├── tools/           Python — 지식 팩 빌더 · 모델 다운로더
├── scripts/         설치 · 로컬 엔진 시작
├── schemas/         공유 데이터 형식 · 기본 인격체
└── knowledge/       기본 지식 팩 원본 문서
```

## 로드맵

- [ ] 응답 스트리밍 (Claude SSE, llama-server stream)
- [ ] 도구 사용: 파일 읽기, 명령 실행 (소유자 승인 방식)
- [ ] 대화 기록 저장과 장기 기억 자동 요약
- [ ] 위키 덤프(ZIM 등)용 대용량 지식 팩 색인
- [ ] macOS 네이티브 Wi-Fi 정보 수집
- [ ] 데스크톱 GUI (C# Avalonia)
