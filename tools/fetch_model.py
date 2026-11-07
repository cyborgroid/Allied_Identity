#!/usr/bin/env python3
"""오프라인 엔진용 GGUF 모델 내려받기 (온라인일 때 한 번만 실행).

    python3 tools/fetch_model.py <URL> [--sha256 <hex>] [--name model.gguf]

내려받은 모델은 <데이터 디렉터리>/models/ 에 저장되며, 이어받기(resume)를 지원한다.
이후에는 인터넷 없이 scripts/start-local-engine.sh 로 실행한다.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import urllib.request
from pathlib import Path
from urllib.parse import urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
from knowledge_builder import data_dir  # noqa: E402


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def download(url: str, dest: Path) -> None:
    part = dest.with_suffix(dest.suffix + ".part")
    have = part.stat().st_size if part.exists() else 0
    req = urllib.request.Request(url, headers={"User-Agent": "AlliedID-fetch-model/0.1"})
    if have:
        req.add_header("Range", f"bytes={have}-")
    with urllib.request.urlopen(req, timeout=60) as resp:
        if have and resp.status != 206:
            have = 0  # 서버가 이어받기를 지원하지 않음 → 처음부터
        total = have + int(resp.headers.get("Content-Length") or 0)
        mode = "ab" if have else "wb"
        done = have
        with part.open(mode) as f:
            while block := resp.read(1 << 20):
                f.write(block)
                done += len(block)
                if total:
                    print(f"\r  {done / 1e9:6.2f} / {total / 1e9:.2f} GB ({done * 100 // total}%)", end="", flush=True)
    print()
    part.replace(dest)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="AlliedID 로컬 모델(GGUF) 내려받기")
    ap.add_argument("url")
    ap.add_argument("--sha256", help="검증할 SHA-256 (선택)")
    ap.add_argument("--name", help="저장 파일 이름 (기본: URL 의 파일 이름)")
    args = ap.parse_args(argv)

    name = args.name or Path(urlparse(args.url).path).name
    if not name.endswith(".gguf"):
        ap.error("GGUF 파일이어야 합니다 (--name 으로 지정 가능).")
    dest = data_dir() / "models" / name
    dest.parent.mkdir(parents=True, exist_ok=True)

    if dest.exists():
        print(f"이미 있음: {dest}")
    else:
        print(f"내려받는 중: {args.url}\n  → {dest}")
        download(args.url, dest)

    if args.sha256:
        actual = sha256_of(dest)
        if actual.lower() != args.sha256.lower():
            print(f"SHA-256 불일치!\n  기대: {args.sha256}\n  실제: {actual}", file=sys.stderr)
            return 1
        print("SHA-256 확인 완료")
    print(f"\n실행: scripts/start-local-engine.sh {dest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
