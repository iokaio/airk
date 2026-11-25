# SPDX-License-Identifier: Apache-2.0
"""Checks for binding exact vector ranks to the measured request."""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
import vector_rank_replay as replay


class VectorRankReplay(unittest.TestCase):
    def test_trace_binds_query_pipeline_and_publication(self):
        query = "where are the committed rows?"
        event = {
            "query_sha256": common.sha256_bytes(query.encode()),
            "publication": {"incarnation": "fixture", "sequence": 3, "digest": "publication"},
            "pipeline": "pipeline",
            "ranks": [[[250, "src/a.rs", 1]], []],
        }
        raw = (replay.MARKER + json.dumps(event)).encode()
        token = "airk-r1:fixture:3:publication"
        self.assertEqual(replay.read_event(raw, query, token, "pipeline"), event["ranks"])
        for invalid in ((query + "other", token, "pipeline"), (query, token, "different"),
                        (query, token.replace(":3:", ":4:"), "pipeline")):
            with self.assertRaisesRegex(common.HarnessError, "different query, pipeline or publication"):
                replay.read_event(raw, *invalid)
        with self.assertRaisesRegex(common.HarnessError, "expected one"):
            replay.read_event(raw + b"\n" + raw, query, token, "pipeline")


if __name__ == "__main__":
    unittest.main()
