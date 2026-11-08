# 실행 방법

프로젝트 폴더에서 ./AlliedID 를 실행하면 필요한 빌드를 자동으로 하고 바로 시작해요. 처음 실행할 때 ~/.local/bin 에 AlliedID 명령이 등록되어서, 그다음부터는 어느 폴더에서든 AlliedID 만 입력하면 돼요. 필요한 것은 Rust(cargo)와 .NET 10 SDK예요.

# 대화 명령

대화 중에는 이런 명령을 쓸 수 있어요. /help 는 도움말, /think 는 생각 과정 보기 켜고 끄기, /effort low|medium|high 는 노력 수준 바꾸기, /teach 질문 => 답 은 가르치기, /good 과 /bad 는 직전 답 평가, /memories 는 기억 목록, /forge 는 데이터 다시 만들기, /status 는 상태 보기, /clear 는 대화 맥락 비우기, /exit 는 끝내기예요.

# 한 번만 묻기

AlliedID ask 질문 형태로 실행하면 한 번 답하고 끝나요. AlliedID status 는 본체와 데이터 상태를, AlliedID net 은 네트워크 환경을 보여 줘요.
