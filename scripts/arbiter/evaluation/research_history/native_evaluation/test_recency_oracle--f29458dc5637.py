# SPDX-License-Identifier: Apache-2.0
"""Pure failure-class controls; not native Rust validation."""
import unittest

ROWS = [(101, [b"initial ranked hit " + str(i).encode() for i in range(5)]),
        (102, [("new " + "\u00e9" * 100).encode(), b"new second ranked source"]),
        (103, [])]
EXPECTED = [(102, 0, 192), (102, 1, 24), (101, 0, 20), (101, 1, 20)]

def project(rows):
    shown = []
    for identity, hits in rows:
        for rank, data in enumerate(hits):
            if len(shown) >= 4:
                continue
            prefix = data[:192]
            while True:
                try:
                    prefix.decode("utf-8")
                    break
                except UnicodeDecodeError:
                    prefix = prefix[:-1]
            shown.append((identity, rank, len(prefix)))
    return shown

class FailureClassControls(unittest.TestCase):
    def test_newest_matches_fixed_oracle(self):
        self.assertEqual(project(reversed(ROWS)), EXPECTED)
    def test_oldest_mutation_fails_fixed_oracle(self):
        self.assertNotEqual(project(ROWS), EXPECTED)
    def test_oldest_hides_both_new_read_hits(self):
        self.assertEqual(project(ROWS), [(101, 0, 20), (101, 1, 20), (101, 2, 20), (101, 3, 20)])
    def test_exact_unicode_window(self):
        self.assertEqual(len(ROWS[1][1][0]), 204)
        self.assertEqual(ROWS[1][1][0][:192].decode("utf-8"), "new " + "\u00e9" * 94)

if __name__ == "__main__":
    unittest.main()
