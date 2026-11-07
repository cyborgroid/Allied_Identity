#!/usr/bin/env bash
# 오프라인 엔진(llama.cpp llama-server) 시작.
#   scripts/start-local-engine.sh [모델.gguf] [포트]
# 모델을 지정하지 않으면 데이터 디렉터리의 models/ 에서 첫 번째 .gguf 를 쓴다.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${2:-8080}"

MODEL="${1:-}"
if [[ -z "$MODEL" ]]; then
  MODELS_DIR="$(python3 -c 'import sys; sys.path.insert(0, "'"$ROOT"'/tools"); from knowledge_builder import data_dir; print(data_dir() / "models")')"
  MODEL="$(find "$MODELS_DIR" -maxdepth 1 -name '*.gguf' 2>/dev/null | sort | head -n1 || true)"
  [[ -n "$MODEL" ]] || { echo "모델이 없습니다. 먼저: python3 tools/fetch_model.py <GGUF URL>" >&2; exit 1; }
fi

command -v llama-server >/dev/null || {
  echo "llama-server 가 필요합니다 (https://github.com/ggml-org/llama.cpp)." >&2
  exit 1
}

echo "모델: $MODEL"
echo "주소: http://127.0.0.1:$PORT  (AlliedID 의 local.endpoint 와 같아야 합니다)"
exec llama-server -m "$MODEL" --host 127.0.0.1 --port "$PORT" --ctx-size 8192 --jinja
