# SPDX-License-Identifier: Apache-2.0
import runpy
from pathlib import Path
import unittest

inspect = runpy.run_path(str(Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission/techqa_normal_visibility.py"))["inspect_menu"]


class OriginalMenu(unittest.TestCase):
    docs = [{"name": "a.txt", "bytes": 3, "sha256": "aaa", "publisher_id": "a"},
            {"name": "b.txt", "bytes": 4, "sha256": "bbb", "publisher_id": "b"}]

    def observe(self, paths, *, truncated=False, wrong_hash=False, wrong_scope=False, wrong_revision=False):
        scope = {"fingerprint": "one"}
        publication = {"token": "p"}
        status = {"scope_observation": scope, "publication": publication, "files": len(paths),
                  "read_scope": {"source": "named", "index_ids": ["idx"]},
                  "discovery_readiness": {"state": "ready"}, "semantic_readiness": {"state": "disabled"}}
        def call(label, tool, args):
            body = {"publication": dict(publication, generation=2 if wrong_revision and tool == "airk_file_info" else 1), "scope_observation": {"fingerprint": "other"} if wrong_scope else scope}
            if tool == "airk_list_files":
                return dict(body, paths=paths, truncated=truncated, cursor=None)
            doc, = [d for d in self.docs if d["name"] == args["path"]]
            return dict(body, path=doc["name"], source_bytes=doc["bytes"], content_hash="bad" if wrong_hash else doc["sha256"])
        return inspect(self.docs, status, "idx", call)

    def test_order_and_omission(self):
        result = self.observe(["b.txt"])
        self.assertEqual([e["visible"] for e in result["ordered_members"]], [False, True])
        self.assertEqual((result["visible"], result["omitted"]), (1, 1))
        self.assertEqual([e["publisher_id"] for e in self.observe(["b.txt", "a.txt"])["ordered_members"]], ["a", "b"])

    def test_reject_incomplete_foreign_or_duplicate_list(self):
        for paths, truncated in ((["a.txt"], True), (["evil.txt"], False), (["a.txt", "a.txt"], False)):
            with self.assertRaises(ValueError):
                self.observe(paths, truncated=truncated)

    def test_reject_mismatched_native_hash_or_grant(self):
        for kw in ({"wrong_scope": True}, {"wrong_hash": True}, {"wrong_revision": True}):
            with self.assertRaises(ValueError):
                self.observe(["a.txt"], **kw)


if __name__ == "__main__":
    unittest.main()
