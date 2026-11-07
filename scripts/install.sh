#!/usr/bin/env bash
# AlliedID 설치 (Linux / macOS)
#   - Rust 코어를 빌드해 `AlliedID` 명령으로 설치
#   - C# 컴패니언(alliedid-companion) 빌드·설치 (dotnet 이 있을 때)
#   - 기본 설정·인격체 생성, 기본 지식 팩 빌드
#
# 사용법: scripts/install.sh [--prefix ~/.local] [--no-companion]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${HOME}/.local"
COMPANION=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    --no-companion) COMPANION=0; shift ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "알 수 없는 옵션: $1" >&2; exit 2 ;;
  esac
done

BIN="${PREFIX}/bin"
mkdir -p "$BIN"

echo "==> Rust 코어 빌드"
command -v cargo >/dev/null || { echo "cargo 가 필요합니다: https://rustup.rs" >&2; exit 1; }
cargo build --release --manifest-path "$ROOT/core/Cargo.toml"
install -m 0755 "$ROOT/core/target/release/AlliedID" "$BIN/AlliedID"
echo "    설치: $BIN/AlliedID"

if [[ $COMPANION -eq 1 ]]; then
  if command -v dotnet >/dev/null; then
    echo "==> C# 컴패니언 빌드"
    DATA_DIR="$(python3 -c 'import sys; sys.path.insert(0, "'"$ROOT"'/tools"); from knowledge_builder import data_dir; print(data_dir())')"
    OUT="$DATA_DIR/companion"
    dotnet publish "$ROOT/companion/src/AlliedID.Companion/AlliedID.Companion.csproj" -c Release -o "$OUT" --nologo -v quiet
    ln -sf "$OUT/alliedid-companion" "$BIN/alliedid-companion"
    echo "    설치: $BIN/alliedid-companion -> $OUT"
  else
    echo "==> dotnet 이 없어 C# 컴패니언은 건너뜁니다 (Linux 에서는 없어도 동작)."
  fi
fi

echo "==> 초기 설정"
"$BIN/AlliedID" init

if command -v python3 >/dev/null; then
  echo "==> 기본 지식 팩 빌드"
  python3 "$ROOT/tools/knowledge_builder.py" "$ROOT/knowledge/sources/basics" --name allied-id-basics
fi

case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo; echo "※ $BIN 이 PATH 에 없습니다. 셸 설정에 추가하세요:"; echo "    export PATH=\"$BIN:\$PATH\"" ;;
esac

echo
echo "완료. 터미널에서 AlliedID 를 입력하세요."
echo "오프라인 엔진: python3 tools/fetch_model.py <GGUF URL>  →  scripts/start-local-engine.sh"
