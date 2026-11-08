# SPDX-License-Identifier: Apache-2.0
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SPEC = importlib.util.spec_from_file_location("airk_bootstrap", Path(__file__).parents[1] / "bootstrap.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BootstrapTests(unittest.TestCase):
    @staticmethod
    def ready_probe(command):
        if "toolchain" in command:
            return "1.98.0-x86_64-unknown-linux-gnu (default)"
        if "component" in command:
            return "clippy-x86_64-unknown-linux-gnu\nrustfmt-x86_64-unknown-linux-gnu"
        if "nextest" in command:
            return "cargo-nextest 0.9.148"
        return "v24.15.0" if "node" in command[0] else "10.0.0"

    def test_dependency_tools_use_canonical_pins_and_only_run_after_base_prerequisites(self):
        calls = []
        policy = SimpleNamespace(PINNED_TOOLS={"cargo-audit": "fixture-audit", "cargo-deny": "fixture-deny"},
                                 require_pinned_tool=calls.append, ScannerError=RuntimeError)
        with patch.object(MODULE, "dependency_policy_owner", return_value=policy) as owner, \
                patch.object(MODULE, "npm_command", return_value=["npm"]):
            commands = MODULE.prerequisites(MODULE.pins(), locate=lambda name: name, probe=self.ready_probe)
            self.assertEqual(commands["cargo"], ["rustup", "run", "1.98.0", "cargo"])
            self.assertEqual(calls, list(policy.PINNED_TOOLS))
            owner.assert_called_once_with()
        with patch.object(MODULE, "dependency_policy_owner") as owner:
            with self.assertRaisesRegex(ValueError, "Install Git"):
                MODULE.prerequisites(MODULE.pins(), locate=lambda _: None, probe=self.ready_probe)
            owner.assert_not_called()

    def test_missing_dependency_pin_is_actionable_and_never_installs_it(self):
        def missing(name):
            raise RuntimeError("wrong pinned version")
        policy = SimpleNamespace(PINNED_TOOLS={"cargo-deny": "fixture-pin"},
                                 require_pinned_tool=missing, ScannerError=RuntimeError)
        with patch.object(MODULE, "dependency_policy_owner", return_value=policy), \
                patch.object(MODULE, "npm_command", return_value=["npm"]):
            with self.assertRaisesRegex(ValueError, "cargo-deny fixture-pin explicitly under artifacts/tools/cargo-deny"):
                MODULE.prerequisites(MODULE.pins(), locate=lambda name: name, probe=self.ready_probe)

    def test_pins_follow_current_native_and_ci_sources(self):
        selected = MODULE.pins()
        self.assertEqual(selected["rust"], "1.98.0")
        self.assertEqual(selected["node"], "24.15.0")
        self.assertEqual(selected["nextest"], "0.9.148")

    def test_missing_tools_are_actionable_without_any_install_command(self):
        with self.assertRaisesRegex(ValueError, "Install Git") as failure:
            MODULE.prerequisites(MODULE.pins(), locate=lambda _: None, probe=lambda _: self.fail("unexpected probe"))
        self.assertIn("rustup", str(failure.exception))
        self.assertIn("PowerShell 7", str(failure.exception))

    def test_missing_pinned_toolchain_does_not_trigger_rustup_install(self):
        calls = []
        def probe(command):
            calls.append(command)
            if command[-1] == "--version":
                return "v24.15.0" if "node" in command[0] else "10.0.0"
            return "stable-x86_64-unknown-linux-gnu (default)"
        with patch.object(MODULE, "npm_command", return_value=["npm"]):
            with self.assertRaisesRegex(ValueError, "Prepare Rust explicitly"):
                MODULE.prerequisites(MODULE.pins(), locate=lambda name: name, probe=probe)
        self.assertFalse(any("install" in command for command in calls))
        self.assertFalse(any("cargo" in command for command in calls))

    def test_offline_and_build_only_modes_preserve_argv_and_skip_tests(self):
        commands = {"cargo": ["rustup", "run", "1.98.0", "cargo"], "npm": ["node", "/space here/npm-cli.js"],
                    "pwsh": ["pwsh"], "git": ["git"]}
        args = argparse.Namespace(download=False, no_tests=True, fast=False)
        rows = dict(MODULE.steps(args, commands, Path("/owned space/run")))
        self.assertEqual(set(rows), {"dependency-policy", "model-inputs", "runtime-inputs", "npm-inputs", "runtime-build", "extension-build"})
        self.assertEqual(next(iter(rows)), "dependency-policy")
        self.assertIn("--offline", rows["dependency-policy"])
        self.assertEqual(rows["dependency-policy"][-1], str(Path("/owned space/run/dependency-policy.json")))
        self.assertIn("--offline", rows["npm-inputs"])
        self.assertFalse(any("--download" in command for command in rows.values()))
        self.assertIn("scripts/provision-static-onnx-runtime.py", rows["runtime-inputs"])
        self.assertIn("--verify", rows["runtime-inputs"])
        self.assertIn("--environment-file", rows["runtime-inputs"])
        self.assertNotIn("--build", rows["runtime-inputs"])
        self.assertNotIn("--download-tools", rows["runtime-inputs"])
        self.assertEqual(rows["runtime-build"][-3], str(Path("/owned space/run/runtime")))
        args.download = True
        args.no_tests = False
        args.fast = True
        rows = dict(MODULE.steps(args, commands, Path("/owned/run")))
        self.assertEqual(next(iter(rows)), "dependency-policy")
        self.assertIn("--require-fresh", rows["dependency-policy"])
        self.assertNotIn("--offline", rows["dependency-policy"])
        self.assertIn("--download", rows["model-inputs"])
        for flag in ["--download", "--download-tools", "--build"]:
            self.assertIn(flag, rows["runtime-inputs"])
        self.assertNotIn("--verify", rows["runtime-inputs"])
        self.assertEqual(rows["runtime-inputs"][-2:], ["--parallel", "2"])
        self.assertNotIn("--offline", rows["npm-inputs"])
        self.assertNotIn("clippy", rows)
        self.assertIn("--all-features", rows["native-tests"])
        self.assertEqual(rows["native-tests"][-2:], ["--profile", "ci"])

    def test_static_runtime_cache_is_persistent_and_existing_explicit_location_is_retained(self):
        static = MODULE.static_environment_owner()
        owner = SimpleNamespace(host_target=lambda: "fixture-native",plain=lambda path:Path(path))
        with patch.object(MODULE, "static_environment_owner", return_value=SimpleNamespace(provisioner_owner=lambda: owner,destination_from_location=static.destination_from_location)), patch.dict(MODULE.os.environ):
            MODULE.os.environ.pop("ORT_LIB_LOCATION", None)
            first = MODULE.static_inputs(Path("/run/first"), False)
            second = MODULE.static_inputs(Path("/run/second"), False)
            location = first[first.index("--destination") + 1]
            self.assertEqual(location, second[second.index("--destination") + 1])
            self.assertEqual(location, str(MODULE.ROOT / "target/static-onnx-runtime/fixture-native"))
            with tempfile.TemporaryDirectory() as temporary:
                for tree in ("b","b2"):
                    supplied = Path(temporary) / "reviewed original cache" / tree
                    MODULE.os.environ["ORT_LIB_LOCATION"] = str(supplied)
                    command = MODULE.static_inputs(Path("/run/third"), False)
                    self.assertEqual(command[command.index("--destination") + 1], str(supplied.parent))
            MODULE.os.environ["ORT_LIB_LOCATION"] = "relative/b"
            with self.assertRaisesRegex(ValueError, "absolute"):
                MODULE.static_inputs(Path("/run/fourth"), False)

    def test_verified_static_handoff_reaches_build_child_only_and_refusal_stops_build(self):
        for refused in (False, True):
            with self.subTest(refused=refused), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(MODULE, "ROOT", Path(temporary)), \
                    patch.object(MODULE, "pins", return_value={"rust": "1.98.0"}), \
                    patch.object(MODULE, "prerequisites", return_value={"rustup": ["rustup"], "git": ["git"]}), \
                    patch.object(MODULE.subprocess, "check_output", return_value="fixture"), \
                    patch.object(MODULE.platform, "platform", return_value="fixture platform"), \
                    patch.object(MODULE, "steps", return_value=[("dependency-policy", ["policy"]), ("runtime-inputs", ["static-owner"]), ("runtime-build", ["builder"])]):
                verified = {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1", "ORT_PREFER_DYNAMIC_LINK": "0", "ORT_LIB_LOCATION": str(Path(temporary) / "cache/b"), "ORT_LIB_PROFILE": "Release"}
                complete = {"environment": verified, "receipt": {"future_owner_field": [1, 2, 3]}}
                before = dict(MODULE.os.environ)
                seen = []
                def execute(name, command, directory, environment):
                    seen.append(name)
                    if name == "dependency-policy":
                        (directory / "dependency-policy.json").write_text(json.dumps({"schema": "airk.dependency-policy-check/1", "all_requested_checks_passed": True}))
                    if name == "runtime-inputs":
                        (directory / "static-runtime-environment.json").write_text(json.dumps(complete))
                    if name == "runtime-build":
                        self.assertEqual({key: environment[key] for key in verified}, verified)
                    return {"name": name, "exit_code": 0, "seconds": 0, "counts": None}
                def load(path):
                    self.assertEqual(json.loads(path.read_text()), complete)
                    if refused:
                        raise ValueError("actual static owner refused")
                    return verified
                with patch.object(MODULE, "execute", side_effect=execute), \
                        patch.object(MODULE, "static_environment_owner", return_value=SimpleNamespace(load_environment_file=load)):
                    self.assertEqual(MODULE.main(["--no-tests"]), 2 if refused else 0)
                self.assertEqual(dict(MODULE.os.environ), before)
                self.assertEqual(seen, ["dependency-policy", "runtime-inputs"] + ([] if refused else ["runtime-build"]))
                report = json.loads(next(Path(temporary).glob("artifacts/bootstrap/*/report.json")).read_text())
                self.assertEqual(report["host"]["os"], "fixture platform")
                self.assertEqual(report["complete"], not refused)
                self.assertEqual(report["static_runtime"]["status"], "refused" if refused else "verified")

    def test_npm_windows_entry_uses_node_without_a_shell(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "space here"
            entry = root / "node_modules/npm/bin/npm-cli.js"
            entry.parent.mkdir(parents=True)
            entry.write_text("fictional npm", encoding="utf-8")
            command = MODULE.npm_command(str(root / "node.exe"), str(root / "npm.cmd"))
            self.assertEqual(command, [str(root / "node.exe"), str(entry)])

    def test_counts_are_observed_and_failed_unknown_counts_stay_unknown(self):
        self.assertEqual(MODULE.counts("native-tests", "Summary [23s] 1939 tests run: 1939 passed, 37 skipped", 0),
                         {"passed": 1939, "skipped": 37, "failed": 0, "flaky": 0})
        self.assertEqual(MODULE.counts("extension-tests", "ℹ pass 547\nℹ fail 0\nℹ skipped 19", 0),
                         {"passed": 547, "skipped": 19, "failed": 0})
        self.assertEqual(MODULE.counts("python-tests", "Ran 309 tests in 24s\nOK (skipped=6)", 0),
                         {"passed": 303, "skipped": 6, "failed": 0})
        self.assertIsNone(MODULE.counts("native-tests", "compilation failed", 1))
        self.assertIsNone(MODULE.counts("python-tests", "Ran 9 tests in 1s\nFAILED", 1))

    def test_failed_native_run_keeps_failed_flaky_and_skipped_summary_counts(self):
        output = ("Summary [1s] 2 tests run: 2 passed, 7 skipped\n"
                  "Summary [23s] 1942 tests run: 1941 passed (2 flaky), 1 failed, 37 skipped\n")
        self.assertEqual(MODULE.counts("native-tests", output, 100),
                         {"passed": 1941, "failed": 1, "skipped": 37, "flaky": 2})
        self.assertEqual(MODULE.counts("native-tests", "Summary [1s] 1 test run: 1 failed, 9 skipped", 100),
                         {"passed": 0, "failed": 1, "skipped": 9, "flaky": 0})
        self.assertEqual(MODULE.counts("native-tests", "Summary [1s] 2 tests run: 2 passed, 9 skipped", 100)["failed"], None)

    def test_check_only_does_not_provision_create_logs_or_run_suites(self):
        with patch.object(MODULE, "prerequisites", return_value={}), patch.object(MODULE, "execute") as execute, \
                patch.object(MODULE.Path, "mkdir") as mkdir:
            self.assertEqual(MODULE.main(["--check-only"]), 0)
            execute.assert_not_called()
            mkdir.assert_not_called()

    def test_failed_child_output_is_retained_with_its_actual_exit_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = MODULE.execute("fixture", [MODULE.sys.executable, "-c", "print('fixture refusal'); raise SystemExit(7)"],
                                    Path(temporary), dict(MODULE.os.environ))
            self.assertEqual(result["exit_code"], 7)
            self.assertEqual(Path(result["log"]).read_text().strip(), "fixture refusal")
            self.assertIsNone(result["counts"])

    def test_policy_step_blocks_builds_and_retains_complete_failure_or_limited_receipt(self):
        for mode, code in (("limited", 0), ("failed", 1), ("unavailable", 2)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(MODULE, "ROOT", Path(temporary)), \
                    patch.object(MODULE, "pins", return_value={"rust": "1.98.0"}), \
                    patch.object(MODULE, "prerequisites", return_value={"rustup": ["rustup"], "git": ["git"]}), \
                    patch.object(MODULE.subprocess, "check_output", return_value="fixture\n"), \
                    patch.object(MODULE.platform, "platform", return_value="fixture platform"), \
                    patch.object(MODULE, "steps", return_value=[("dependency-policy", ["policy"]), ("runtime-build", ["build"])]):
                policy = {"schema": "airk.dependency-policy-check/1", "status": mode,
                          "all_requested_checks_passed": code == 0, "fresh_full_advisory_check": False,
                          "checks": {"npm_audit": {"status": "not_run", "reason": "offline"},
                                     "cargo_vet": {"status": "passed" if code == 0 else mode}},
                          "network_mode": "strict offline; npm not checked", "full_extra_field": ["retain exactly"]}
                seen = []
                def execute(name, command, directory, environment):
                    seen.append(name)
                    if name == "dependency-policy":
                        (directory / "dependency-policy.json").write_text(json.dumps(policy))
                    return {"name": name, "exit_code": code if name == "dependency-policy" else 0,
                            "seconds": 0.1, "counts": None}
                with patch.object(MODULE, "execute", side_effect=execute):
                    self.assertEqual(MODULE.main(["--no-tests"]), code)
                self.assertEqual(seen, ["dependency-policy", "runtime-build"] if code == 0 else ["dependency-policy"])
                report = json.loads(next(Path(temporary).glob("artifacts/bootstrap/*/report.json")).read_text())
                self.assertEqual(report["dependency_policy"], policy)
                self.assertEqual(report["complete"], code == 0)
                self.assertFalse(report["dependency_policy"]["fresh_full_advisory_check"])

    def test_missing_or_inconsistent_policy_report_never_starts_provisioning(self):
        for policy in (None, {"schema": "airk.dependency-policy-check/1", "all_requested_checks_passed": False}):
            with self.subTest(policy=policy), tempfile.TemporaryDirectory() as temporary, \
                    patch.object(MODULE, "ROOT", Path(temporary)), \
                    patch.object(MODULE, "pins", return_value={"rust": "1.98.0"}), \
                    patch.object(MODULE, "prerequisites", return_value={"rustup": ["rustup"], "git": ["git"]}), \
                    patch.object(MODULE.subprocess, "check_output", return_value="fixture\n"), \
                    patch.object(MODULE.platform, "platform", return_value="fixture platform"), \
                    patch.object(MODULE, "steps", return_value=[("dependency-policy", ["policy"]), ("model-inputs", ["download"])]):
                def execute(name, command, directory, environment):
                    if policy is not None:
                        (directory / "dependency-policy.json").write_text(json.dumps(policy))
                    return {"name": name, "exit_code": 0, "seconds": 0.1, "counts": None}
                with patch.object(MODULE, "execute", side_effect=execute) as invoke:
                    self.assertEqual(MODULE.main(["--download"]), 2)
                    self.assertEqual(invoke.call_count, 1)

    def test_failing_step_keeps_a_receipt_and_does_not_start_later_work(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(MODULE, "ROOT", Path(temporary)), \
                patch.object(MODULE, "pins", return_value={"rust": "1.98.0"}), \
                patch.object(MODULE, "prerequisites", return_value={"rustup": ["rustup"], "git": ["git"]}), \
                patch.object(MODULE.subprocess, "check_output", return_value="fixture\n"), \
                patch.object(MODULE.platform, "platform", return_value="fixture platform"), \
                patch.object(MODULE, "steps", return_value=[("one", ["one"]), ("two", ["two"])]), \
                patch.object(MODULE, "execute", return_value={"name": "one", "exit_code": 7, "seconds": 0.1, "counts": None}) as execute:
            self.assertEqual(MODULE.main([]), 7)
            self.assertEqual(execute.call_count, 1)
            reports = list(Path(temporary).glob("artifacts/bootstrap/*/report.json"))
            self.assertEqual(len(reports), 1)
            self.assertIn('"complete": false', reports[0].read_text())
            self.assertIn('"os": "fixture platform"', reports[0].read_text())


if __name__ == "__main__":
    unittest.main()
