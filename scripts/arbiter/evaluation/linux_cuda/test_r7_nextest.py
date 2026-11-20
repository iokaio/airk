# SPDX-License-Identifier: Apache-2.0
"""Check signed GNU Nextest configuration relocation without running Cargo."""

from pathlib import Path
import tomllib
import unittest

from r7_nextest import config_for_store, file_pin


class R7NextestConfig(unittest.TestCase):
    def setUp(self):
        self.original = (
            b'# signed profile bytes\r\n'
            b'[profile.ci]\r\n'
            b'retries = 0\r\n'
            b'[profile.ci.junit]\r\n'
            b'path = "junit.xml"\r\n'
            b'[[profile.ci.overrides]]\r\n'
            b'filter = "package(airk-retrieval)"\r\n'
            b'threads-required = 2\r\n'
        )
        self.store = Path.cwd() / "private attempt" / "nextest-store"

    def test_only_store_dir_changes(self):
        effective = config_for_store(self.original, self.store)
        self.assertTrue(effective.startswith(self.original))
        parsed = tomllib.loads(effective.decode())
        self.assertEqual(parsed.pop("store"), {"dir": str(self.store)})
        self.assertEqual(parsed, tomllib.loads(self.original.decode()))
        self.assertNotEqual(file_pin(self.store, effective)["sha256"], file_pin(self.store, self.original)["sha256"])

    def test_existing_store_is_refused(self):
        with self.assertRaisesRegex(ValueError, "already specifies a store"):
            config_for_store(self.original + b'\r\n[store]\r\ndir = "target/nextest"\r\n', self.store)

    def test_relative_or_traversing_store_is_refused(self):
        for path in (Path("target/nextest"), self.store / ".." / "outside"):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "private absolute"):
                config_for_store(self.original, path)

    def test_changed_report_path_is_refused(self):
        with self.assertRaisesRegex(ValueError, "CI JUnit path differs"):
            config_for_store(self.original.replace(b'"junit.xml"', b'"elsewhere.xml"'), self.store)


if __name__ == "__main__":
    unittest.main()
