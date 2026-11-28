# SPDX-License-Identifier: Apache-2.0
"""Current workspace Gather must preserve native path eligibility across edits and pages."""

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import Mock, patch

import common
import gather_context_mcp
import live_gather
import qwen_file_trial
import workspace_scope


def page(paths, *, cursor=None, truncated=False, token="r1", index="index", fingerprint="f1"):
    body = {"paths": paths, "cursor": cursor, "truncated": truncated, "publication": {"token": token},
            "scope_observation": {"fingerprint": fingerprint}, "read_scope": {"index_ids": [index]},
            "discovery_readiness": {"state": "ready"}}
    return {"result": {"structuredContent": body}}


class ScopeTest(unittest.TestCase):
    def test_continues_only_the_same_filtered_inventory(self):
        client = Mock()
        client.exchange.side_effect = [page(["a.py"], truncated=True, cursor="next"), page(["c.py"])]
        metrics = {}
        self.assertEqual(workspace_scope.published_paths(client, "index", metrics), {"a.py", "c.py"})
        args = [c.args[0]["params"]["arguments"] for c in client.exchange.call_args_list]
        self.assertEqual(args[1], {**args[0], "cursor": "next"})
        self.assertEqual((metrics["publication"], metrics["inventory_pages"]), ("r1", 2))

    def test_missing_policy_and_conflicting_denied_paths_are_refused(self):
        for rules in (None, [["glob", "*"]], [["exact", "private/x"]], [["subtree", "private"]]):
            with self.subTest(rules=rules), self.assertRaises(ValueError):
                workspace_scope.check_denies({"private/x"}, rules)
        workspace_scope.check_denies({"private2/x"}, [["subtree", "private"], ["exact", "other/x"]])

    def test_cached_inventory_requires_a_fresh_matching_first_page(self):
        client = Mock()
        first = page(["a.py"], truncated=True, cursor="next")
        changed = page(["c.py"], truncated=True, cursor="changed", token="r2")
        client.exchange.side_effect = [first, page(["b.py"]), first, changed, page(["d.py"], token="r2"),
                                       page(["e.py"], token="r2", fingerprint="updated-policy")]
        cache, metrics = workspace_scope.InventoryCache(), {}
        self.assertEqual(workspace_scope.published_paths(client, "index", metrics, cache), {"a.py", "b.py"})
        self.assertEqual((metrics["inventory_pages"], metrics["inventory_cached"]), (2, False))
        self.assertEqual(workspace_scope.published_paths(client, "index", metrics, cache), {"a.py", "b.py"})
        self.assertEqual((metrics["inventory_pages"], metrics["inventory_cached"]), (1, True))
        self.assertEqual(workspace_scope.published_paths(client, "index", metrics, cache), {"c.py", "d.py"})
        self.assertEqual(workspace_scope.published_paths(client, "index", metrics, cache), {"e.py"})

    def test_incomplete_mixed_or_wrong_scope_is_never_used(self):
        first = page(["a.py"], truncated=True, cursor="next")
        for replies in ([page([], truncated=True)], [first, page(["b.py"], token="r2")],
                        [first, page(["b.py"], fingerprint="other")], [first, page(["b.py"], index="other")],
                        [first, page(["a.py"])], [first, page(["b.py"], truncated=True, cursor="next")],
                        [{"result": {"isError": True}}], [page([], cursor="unexpected")]):
            with self.subTest(replies=replies), self.assertRaises(ValueError):
                client = Mock()
                client.exchange.side_effect = replies
                workspace_scope.published_paths(client, "index")


