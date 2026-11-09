# SPDX-License-Identifier: Apache-2.0
"""The report-only structural metrics must follow the real module tree."""
import importlib.util
from pathlib import Path
import tempfile
import textwrap
import unittest

SPEC = importlib.util.spec_from_file_location(
    "simplify_metrics", Path(__file__).resolve().parents[1] / "dev" / "simplify_metrics.py"
)
METRICS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(METRICS)


def write(root, relative, text):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(text).lstrip(), encoding="utf-8")


class ModuleWalk(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        crate = "crates/demo"
        write(self.root, f"{crate}/Cargo.toml", '[package]\nname = "demo"\n')
        write(self.root, f"{crate}/src/lib.rs", """
            mod alpha;
            pub mod beta;
            #[cfg(windows)]
            #[path = "gamma_windows.rs"]
            mod gamma;
            #[path = "delta_impl.rs"]
            mod delta;
            mod inline {
                mod nested;
            }
        """)
        write(self.root, f"{crate}/src/alpha.rs", "mod child;\nuse crate::beta::Thing;\n")
        write(self.root, f"{crate}/src/alpha/child.rs", "include!(\"shared_body.rs\");\n")
        write(self.root, f"{crate}/src/alpha/shared_body.rs", "fn shared() {}\n")
        write(self.root, f"{crate}/src/beta/mod.rs", "pub struct Thing;\nuse crate::{alpha, delta::Other};\n")
        write(self.root, f"{crate}/src/gamma_windows.rs", "\n")
        write(self.root, f"{crate}/src/delta_impl.rs", "pub struct Other;\n")
        write(self.root, f"{crate}/src/inline/nested.rs", "\n")
        write(self.root, f"{crate}/src/orphan.rs", "fn unused() {}\n")
        write(self.root, f"{crate}/tests/integration.rs", '#[path = "../src/orphan_like.rs"]\nmod orphan_like;\n')
        write(self.root, f"{crate}/src/orphan_like.rs", "\n")
        self.crate = METRICS.Crate(self.root / crate, self.root)
        self.crate.walk()

    def tearDown(self):
        self.temporary.cleanup()

    def reached(self):
        return {METRICS.rel(path, self.root) for path in self.crate.reached}

    def test_walk_follows_mod_path_include_and_inline_modules(self):
        reached = self.reached()
        for expected in (
            "crates/demo/src/alpha.rs",
            "crates/demo/src/alpha/child.rs",
            "crates/demo/src/alpha/shared_body.rs",
            "crates/demo/src/beta/mod.rs",
            "crates/demo/src/gamma_windows.rs",
            "crates/demo/src/delta_impl.rs",
            "crates/demo/src/inline/nested.rs",
            "crates/demo/src/orphan_like.rs",
        ):
            self.assertIn(expected, reached)
        self.assertNotIn("crates/demo/src/orphan.rs", reached)

    def test_platform_mounts_are_separated_from_hidden_mounts(self):
        mounts = {(value, platform) for _, _, value, platform in self.crate.path_mounts}
        self.assertIn(("gamma_windows.rs", True), mounts)
        self.assertIn(("delta_impl.rs", False), mounts)
        self.assertEqual([value for _, _, value in self.crate.includes], ["shared_body.rs"])

    def test_report_counts_orphans_and_library_cycle(self):
        report = METRICS.collect(self.root, history=False, harness=False)
        honesty = report["metrics"]["module_honesty"]
        self.assertEqual(honesty["orphans"], ["crates/demo/src/orphan.rs"])
        graph = report["metrics"]["module_graph"]["demo"]
        self.assertEqual(graph["cycles"], [["alpha", "beta"]])
        self.assertEqual(graph["top_level_modules"], 5)


class ProductionEdges(unittest.TestCase):
    def test_test_only_items_and_mounts_add_no_production_edge(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            crate = "crates/demo"
            write(root, f"{crate}/Cargo.toml", '[package]\nname = "demo"\n')
            write(root, f"{crate}/src/lib.rs", """
                mod low;
                mod high;
                #[cfg(test)]
                #[path = "low_tests.rs"]
                mod low_tests;
                #[cfg(any(target_os = "macos", test))]
                mod mixed;
            """)
            write(root, f"{crate}/src/low.rs", """
                pub fn value() -> u8 { 1 }
                #[cfg(test)]
                mod tests {
                    use crate::high::call;
                }
                #[cfg(all(windows, test))]
                use crate::high::call as other;
            """)
            write(root, f"{crate}/src/high.rs", "pub fn call() -> u8 { crate::low::value() }\n")
            write(root, f"{crate}/src/low_tests.rs", "use crate::high::call;\n")
            write(root, f"{crate}/src/mixed.rs", "use crate::high::call;\n")
            report = METRICS.collect(root, history=False, harness=False)
            graph = report["metrics"]["module_graph"]["demo"]
            self.assertEqual(graph["cycles"], [])
            self.assertEqual(graph["top_level_modules"], 3)
            self.assertEqual(graph["edges"], 2)

    def test_cfg_classification(self):
        self.assertTrue(METRICS.is_test_cfg("test"))
        self.assertTrue(METRICS.is_test_cfg("all(windows, test)"))
        self.assertFalse(METRICS.is_test_cfg('any(target_os = "macos", test)'))
        self.assertFalse(METRICS.is_test_cfg("windows"))


class FunctionLengths(unittest.TestCase):
    def test_closing_brace_at_opening_indentation_ends_the_function(self):
        lines = textwrap.dedent("""
            pub fn outer() {
                if true {
                }
            }
            fn declared();
            fn empty() {}
            impl Thing {
                pub(crate) async fn method(
                    value: u8,
                ) -> u8 {
                    value
                }
            }
        """).strip("\n").splitlines()
        self.assertEqual(
            [(name, length) for name, _, length in METRICS.function_lengths(lines)],
            [("outer", 4), ("method", 5)],
        )


class ContextSize(unittest.TestCase):
    def test_estimates_tokens_from_bytes_and_reports_missing_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "CLAUDE.md").write_bytes(b"a" * 8)
            (root / "README.md").write_bytes(b"b" * 9)
            (root / "docs").mkdir()
            (root / "docs" / "architecture.md").write_bytes(b"c\r\n" * 4)
            context = METRICS.context_size(root)
            self.assertEqual(context["files"]["CLAUDE.md"], {"bytes": 8, "estimated_tokens": 2})
            self.assertEqual(context["files"]["README.md"], {"bytes": 9, "estimated_tokens": 3})
            self.assertEqual(context["files"]["docs/architecture.md"], {"bytes": 12, "estimated_tokens": 3})
            self.assertEqual(context["missing"], ["CONTRIBUTING.md"])
            self.assertEqual((context["total_bytes"], context["estimated_tokens"]), (29, 8))
            self.assertEqual(context["target_tokens"], 25000)
            self.assertIn("bytes / 4", context["method"])

    def test_report_includes_the_ungated_context_row(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "README.md").write_bytes(b"x" * 400)
            report = METRICS.collect(root, history=False, harness=False)
            self.assertEqual(report["metrics"]["context_size"]["estimated_tokens"], 100)
            rendered = METRICS.render(report)
            self.assertIn("Context size, estimated tokens", rendered)
            self.assertIn("| 100 (400 bytes); missing CLAUDE.md, CONTRIBUTING.md, docs/architecture.md |", rendered)
            self.assertIn("never pass/fail gates", rendered)


class Cycles(unittest.TestCase):
    def test_strongly_connected_components(self):
        graph = {"a": {"b"}, "b": {"c"}, "c": {"a"}, "d": {"a"}, "e": set()}
        components = [component for component in METRICS.strongly_connected(graph) if len(component) > 1]
        self.assertEqual(components, [["a", "b", "c"]])

    def test_group_heads_take_only_first_segments(self):
        text = "use crate::{alpha, beta::{x, y}, gamma::z};"
        self.assertEqual(METRICS._group_heads(text, text.index("{")), ["alpha", "beta", "gamma"])


if __name__ == "__main__":
    unittest.main()
