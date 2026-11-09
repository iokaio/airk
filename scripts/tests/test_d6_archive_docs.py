# SPDX-License-Identifier: Apache-2.0
"""Step D6a moves dated records and rewrites only the link spans that change."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "d6_archive_docs", Path(__file__).resolve().parents[1] / "dev" / "d6_archive_docs.py"
)
D6 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(D6)

MOVES = ["docs/old-plan.md", "docs/old-review.md"]
PINS = ["AGENTS.md", "CLAUDE.md", "docs/README.md"]
SHA = "0123456789abcdef0123456789abcdef01234567"

FILES = {
    "AGENTS.md": b"# Agents\n",
    "CLAUDE.md": b"# Agents\n",
    "src/code.rs": b"// code\n",
    "docs/current.md": b"# Current\n\n## Section\n",
    "docs/simplify-plan.md": b"# Plan\n",
    # Outbound links, a link between moved files and an anchor-only link.
    "docs/old-plan.md": (b"# Old plan (2026-10-01)\n\n[code](../src/code.rs) [current](current.md#section)\n"
                         b"[review](old-review.md#findings) [self](#top) [web](https://example.com/x.md)\n"),
    "docs/old-review.md": b"# Old review\n\n## Findings\n\nSee [plan](./old-plan.md).\n",
    # Inbound links, a reference definition, prose and GitHub URLs.
    "docs/README.md": (b"# Docs\n\n- [plan](old-plan.md#goals)\n- [review][r]\n- Prose names old-plan.md.\n\n"
                       b"[r]: old-review.md\n\n"
                       b"https://github.com/iokaio/airk/blob/main/docs/old-plan.md\n"
                       b"https://github.com/iokaio/airk/blob/feat/topic/docs/old-plan.md#goals\n"
                       b"https://github.com/iokaio/airk/blob/" + SHA.encode() + b"/docs/old-plan.md\n"
                       b"https://github.com/iokaio/airk/tree/v1.2.3/docs/old-plan.md\n"),
    "README.md": b"[plan](docs/old-plan.md) ![shot](docs/current.md)\n",
    "site/page.html": b'<a href="../docs/old-review.md">review</a> <img src="../docs/current.md">\n',
    # CRLF with a byte-order mark, and a cp1252 file with non-UTF-8 bytes.
    "docs/crlf.md": b"\xef\xbb\xbf# CRLF\r\n\r\nSee [plan](old-plan.md).\r\nTrailing line\r\n",
    "docs/releases/cp.md": b"# \x93Quoted\x94 caf\xe9\r\n\r\n[review](../old-review.md) \x96 end\r\n",
    "scripts/tool.py": b'# SPDX\nDOC = "docs/old-plan.md"\nOTHER = "docs/old-plan.md.bak"\n',
}


def run(root, *args):
    subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)


class ArchiveDocs(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        run(self.root, "init", "-q")
        run(self.root, "config", "core.autocrlf", "false")
        for relative, data in FILES.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        run(self.root, "add", "-A")

    def tearDown(self):
        self.temporary.cleanup()

    def read(self, relative):
        return (self.root / relative).read_bytes()

    def apply(self):
        plan = D6.build_plan(self.root, MOVES)
        self.assertEqual(plan.errors, [])
        D6.apply(self.root, plan)
        return plan

    def test_outbound_and_between_moved_links(self):
        self.apply()
        self.assertEqual(
            self.read("docs/history/old-plan.md"),
            b"# Old plan (2026-10-01)\n\n[code](../../src/code.rs) [current](../current.md#section)\n"
            b"[review](old-review.md#findings) [self](#top) [web](https://example.com/x.md)\n",
        )
        self.assertEqual(self.read("docs/history/old-review.md"), FILES["docs/old-review.md"])

    def test_inbound_links_reference_definitions_and_attributes(self):
        plan = self.apply()
        readme = self.read("docs/README.md")
        self.assertIn(b"- [plan](history/old-plan.md#goals)\n", readme)
        self.assertIn(b"[r]: history/old-review.md\n", readme)
        self.assertIn(b"- Prose names old-plan.md.\n", readme)
        self.assertIn(("docs/README.md", 5, "old-plan.md"), plan.prose)
        self.assertEqual(self.read("README.md"), b"[plan](docs/history/old-plan.md) ![shot](docs/current.md)\n")
        self.assertEqual(self.read("site/page.html"),
                         b'<a href="../docs/history/old-review.md">review</a> <img src="../docs/current.md">\n')
        self.assertEqual(self.read("scripts/tool.py"),
                         b'# SPDX\nDOC = "docs/history/old-plan.md"\nOTHER = "docs/old-plan.md.bak"\n')
        self.assertEqual(plan.counts["reference"], 1)
        self.assertEqual(plan.counts["attribute"], 1)
        self.assertEqual(plan.counts["path_string"], 1)

    def test_crlf_bom_and_cp1252_bytes_are_preserved(self):
        self.apply()
        self.assertEqual(self.read("docs/crlf.md"),
                         b"\xef\xbb\xbf# CRLF\r\n\r\nSee [plan](history/old-plan.md).\r\nTrailing line\r\n")
        self.assertEqual(self.read("docs/releases/cp.md"),
                         b"# \x93Quoted\x94 caf\xe9\r\n\r\n[review](../history/old-review.md) \x96 end\r\n")

    def test_branch_urls_rewritten_and_fixed_refs_kept(self):
        plan = self.apply()
        readme = self.read("docs/README.md")
        self.assertIn(b"/blob/main/docs/history/old-plan.md\n", readme)
        self.assertIn(b"/blob/feat/topic/docs/history/old-plan.md#goals\n", readme)
        self.assertIn(b"/blob/" + SHA.encode() + b"/docs/old-plan.md\n", readme)
        self.assertIn(b"/tree/v1.2.3/docs/old-plan.md\n", readme)
        self.assertEqual(plan.counts["github_url"], 2)

    def test_check_passes_after_apply_and_fails_on_a_broken_link(self):
        self.assertTrue(D6.check(self.root, MOVES, PINS))
        self.apply()
        self.assertEqual(D6.check(self.root, MOVES, PINS), [])
        index = self.read("docs/history/README.md").decode("utf-8")
        self.assertIn("not current guidance", index)
        self.assertLess(index.index("| Old plan (2026-10-01) | [old-plan.md](old-plan.md) | `docs/old-plan.md` |"),
                        index.index("| Old review | [old-review.md](old-review.md) | `docs/old-review.md` |"))
        (self.root / "docs/current.md").write_bytes(b"# Current\n\n[gone](old-plan.md)\n")
        problems = D6.check(self.root, MOVES, PINS)
        self.assertEqual(problems, ["broken inline link docs/current.md:3: old-plan.md"])

    def test_check_reports_stale_index_and_differing_agent_files(self):
        self.apply()
        (self.root / "docs/history/README.md").write_bytes(b"# edited\n")
        (self.root / "AGENTS.md").write_bytes(b"# Different\n")
        problems = D6.check(self.root, MOVES, PINS)
        self.assertIn("docs/history/README.md is missing or stale", problems)
        self.assertIn("AGENTS.md and CLAUDE.md differ", problems)

    def test_pinned_and_protected_paths_stop_the_plan(self):
        plan = D6.build_plan(self.root, ["docs/README.md"])
        self.assertIn("move touches pinned path docs/README.md", plan.errors)
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit):
            D6.apply(self.root, plan)
        self.assertTrue((self.root / "docs/README.md").is_file())
        crate_doc = self.root / "crates/demo/README.md"
        crate_doc.parent.mkdir(parents=True)
        crate_doc.write_bytes(b"[plan](../../docs/old-plan.md)\n")
        run(self.root, "add", "-A")
        plan = D6.build_plan(self.root, MOVES)
        self.assertEqual(plan.errors, ["edit needed in protected file crates/demo/README.md: 1 spans"])

    def test_reviewed_lists_are_disjoint_and_literal(self):
        self.assertEqual(len(D6.MOVES), len(set(D6.MOVES)))
        self.assertEqual([path for path in D6.MOVES if D6.pinned(path)], [])
        self.assertIn("docs/performance-improvements-1.8.2.md", D6.PINNED)
        self.assertTrue(all(path.startswith("docs/") and path.count("/") == 1 for path in D6.MOVES))


if __name__ == "__main__":
    unittest.main()
