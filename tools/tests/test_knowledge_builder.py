import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import knowledge_builder as kb  # noqa: E402


class ChunkTests(unittest.TestCase):
    def test_short_paragraphs_merge(self):
        chunks = kb.chunk_text("하나.\n\n둘.\n\n셋.", 100)
        self.assertEqual(chunks, ["하나.\n\n둘.\n\n셋."])

    def test_respects_max_chars(self):
        text = "\n\n".join(["가" * 50] * 10)
        chunks = kb.chunk_text(text, 120)
        self.assertTrue(all(len(c) <= 120 for c in chunks))
        self.assertEqual("".join(chunks).count("가"), 500)

    def test_long_paragraph_is_split(self):
        chunks = kb.chunk_text("A" * 1000, 300)
        self.assertTrue(all(len(c) <= 300 for c in chunks))
        self.assertEqual(sum(len(c) for c in chunks), 1000)


class BuildTests(unittest.TestCase):
    def test_build_and_write_pack(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "docs"
            src.mkdir()
            (src / "net.md").write_text("# 네트워크\n\n와이파이 점검 방법.", encoding="utf-8")
            (src / "page.html").write_text(
                "<html><title>도움말</title><script>x()</script><p>본문</p></html>", encoding="utf-8"
            )
            out = Path(tmp) / "out.jsonl"
            self.assertEqual(kb.main([str(src), "--name", "t", "-o", str(out)]), 0)
            docs = [json.loads(line) for line in out.read_text(encoding="utf-8").splitlines()]
            by_source = {d["source"]: d for d in docs}
            self.assertEqual(by_source["net.md"]["title"], "네트워크")
            self.assertEqual(by_source["page.html"]["title"], "도움말")
            self.assertNotIn("x()", by_source["page.html"]["text"])
            self.assertTrue(all(d["id"].startswith("t:") for d in docs))


if __name__ == "__main__":
    unittest.main()
