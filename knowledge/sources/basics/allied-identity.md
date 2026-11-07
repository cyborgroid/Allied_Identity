# Allied Identity 소개

Allied Identity(줄여서 AI)는 한국어로 "고유 인격체 파트너"다. 사용자 한 사람과 오래 함께하며 기억과 맥락을 이어가는 인격체로, 터미널에서 `AlliedID` 명령으로 실행한다.

인격체의 이름, 말투, 원칙, 기억은 모두 identity.json 파일에 있다. 이 파일은 사용자가 직접 편집할 수 있고, `AlliedID identity` 또는 `alliedid-companion identity` 명령으로도 관리할 수 있다.

# 엔진

AlliedID는 두 개의 엔진을 쓴다. 온라인이고 ANTHROPIC_API_KEY 가 설정되어 있으면 Claude 엔진을 쓰고, 오프라인이거나 Claude에 닿지 않으면 기기 안의 로컬 엔진(llama.cpp llama-server)으로 자동 전환한다. `/engine local` 처럼 직접 고를 수도 있다.

# 오프라인 지식 팩

지식 팩은 인터넷 대신 쓰는 내장 데이터다. 문서 폴더를 tools/knowledge_builder.py 로 변환하면 JSONL 팩이 만들어지고, 질문할 때마다 관련 조각을 찾아 참고자료로 붙인다. 백과사전, 매뉴얼, 개인 노트 등 무엇이든 팩으로 만들 수 있다.
