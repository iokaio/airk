# SPDX-License-Identifier: Apache-2.0
"""Exercise the finalist through actual deterministic extraction without disk or model fixtures."""
import hashlib
from pathlib import Path
from time import monotonic
import unittest

import live_gather
import range_choice_live
import topical_index


class BundledLiveTests(unittest.TestCase):
    def repository(self):
        repo = live_gather.WorkspaceRepository(Path("."))
        files = {}
        for n in range(250):
            path = f"src/item{n:03}.py"
            text = "SignalTag = 5" if n < 45 else "plain = 1"
            repo.texts[path] = [text]
            repo.digests[path] = hashlib.sha256(text.encode()).digest()
            files[path] = topical_index.file_record(path, text, len(text))
        repo.index = topical_index.assemble(repo.source, files, monotonic())
        return repo

    def test_real_pool_and_file_score_reversal(self):
        args = {"task": "Inspect changes", "needs": ["rule", "test"], "anchors": ["SignalTag"]}
        seen = []
        def judge(instruction, pairs):
            seen.append(instruction)
            self.assertEqual(len(pairs), 40)
            for n,(q,doc) in enumerate(pairs):
                self.assertEqual(q, "Inspect changes\nrule\ntest\nSignalTag")
                self.assertEqual(doc, f"src/item{n:03}.py\n\nLines 1-1 (excerpt):\nSignalTag = 5")
            return [n/40 for n in range(40)]
        passages,count,audit = range_choice_live.propose(self.repository(),args,judge)
        self.assertEqual(len(seen), 1)
        self.assertEqual(count, 16)
        self.assertEqual([p.at for p in passages], [f"src/item{n:03}.py:1-1" for n in range(39,33,-1)])
        self.assertTrue(all(not p.supports for p in passages))
        self.assertEqual(audit["attribution"], "bundled_file")

    def test_empty_candidates_skip_scorer(self):
        p,count,audit = range_choice_live.propose(self.repository(), {"task": "UnmatchedString"}, self.fail)
        self.assertEqual((p,count,audit["pool"]), ([],0,[]))
