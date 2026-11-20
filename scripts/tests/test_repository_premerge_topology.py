# SPDX-License-Identifier: Apache-2.0
"""Focused controls for the frozen later-base topology screen."""

from datetime import datetime
import json
from pathlib import Path
import runpy
import unittest


OWNER = Path(__file__).resolve().parents[1] / "arbiter/analysis/public_source_admission"
AUDIT = runpy.run_path(str(OWNER / "audit_repository_premerge_topology.py"))
ISSUE, BASE, MERGE = (f"2026-08-0{day}T12:00:00+00:00" for day in (1, 2, 3))


def commit(oid, tree, *parents):
    return {"oid": oid, "tree": {"oid": tree},
            "parents": {"nodes": [{"oid": p} for p in parents], "totalCount": len(parents)}}


class TopologyTests(unittest.TestCase):
    def setUp(self):
        self.row = dict(instance_id="case", canonical_repo="org/repo", lineage=[{"id": 1}],
                        base_commit="base", issue_number=2, pull_number=3, base_commit_date=BASE,
                        original_issue_created_at=ISSUE, pull_created_at=BASE, pull_merged_at=MERGE)
        self.data = dict(databaseId=1, visibility="PUBLIC", frozen=dict(commit("base", "a"), committedDate=BASE),
                         issue=dict(number=2, createdAt=ISSUE),
                         pull=dict(number=3, merged=True, createdAt=BASE, mergedAt=MERGE,
                                   mergeCommit=commit("merge", "b", "head"), baseRefOid="base", headRefOid="head",
                                   commits=dict(totalCount=1, nodes=[{"commit": commit("head", "b", "base")}])))
    def result(self):
        return AUDIT["audit"](self.row, self.data, lambda t: datetime.fromisoformat(t) if t else None)

    def test_extended_history_requires_matching_identity_and_observed_edges(self):
        original = self.result()
        original["status"] = "strict_merge_ancestry_unproved"
        rows = [dict(original, instance_id=str(i)) for i in range(12)]
        prior = json.dumps({"rows": rows}).encode()
        page_count = 0

        def fetch(gh, requests, width):
            nonlocal page_count
            page_count += 1
            output = {}
            for key, _, query in requests:
                if page_count == 1:
                    self.assertNotIn("after:", query)
                    nodes, cursor = [commit("merge", "b", "other")], "more"
                else:
                    self.assertIn('after:"more"', query)
                    nodes, cursor = [commit("other", "x", "base")], None
                output[key] = {"databaseId": 2 if key == "0" else 1, "visibility": "PUBLIC", "commit": {
                    "oid": "merge", "history": {"nodes": nodes, "pageInfo": {
                        "hasNextPage": bool(cursor), "endCursor": cursor}}}}
            return output, []

        helper = {"pinned_bytes": lambda *args: prior, "graph_batches": fetch}
        result = AUDIT["extend_history"](helper, Path("."), Path("gh"))
        self.assertEqual(result["counts"], {"strict_premerge_ancestor_with_different_tree": 11,
                                            "strict_merge_ancestry_unproved": 1})
        self.assertEqual(result["rows"][1]["merge_parent_path"], ["merge", "other", "base"])
        self.assertEqual(len(result["rows"][0]["pages"]), 1)
        self.assertEqual(page_count, 2)

    def test_observed_path_and_distinct_tree(self):
        r = self.result()
        self.assertEqual(r["status"], "strict_premerge_ancestor_with_different_tree")
        self.assertEqual(r["merge_parent_path"], ["merge", "head", "base"])
        self.assertEqual(r["pull_head_parent_path"], ["head", "base"])

    def test_declared_pull_base_without_observed_edges_is_insufficient(self):
        self.data["pull"]["commits"] = {"totalCount": 51, "nodes": []}
        self.assertEqual(self.result()["status"], "strict_merge_ancestry_unproved")
        self.assertFalse(self.result()["pull_commit_list_complete"])

    def test_equal_tree_and_equal_commit_are_separate_refusals(self):
        self.data["pull"]["mergeCommit"]["tree"] = {"oid": "a"}
        self.assertEqual(self.result()["status"], "same_tree_as_merge")
        self.data["pull"]["mergeCommit"]["oid"] = "base"
        self.assertEqual(self.result()["status"], "strict_merge_ancestry_unproved")

    def test_require_live_identity_and_frozen_dates(self):
        self.data["databaseId"] = 4
        self.assertEqual(self.result()["status"], "unresolved_identity")
        self.data["databaseId"] = 1
        self.data["issue"]["createdAt"] = BASE
        self.assertEqual(self.result()["status"], "chronology_differs_from_sealed_metadata")

    def test_partial_history_does_not_invalidate_an_observed_direct_edge(self):
        self.data["pull"]["mergeCommit"] = commit("merge", "b", "base")
        self.data["pull"]["commits"] = {"totalCount": 51, "nodes": []}
        self.assertEqual(self.result()["merge_parent_path"], ["merge", "base"])
        self.assertFalse(self.result()["pull_commit_list_complete"])


if __name__ == "__main__":
    unittest.main()
