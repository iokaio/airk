# SPDX-License-Identifier: Apache-2.0
"""Unit tests for the seam census on a small invented crate tree."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

_SPEC = importlib.util.spec_from_file_location(
    "indexing_plan_seam_census", Path(__file__).with_name("seam_census.py"))
census = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(census)

STORE = """
pub struct CurrentRead;
impl CurrentRead {
    pub fn artifact(&self) -> u8 {
        if true { 1 } else { 2 }
    }
    pub(crate) fn inner_only(&self) {}
    fn private(&self) {}
}
impl Other {
    pub fn ignored(&self) {}
}
impl crate::FileStore {
    pub fn load_retained(&self) {}
}
"""
KERNEL = """
fn use_it(read: &CurrentRead, store: &FileStore) {
    read.artifact();
    read .artifact ();
    store.load_retained();
    FileStore::load_retained(store);
}
fn artifact() {}
"""
KERNEL_TESTS = "fn t(read: &CurrentRead) { read.artifact(); read.inner_only(); }"


def tree(root):
    for relative, text in (("airk-store/src/lib.rs", STORE),
                           ("airk-store/src/tests.rs", "impl CurrentRead { pub fn in_tests(&self) {} }"),
                           ("airk-kernel/src/lib.rs", KERNEL),
                           ("airk-kernel/tests/it.rs", KERNEL_TESTS)):
        path = Path(root) / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


class PublicFunctions(unittest.TestCase):
    def test_only_public_items_of_the_named_owners_are_found(self):
        found = list(census.public_functions(STORE))
        self.assertEqual(found, [("CurrentRead", "artifact", "public"),
                                 ("CurrentRead", "inner_only", "crate"),
                                 ("FileStore", "load_retained", "public")])

    def test_test_paths_are_recognized(self):
        self.assertTrue(census.is_test_path(Path("airk-kernel/tests/it.rs")))
        self.assertTrue(census.is_test_path(Path("airk-store/src/current/tests.rs")))
        self.assertTrue(census.is_test_path(Path("airk-store/src/quota/namespaces_tests.rs")))
        self.assertFalse(census.is_test_path(Path("airk-store/src/current/read.rs")))


class Census(unittest.TestCase):
    def test_calls_are_counted_by_crate_and_apart_from_tests(self):
        with tempfile.TemporaryDirectory() as root:
            tree(root)
            rows = {row["function"]: row for row in census.census(Path(root))}
        self.assertEqual(sorted(rows), ["artifact", "inner_only", "load_retained"])
        self.assertEqual(rows["artifact"]["calls_outside_store"], {"airk-kernel": 2})
        self.assertEqual(rows["artifact"]["calls_in_tests"], 1)
        self.assertTrue(rows["artifact"]["ambiguous_name"], "the kernel defines the name too")
        self.assertEqual(rows["load_retained"]["calls_outside_store"], {"airk-kernel": 2})
        self.assertFalse(rows["load_retained"]["ambiguous_name"])
        self.assertEqual(rows["inner_only"]["calls_outside_store"], {})
        self.assertEqual(rows["inner_only"]["calls_in_tests"], 1)

    def test_markdown_lists_each_function_once(self):
        with tempfile.TemporaryDirectory() as root:
            tree(root)
            table = census.markdown(census.census(Path(root)))
        self.assertEqual(table.count("| `artifact` |"), 1)
        self.assertIn("2 (airk-kernel 2)", table)


if __name__ == "__main__":
    unittest.main()
