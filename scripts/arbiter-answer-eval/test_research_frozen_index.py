# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for frozen research admission and invariants."""

import hashlib
import unittest
from pathlib import Path
from unittest.mock import patch

import common
import research_frozen_index as frozen


class FrozenResearchTest(unittest.TestCase):
    def test_only_research_with_authenticated_solver_and_rubric_is_admitted(self):
        path = Path(__file__).resolve()
        repo = "bookbuilder-v2"
        task = {"id": "sample", "repository": "bookbuilder", "kind": "research"}
        rows = [{"id": name, "repository": repo, "kind": "research", "solver_sha256": "ab", "grader_sha256": "cd"}
                for name in ("sample", "r2", "r3", "r4")]
        design = path.parent
        solver = design / "solver/sample.json"
        manifest = design.parent / "E2E-r1/native-state/frozen-eligible-r3.json"
        with (patch.object(Path, "resolve", lambda p, **kwargs: p),
              patch.object(common, "read_json", return_value={"tasks": rows}),
              patch.object(common, "sha256_file", side_effect=[frozen.FREEZE, "ab", "cd"])):
            self.assertEqual(frozen.admit(solver, task, repo, manifest), rows[0])
        for kind, digests in (("coding", [frozen.FREEZE]), ("research", ["bad-freeze"]),
                              ("research", [frozen.FREEZE, "bad-solver"]), ("research", [frozen.FREEZE, "ab", "bad-rubric"])):
            with (self.subTest(kind=kind, digests=digests), patch.object(Path, "resolve", lambda p, **kwargs: p),
                  patch.object(common, "read_json", return_value={"tasks": rows}),
                  patch.object(common, "sha256_file", side_effect=digests), self.assertRaises(common.HarnessError)):
                frozen.admit(solver, {**task, "kind": kind}, repo, manifest)

    def test_paginated_inventory_refuses_policy_change_under_the_same_grant(self):
        from unittest.mock import Mock
        from test_workspace_scope import page

        first = page(["a.py"], truncated=True, cursor="next")
        second = page(["b.py"])
        first["result"]["structuredContent"]["scope_observation"]["members"] = [{"policy_epoch": 1}]
        second["result"]["structuredContent"]["scope_observation"]["members"] = [{"policy_epoch": 2}]
        client = Mock()
        client.exchange.side_effect = [first, second]
        with self.assertRaisesRegex(ValueError, "authorization changed"):
            frozen.workspace_scope.published_paths(client, "index", observation={})

    def test_eligible_digest_binds_relative_name_and_raw_digest(self):
        here = Path(__file__).parent
        names = ["agent_cost_run.py", "common.py"]
        expected = hashlib.sha256()
        for name in names:
            expected.update(name.encode() + b"\0" + hashlib.sha256((here / name).read_bytes()).digest())
        self.assertEqual(frozen.eligible_digest(here, reversed(names)), expected.hexdigest())

    def test_unsafe_inventory_names_fail_before_open(self):
        for bad in ("../source", "/absolute", "a//b", "a\\b", "C:escape", "source:stream", "dir./name", "CON"):
            with self.subTest(path=bad), self.assertRaises(common.HarnessError):
                frozen.eligible_digest(Path(__file__).parent, [bad])

    def test_finish_requires_answer_and_identical_scope_tree_without_edits(self):
        pre = {"publication": "pub", "scope_fingerprint": "fp", "tree_digest": "tree", "inventory_seconds": 1}
        pre["scope_observation"] = {"fingerprint": "fp", "members": ["same"]}
        post = {**pre, "inventory_seconds": 2, "scope_fingerprint": "new-grant",
                "scope_observation": {"fingerprint": "new-grant", "members": ["same"]}}
        self.assertTrue(frozen.finish(pre, post, {"Read": 2}, [], "answer"))
        for after, tools, changes, answer in [({**post, "publication": "p2"}, {}, [], "answer"),
                                              ({**post, "scope_observation": {"members": ["other"]}}, {}, [], "answer"),
                                              ({**post, "tree_digest": "tree2"}, {}, [], "answer"),
                                              (post, {"Write": 1}, [], "answer"), (post, {"Edit": 1}, [], "answer"),
                                              (post, {}, ["added"], "answer"), (post, {}, [], "   ")]:
            with self.subTest(after=after, tools=tools, changes=changes, answer=answer):
                self.assertFalse(frozen.finish(pre, after, tools, changes, answer))
