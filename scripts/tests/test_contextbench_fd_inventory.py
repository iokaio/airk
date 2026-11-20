# SPDX-License-Identifier: Apache-2.0
import io
from pathlib import Path
import runpy
import tarfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
audit = runpy.run_path(str(ROOT / "contextbench_fd_sources.py"))
stage = runpy.run_path(str(ROOT / "stage_contextbench_fd_sources.py"))
native = runpy.run_path(str(ROOT / "verify_fd_native_inventory.py"))


class WholeTreeTests(unittest.TestCase):
    def test_rejects_unselected_path_before_extraction(self):
        base = "b" * 40
        blob = io.BytesIO()
        with tarfile.open(fileobj=blob, mode="w:gz") as tar:
            for name in ("legitimate.txt", "../escape.txt"):
                member = tarfile.TarInfo("fd-" + base + "/" + name)
                member.size = 1
                tar.addfile(member, io.BytesIO(b"x"))
        class Response(io.BytesIO):
            status = 200
            def geturl(self):
                return "https://codeload.github.com/sharkdp/fd/tar.gz/" + base
        revision = {"base": base, "entries": [["legitimate.txt", "100644", audit["identifier"](b"x"), 1]]}
        with patch.dict(audit["fetch"].__globals__, {"urlopen": lambda *args, **kwargs: Response(blob.getvalue())}):
            with self.assertRaisesRegex(ValueError, "archive path"):
                audit["fetch"](revision)

    def test_unexpected_lock_data_refused_before_dependency_use(self):
        with self.assertRaisesRegex(ValueError, "more than package metadata"):
            stage["dependency"](b'[[package]]\nname="lib"\nimplementation="foreign"\nversion="1"\n', b"")

    def test_source_omission_retained_and_hash_mismatch_refused(self):
        files = [{"path": path, "oid": "git", "sha256": "digest", "bytes": 8} for path in ("indexed.rs", ".hidden")]
        cases = [{"selector": {"base": "test"}, "documents": files}]
        def cli(label, tool, args):
            if tool == "airk_list_files":
                return {"scope_observation": "scope", "paths": ["indexed.rs"], "truncated": False}
            if tool == "airk_status":
                return {"scope_observation": "scope", "files": 1}
            return {"path": "indexed.rs", "scope_observation": "scope", "source_bytes": 8, "content_hash": "digest",
                    "chunks": 1, "extraction_method": "source", "parse_status": "parsed"}
        row, = native["observe"](None, cases, None, cli)
        self.assertEqual(row["absent_from_complete_native_list"], [".hidden"])
        with self.assertRaisesRegex(ValueError, "native blob"):
            native["observe"](None, cases, None, lambda a,b,c: cli(a,b,c) | ({"content_hash":"wrong"} if b == "airk_file_info" else {}))


if __name__ == "__main__":
    unittest.main()