class WorkspaceRefreshTest(unittest.TestCase):
    def test_backend_refuses_an_unavailable_inventory_after_a_success(self):
        with TemporaryDirectory() as tmp:
            source = Path(tmp)
            (source / "src").mkdir()
            (source / "src" / "package.txt").write_text("One package holds six tiles.\n")
            args = {"task": "Read src/package.txt"}
            score = Mock(return_value=[1.0])
            call = gather_context_mcp.live_backend(source, None, "qwen-focus", file_score=score, handle=object())
            with patch("workspace_scope.eligible_paths", side_effect=[{"src/package.txt"}, ValueError("Changed scope")]):
                text, failed, _ = call("airk_answer", "synthetic", args)
                self.assertFalse(failed)
                self.assertIn("six tiles", text)
                self.assertEqual(call.capture["files_reparsed"], 1)
                example = call.capture["observed_scorer_pair"]
                instruction, pairs = score.call_args.args
                self.assertEqual((example["instruction"], example["query"], example["card"]), (instruction, *pairs[0]))
                self.assertEqual(example["score"], 1.0)
                self.assertIn(example["card"], example["before_token_cap"])
                self.assertNotIn("before_token_cap", text)
                text, failed, _ = call("airk_answer", "synthetic", args)
                self.assertTrue(failed)
                self.assertIn("workspace_evidence_unavailable", text)
                self.assertNotIn("six tiles", text)
                score.assert_called_once()

    def test_same_size_edits_denies_and_publication_lag(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "src").mkdir()
            original = root / "src/packing.py"
            original.write_text("class AtlasCarton:\n    count = 12\n")
            (root / "do-not-read.py").write_text("class SecretCarton:\n    count = 64\n")
            renamed = root / "new.py"
            repo = live_gather.WorkspaceRepository(root)
            real_read = Path.read_bytes

            def allowed_read(path):
                self.assertNotEqual(path.name, "do-not-read.py")
                return real_read(path)

            with patch.object(Path, "read_bytes", allowed_read):
                repo.refresh({"src/packing.py"})
                self.assertIn("AtlasCarton", repo.index["word_names"]["carton"])
                self.assertNotIn("SecretCarton", repo.index["word_names"]["carton"])
                original.write_text("class LumenCarton:\n    count = 18\n")
                renamed.write_text("class FreshCarton:\n    count = 24\n")
                repo.refresh({"src/packing.py"})
                self.assertEqual(repo.capture["files_reparsed"], 1)
                self.assertEqual(repo.index["word_names"]["carton"], ["LumenCarton"])
                pairs = []

                def score(instruction, candidates):
                    pairs.extend(candidates)
                    return [1.0] * len(candidates)

                reply = qwen_file_trial.live_reply(repo, {"task": "Find LumenCarton in src/packing.py", "anchors": ["LumenCarton"]}, score)
                self.assertTrue(any("count = 18" in pair[1] for pair in pairs))
                self.assertTrue(any("count = 18" in p["text"] for p in reply["passages"]))
                self.assertTrue(all("SecretCarton" not in pair[1] for pair in pairs))
                original_pairs = list(pairs)
                pairs.clear()
                observed = {}
                measured = qwen_file_trial.live_reply(repo, {"task": "Find LumenCarton in src/packing.py",
                                                             "anchors": ["LumenCarton"]}, score, observed)
                self.assertEqual(original_pairs, pairs)
                self.assertEqual(observed["before_token_cap_sha256"], common.sha256_bytes(observed["before_token_cap"].encode()))
                self.assertEqual({k: v for k, v in reply.items() if k != "elapsed_ms"},
                                 {k: v for k, v in measured.items() if k != "elapsed_ms"})
                repo.refresh({"src/packing.py", "new.py"})
                self.assertEqual(repo.capture["files_reparsed"], 1)
                self.assertEqual(repo.index["word_names"]["carton"], ["FreshCarton", "LumenCarton"])
                repo.refresh({"src/packing.py", "new.py"})
                self.assertEqual(repo.capture["files_reparsed"], 0)
                original.unlink()
                repo.refresh({"src/packing.py", "new.py"})
                self.assertEqual(set(repo.texts), {"new.py"})
                repo.refresh(set())
                self.assertEqual(repo.index["files"], {})
                with self.assertRaises(ValueError):
                    repo.refresh(None)
