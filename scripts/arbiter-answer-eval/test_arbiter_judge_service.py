# SPDX-License-Identifier: Apache-2.0
"""Plumbing of the loopback judging service, with a stand-in judge and no model."""

from __future__ import annotations

import threading
import unittest
import urllib.error

import arbiter_judge_service as service


class FixedJudge:
    """Scores a pair by the passage's length, so the expected values owe nothing to the service."""

    def __init__(self) -> None:
        self.pairs, self.seconds, self.clipped = 0, 0.0, 0

    def score(self, instruction: str, pairs):
        self.pairs += len(pairs)
        return [float(len(passage)) for _, passage in pairs]


class JudgeServiceTest(unittest.TestCase):
    def setUp(self) -> None:
        self.judge = FixedJudge()
        self.server = service.serve(self.judge, 0, lambda: 123)
        self.address = f"http://127.0.0.1:{self.server.server_address[1]}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=10)

    def test_scores_in_order_and_counts_judgments(self) -> None:
        self.assertEqual(service.score(self.address, "judge", [("need", "abc"), ("need", "a")]), [3.0, 1.0])
        self.assertEqual(service.score(self.address, "judge", []), [])
        got = service.stats(self.address)
        self.assertEqual((got["judgments"], got["peak_gpu_bytes"]), (2, 123))

    def test_refuses_a_malformed_request(self) -> None:
        with self.assertRaises(urllib.error.HTTPError) as raised:
            service.score(self.address, "judge", [("only one",)])  # type: ignore[list-item]
        self.assertEqual(raised.exception.code, 400)
        self.assertEqual(self.judge.pairs, 0)


if __name__ == "__main__":
    unittest.main()
