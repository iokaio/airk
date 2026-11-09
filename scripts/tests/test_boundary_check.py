# SPDX-License-Identifier: Apache-2.0
"""The S5 boundary gates must fail precisely on new violations and pass reviewed ones."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import textwrap
import unittest

SPEC = importlib.util.spec_from_file_location(
    "boundary_check", Path(__file__).resolve().parents[1] / "dev" / "boundary_check.py"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

REVIEWED = "reviewed in a test"


def write(root, relative, text):
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(text).lstrip(), encoding="utf-8")


def manifest(name, dependencies=""):
    return f'[package]\nname = "{name}"\n\n[dependencies]\n{dependencies}\n[dev-dependencies]\n'


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        write(self.root, "crates/demo/Cargo.toml", manifest("demo", 'base = { path = "../base" }\n'))
        write(self.root, "crates/demo/src/lib.rs", """
            mod alpha;
            mod beta;
            mod gamma;
            #[cfg(windows)]
            #[path = "switch_windows.rs"]
            mod switch;
            #[path = "delta_impl.rs"]
            mod delta;
        """)
        write(self.root, "crates/demo/src/alpha.rs", "use crate::beta::B;\npub struct A;\n")
        write(self.root, "crates/demo/src/beta.rs", "use crate::alpha::A;\npub struct B;\n")
        write(self.root, "crates/demo/src/gamma.rs", "pub struct G;\n")
        write(self.root, "crates/demo/src/switch_windows.rs", "\n")
        write(self.root, "crates/demo/src/delta_impl.rs", "\n")
        write(self.root, "crates/base/Cargo.toml", manifest("base"))
        write(self.root, "crates/base/src/lib.rs", "pub fn base() {}\n")
        self.baseline = self.reviewed(CHECK.build_baseline(CHECK.analyze(self.root), {}))

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def reviewed(baseline):
        for entry in baseline["module_honesty"]:
            if not entry["reason"]:
                entry["reason_class"], entry["reason"] = "kept-mount-child", REVIEWED
        return CHECK.normalize(baseline)

    def run_check(self, exceptions=None):
        return CHECK.evaluate(CHECK.analyze(self.root), exceptions or self.baseline)

    def failures(self, report, check):
        return report["checks"][check]["failures"]


class ModuleHonesty(Fixture):
    def test_allowed_exception_passes_and_platform_switch_needs_none(self):
        report = self.run_check()
        self.assertTrue(report["ok"], report)
        entries = {(entry["kind"], entry["file"], entry["value"]) for entry in self.baseline["module_honesty"]}
        self.assertEqual(entries, {("path", "crates/demo/src/lib.rs", "delta_impl.rs")})
        self.assertEqual(report["checks"]["registration_completeness"]["status"], "not_applicable")

    def test_exception_identity_is_line_independent(self):
        lib = self.root / "crates/demo/src/lib.rs"
        lib.write_text("// moved down\n\n" + lib.read_text(encoding="utf-8"), encoding="utf-8")
        self.assertTrue(self.run_check()["ok"])

    def test_unreviewed_exception_fails(self):
        exceptions = CHECK.normalize(CHECK.build_baseline(CHECK.analyze(self.root), {}))
        failures = self.failures(self.run_check(exceptions), "module_honesty")
        self.assertEqual(len(failures), 1)
        self.assertIn("needs a reviewed reason_class and reason", failures[0])

    def test_new_non_platform_mount_fails(self):
        write(self.root, "crates/demo/src/epsilon_impl.rs", "\n")
        lib = self.root / "crates/demo/src/lib.rs"
        lib.write_text(lib.read_text(encoding="utf-8") + '#[path = "epsilon_impl.rs"]\nmod epsilon;\n', encoding="utf-8")
        report = self.run_check()
        self.assertFalse(report["ok"])
        failures = self.failures(report, "module_honesty")
        self.assertEqual(len(failures), 1)
        self.assertIn('crates/demo/src/lib.rs: #[path = "epsilon_impl.rs"]', failures[0])
        self.assertIn("no reviewed exception", failures[0])

    def test_new_include_of_rust_source_fails(self):
        write(self.root, "crates/demo/src/body.rs", "fn body() {}\n")
        write(self.root, "crates/demo/src/gamma.rs", 'pub struct G;\ninclude!("body.rs");\n')
        failures = self.failures(self.run_check(), "module_honesty")
        self.assertEqual(len(failures), 1)
        self.assertIn('crates/demo/src/gamma.rs: include!("body.rs")', failures[0])

    def test_orphan_fails(self):
        write(self.root, "crates/demo/src/orphan.rs", "fn unused() {}\n")
        failures = self.failures(self.run_check(), "module_honesty")
        self.assertEqual(len(failures), 1)
        self.assertIn("crates/demo/src/orphan.rs: orphan .rs file", failures[0])

    def test_removed_mount_leaves_a_stale_warning_only(self):
        lib = self.root / "crates/demo/src/lib.rs"
        lib.write_text(lib.read_text(encoding="utf-8").replace('#[path = "delta_impl.rs"]\nmod delta;\n', ""), encoding="utf-8")
        (self.root / "crates/demo/src/delta_impl.rs").unlink()
        report = self.run_check()
        self.assertTrue(report["ok"])
        self.assertIn("stale exception", report["checks"]["module_honesty"]["warnings"][0])


class ReasonClasses(unittest.TestCase):
    def test_reason_classes_are_computed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write(root, "crates/demo/Cargo.toml", manifest("demo"))
            write(root, "crates/demo/src/lib.rs", """
                #[cfg(windows)]
                #[path = "host_windows.rs"]
                mod host;
                #[cfg(feature = "full")]
                mod engine;
                #[cfg(not(feature = "full"))]
                #[path = "remote_engine.rs"]
                mod engine;
                #[path = "shared_impl.rs"]
                mod shared;
                mod embed_pool;
            """)
            write(root, "crates/demo/src/host_windows.rs", '#[path = "host_windows_job.rs"]\nmod job;\n')
            write(root, "crates/demo/src/host_windows_job.rs", "\n")
            write(root, "crates/demo/src/engine.rs", "\n")
            write(root, "crates/demo/src/remote_engine.rs", "\n")
            write(root, "crates/demo/src/shared_impl.rs", "\n")
            write(root, "crates/demo/src/embed_pool.rs", 'include!("embed_pool_core.rs");\n')
            write(root, "crates/demo/src/embed_pool_core.rs", "\n")
            write(root, "crates/demo/tests/suite.rs", """
                #[path = "suite/part.rs"]
                mod part;
                #[path = "../src/shared_impl.rs"]
                mod shared;
            """)
            write(root, "crates/demo/tests/suite/part.rs", "\n")
            classes = {
                (item["file"], item["value"]): item["reason_class"]
                for item in CHECK.analyze(root)["honesty"]
            }
            self.assertEqual(classes, {
                ("crates/demo/src/host_windows.rs", "host_windows_job.rs"): "platform-switch-child",
                ("crates/demo/src/lib.rs", "remote_engine.rs"): "feature-switched-alternative",
                ("crates/demo/src/lib.rs", "shared_impl.rs"): "multi-declaration",
                ("crates/demo/src/embed_pool.rs", "embed_pool_core.rs"): "embed-pool-include",
                ("crates/demo/tests/suite.rs", "suite/part.rs"): "integration-test-root-child",
                ("crates/demo/tests/suite.rs", "../src/shared_impl.rs"): "parent-dir-mount",
            })


class CrateEdges(Fixture):
    def test_new_crate_edge_fails(self):
        write(self.root, "crates/base/Cargo.toml", manifest("base", 'demo = { path = "../demo" }\n'))
        failures = self.failures(self.run_check(), "crate_edges")
        self.assertEqual(failures, ["forbidden crate edge base -> demo (not in crate_edges.allowed.base)"])

    def test_target_specific_and_renamed_dependencies_count_but_dev_dependencies_do_not(self):
        text = manifest("base") + 'demo = { path = "../demo" }\n'  # appended under [dev-dependencies]
        write(self.root, "crates/base/Cargo.toml", text)
        self.assertTrue(self.run_check()["ok"])
        text = manifest("base") + "\n[target.'cfg(unix)'.dependencies]\nrenamed = { path = \"../demo\", package = \"demo\" }\n"
        write(self.root, "crates/base/Cargo.toml", text)
        self.assertIn("base -> demo", self.failures(self.run_check(), "crate_edges")[0])

    def test_new_crate_without_reviewed_entry_fails(self):
        write(self.root, "crates/extra/Cargo.toml", manifest("extra", 'base = { path = "../base" }\n'))
        write(self.root, "crates/extra/src/lib.rs", "\n")
        failures = self.failures(self.run_check(), "crate_edges")
        self.assertEqual(failures, ["crate extra has no reviewed allowed-edge entry (depends on ['base'])"])


class ModuleCycles(Fixture):
    def test_baseline_records_the_cycle(self):
        self.assertEqual(self.baseline["module_cycles"]["demo"], [["alpha", "beta"]])

    def test_new_module_joining_a_cycle_fails(self):
        write(self.root, "crates/demo/src/gamma.rs", "use crate::alpha::A;\npub struct G;\n")
        write(self.root, "crates/demo/src/beta.rs", "use crate::{alpha::A, gamma::G};\npub struct B;\n")
        failures = self.failures(self.run_check(), "module_cycles")
        self.assertEqual(len(failures), 1)
        self.assertIn("module(s) ['gamma'] joined a top-level module cycle ['alpha', 'beta', 'gamma']", failures[0])

    def test_new_cycle_fails(self):
        write(self.root, "crates/demo/src/gamma.rs", "use crate::delta::D;\npub struct G;\n")
        write(self.root, "crates/demo/src/delta_impl.rs", "use crate::gamma::G;\npub struct D;\n")
        failures = self.failures(self.run_check(), "module_cycles")
        self.assertEqual(len(failures), 1)
        self.assertIn("['delta', 'gamma'] joined", failures[0])

    def test_module_leaving_a_cycle_passes(self):
        write(self.root, "crates/demo/src/gamma.rs", "use crate::alpha::A;\npub struct G;\n")
        write(self.root, "crates/demo/src/beta.rs", "use crate::{alpha::A, gamma::G};\npub struct B;\n")
        baseline = self.reviewed(CHECK.build_baseline(CHECK.analyze(self.root), {}))
        self.assertEqual(baseline["module_cycles"]["demo"], [["alpha", "beta", "gamma"]])
        write(self.root, "crates/demo/src/gamma.rs", "pub struct G;\n")
        report = self.run_check(baseline)
        self.assertTrue(report["ok"], report)
        self.assertIn("['gamma'] left their baseline cycle", report["checks"]["module_cycles"]["warnings"][0])

    def test_merging_baseline_cycles_fails(self):
        write(self.root, "crates/demo/src/gamma.rs", "use crate::delta::D;\npub struct G;\n")
        write(self.root, "crates/demo/src/delta_impl.rs", "use crate::gamma::G;\npub struct D;\n")
        baseline = self.reviewed(CHECK.build_baseline(CHECK.analyze(self.root), {}))
        self.assertEqual(baseline["module_cycles"]["demo"], [["alpha", "beta"], ["delta", "gamma"]])
        write(self.root, "crates/demo/src/beta.rs", "use crate::{alpha::A, gamma::G};\npub struct B;\n")
        write(self.root, "crates/demo/src/gamma.rs", "use crate::{alpha::A, delta::D};\npub struct G;\n")
        failures = self.failures(self.run_check(baseline), "module_cycles")
        self.assertEqual(len(failures), 1)
        self.assertIn("merged into one cycle", failures[0])


class CommandLine(Fixture):
    def main(self, *arguments):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = CHECK.main(["--root", str(self.root), *arguments])
        return code, output.getvalue()

    def test_write_baseline_then_check_and_json(self):
        exceptions = self.root / "exceptions.json"
        code, _ = self.main("--exceptions", str(exceptions), "--write-baseline")
        self.assertEqual(code, 0)
        data = json.loads(exceptions.read_text(encoding="utf-8"))
        self.assertEqual(data["schema"], CHECK.SCHEMA)
        self.assertEqual(data["crate_edges"]["allowed"], {"base": [], "demo": ["base"]})
        # The lib.rs mount is unclassified, so the seed needs a written reason first.
        code, text = self.main("--exceptions", str(exceptions))
        self.assertEqual(code, 1)
        self.assertIn("needs a reviewed reason_class and reason", text)
        data["module_honesty"][0].update(reason_class="kept-mount-child", reason=REVIEWED)
        exceptions.write_text(json.dumps(data), encoding="utf-8")
        code, text = self.main("--exceptions", str(exceptions), "--json")
        self.assertEqual(code, 0)
        self.assertTrue(json.loads(text)["ok"])
        # A reviewed reason survives regeneration.
        self.main("--exceptions", str(exceptions), "--write-baseline")
        kept = json.loads(exceptions.read_text(encoding="utf-8"))["module_honesty"][0]
        self.assertEqual(kept["reason"], REVIEWED)


if __name__ == "__main__":
    unittest.main()
