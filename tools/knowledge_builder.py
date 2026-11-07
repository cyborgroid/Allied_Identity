#!/usr/bin/env python3
"""AlliedID 오프라인 지식 팩 빌더.

인터넷 대신 쓸 "데이터"를 만든다. 문서 폴더(.md .txt .html .rst .json)를 읽어
문단 단위 조각으로 나누고, Rust 코어가 읽는 JSONL 팩으로 저장한다.

    python3 tools/knowledge_builder.py ./my-docs --name my-docs
    python3 tools/knowledge_builder.py ./wiki --name wiki --chunk 1500 -o /tmp/wiki.jsonl

표준 라이브러리만 사용한다(오프라인 환경에서도 바로 실행 가능).
"""

from __future__ import annotations

import argparse
import html.parser
import json
import os
import re
import sys
from pathlib import Path
from typing import Iterator

TEXT_EXTS = {".md", ".markdown", ".txt", ".rst"}
HTML_EXTS = {".html", ".htm"}
JSON_EXTS = {".json"}


def data_dir() -> Path:
    """core/src/paths.rs 와 같은 규칙."""
    override = os.environ.get("ALLIEDID_HOME")
    if override:
        return Path(override)
    home = Path.home()
    if sys.platform == "win32":
        return Path(os.environ.get("LOCALAPPDATA", home)) / "AlliedID"
    if sys.platform == "darwin":
        return home / "Library" / "Application Support" / "AlliedID"
    return Path(os.environ.get("XDG_DATA_HOME", home / ".local" / "share")) / "allied-id"


class _TextExtractor(html.parser.HTMLParser):
    SKIP = {"script", "style", "nav", "footer", "header", "noscript"}
    BLOCK = {"p", "div", "br", "li", "h1", "h2", "h3", "h4", "h5", "h6", "tr", "section", "article"}

    def __init__(self) -> None:
        super().__init__()
        self.parts: list[str] = []
        self.title = ""
        self._skip = 0
        self._in_title = False

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self._skip += 1
        if tag == "title":
            self._in_title = True
        if tag in self.BLOCK:
            self.parts.append("\n\n")

    def handle_endtag(self, tag):
        if tag in self.SKIP and self._skip:
            self._skip -= 1
        if tag == "title":
            self._in_title = False

    def handle_data(self, data):
        if self._in_title:
            self.title += data
        elif not self._skip:
            self.parts.append(data)


def read_document(path: Path) -> tuple[str, str]:
    """(title, text) 를 돌려준다."""
    raw = path.read_text(encoding="utf-8", errors="replace")
    ext = path.suffix.lower()
    if ext in HTML_EXTS:
        p = _TextExtractor()
        p.feed(raw)
        return p.title.strip() or path.stem, "".join(p.parts)
    if ext in JSON_EXTS:
        obj = json.loads(raw)
        if isinstance(obj, dict) and "text" in obj:
            return str(obj.get("title") or path.stem), str(obj["text"])
        return path.stem, json.dumps(obj, ensure_ascii=False, indent=1)
    m = re.search(r"^#\s+(.+)$", raw, re.MULTILINE)
    return (m.group(1).strip() if m else path.stem), raw


def chunk_text(text: str, max_chars: int) -> list[str]:
    """빈 줄 기준 문단을 모아 max_chars 이하 조각으로 만든다. 너무 긴 문단은 문장 단위로 자른다."""
    paragraphs = [re.sub(r"[ \t]+", " ", p).strip() for p in re.split(r"\n\s*\n", text)]
    paragraphs = [p for p in paragraphs if p]

    pieces: list[str] = []
    for p in paragraphs:
        if len(p) <= max_chars:
            pieces.append(p)
            continue
        sentence_buf = ""
        for s in re.split(r"(?<=[.!?。])\s+", p):
            if not s:
                continue
            if len(sentence_buf) + len(s) + 1 > max_chars and sentence_buf:
                pieces.append(sentence_buf.strip())
                sentence_buf = ""
            while len(s) > max_chars:
                pieces.append(s[:max_chars])
                s = s[max_chars:]
            sentence_buf += s + " "
        if sentence_buf.strip():
            pieces.append(sentence_buf.strip())

    chunks: list[str] = []
    buf = ""
    for piece in pieces:
        if buf and len(buf) + len(piece) + 2 > max_chars:
            chunks.append(buf)
            buf = ""
        buf = f"{buf}\n\n{piece}" if buf else piece
    if buf:
        chunks.append(buf)
    return chunks


def iter_files(src: Path) -> Iterator[Path]:
    exts = TEXT_EXTS | HTML_EXTS | JSON_EXTS
    if src.is_file():
        yield src
        return
    for path in sorted(src.rglob("*")):
        if path.is_file() and path.suffix.lower() in exts and not any(part.startswith(".") for part in path.parts):
            yield path


def build(src: Path, name: str, max_chars: int) -> list[dict]:
    docs = []
    base = src if src.is_dir() else src.parent
    for path in iter_files(src):
        try:
            title, text = read_document(path)
        except (OSError, json.JSONDecodeError) as e:
            print(f"건너뜀 {path}: {e}", file=sys.stderr)
            continue
        rel = path.relative_to(base).as_posix()
        for i, chunk in enumerate(chunk_text(text, max_chars)):
            docs.append({"id": f"{name}:{rel}#{i}", "title": title, "source": rel, "text": chunk})
    return docs


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="AlliedID 오프라인 지식 팩 빌더")
    ap.add_argument("src", type=Path, help="문서 폴더 또는 파일")
    ap.add_argument("--name", help="팩 이름 (기본: 폴더 이름)")
    ap.add_argument("--chunk", type=int, default=1200, help="조각 최대 글자 수 (기본 1200)")
    ap.add_argument("-o", "--out", type=Path, help="출력 경로 (기본: <데이터 디렉터리>/knowledge/<name>.jsonl)")
    args = ap.parse_args(argv)

    if not args.src.exists():
        ap.error(f"{args.src} 가 없습니다.")
    name = args.name or args.src.resolve().stem
    if not re.fullmatch(r"[\w.-]+", name):
        ap.error("팩 이름에는 글자, 숫자, '.', '-', '_' 만 쓸 수 있습니다.")

    docs = build(args.src, name, args.chunk)
    out = args.out or data_dir() / "knowledge" / f"{name}.jsonl"
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(".jsonl.tmp")
    with tmp.open("w", encoding="utf-8") as f:
        for d in docs:
            f.write(json.dumps(d, ensure_ascii=False) + "\n")
    tmp.replace(out)
    print(f"{len(docs)}개 조각 → {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
