# SPDX-License-Identifier: Apache-2.0
import unittest

from adaptive_passage_budget import Passage, allocation


def segment(path, line, text, owner=None):
    at = f"{path}:{line}-{line + len(text.splitlines()) - 1}"
    return Passage(at, path, line, line + len(text.splitlines()) - 1, text, owner, at if owner else None)


class PassageAllocationTests(unittest.TestCase):
    def test_prioritizes_unrepresented_need_at_same_budget(self):
        stream = [segment("a", 1, "aaa\n", 1), segment("a", 2, "bbb\n"),
                  segment("b", 1, "ccc\n", 2)]
        self.assertEqual([p["at"] for p in allocation(stream, 8)["passages"]], ["a:1-1", "a:2-2"])
        for adaptive in (False, True):
            result = allocation(stream, 8, True, adaptive)
            self.assertEqual([p["needs"] for p in result["passages"]], [[1], [2]])
            self.assertEqual(result["omission_counts"], {"byte_limit": 1})

    def test_lexical_and_positional_novelty_and_exact_threshold(self):
        stream = [segment("a", 1, "one\ntwo\n", 1), segment("a", 2, "two\nthree\n"),
                  segment("b", 1, " one  \n two\n", 2), segment("b", 3, "one\ntwo\nfour\n", 2),
                  segment("a", 5, "one\ntwo\nfive\n")]
        result = allocation(stream, 20000, True, True)
        self.assertEqual([p["at"] for p in result["passages"]], ["a:1-2", "b:3-5", "a:2-3"])
        self.assertEqual(result["omission_counts"], {"low_novelty": 2})
        self.assertEqual(len(stream), len(result["passages"]) + len(result["omissions"]))

    def test_file_ceiling_unicode_and_skip_oversized(self):
        stream = [segment("big", 1, "éééé\n", 1)] + [segment(str(i), 1, f"{i}\n", i) for i in range(1, 8)]
        small = allocation(stream, 8, True, True)
        self.assertEqual(small["passage_bytes"], 8)
        self.assertEqual(len(small["passages"]), 4)
        large = allocation(stream, 20000)
        self.assertEqual(len(large["passages"]), 6)
        self.assertEqual(large["omission_counts"], {"file_limit": 2})