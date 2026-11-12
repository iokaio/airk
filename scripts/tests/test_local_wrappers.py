# SPDX-License-Identifier: Apache-2.0
"""Contract cases for the local force-build wrappers (split plan §10.5.1, §10.5.5; U21, U37).

Commands are mocked: these cases check command construction, ordering, fail-fast propagation,
fresh outputs and that a build never installs or publishes. Installing the produced EXE and
VSIX on Windows (§12) is the separate acceptance evidence.
"""
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


vsix = load("local_vsix", ROOT / "scripts/build-local-vsix.py")
service = load("local_service", ROOT / "scripts/build-local-service-installer.py")
fixtures = load("universal_fixtures", Path(__file__).with_name("test_package_extension.py"))
# Commands a build-only wrapper must never construct (install, publish, trust or service control).
FORBIDDEN = re.compile(r"--install-extension|vsce\b.*\bpublish|publish\.cjs|\bgh\b|release (?:create|upload)"
                       r"|Start-Process|sc(?:\.exe)? (?:start|create)|Import-Certificate|certutil|__airk_role|airk(?:\.exe)? setup|install\.sh",
                       re.IGNORECASE)
FALSE_SOURCE = {"commit": "a" * 40, "tree": "c" * 40, "dirty": False, "dirty_paths": []}


class RecordingSteps:
    """Stands in for build-local-service-installer.Steps; records and optionally fails one step."""

    def __init__(self, folder, fail=None, on_package=None):
        self.folder, self.fail, self.on_package = folder, fail, on_package
        self.records, self.commands = [], []

    def run(self, name, command, cwd=ROOT):
        command = [str(part) for part in command]
        self.commands.append((name, command))
        self.records.append({"step": name, "command": command, "exit_code": 7 if name == self.fail else 0})
        if name == self.fail:
            raise RuntimeError(f"{name} failed (7)")
        if name == "package" and self.on_package:
            self.on_package(Path(command[command.index("--output") + 1]))

    def call(self, name, action):
        self.commands.append((name, None))
        if name == self.fail:
            raise RuntimeError(f"{name} failed")
        return action()


class LocalVsixWrapper(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name) / "local-vsix"
        # The checked-out version may predate the split; the gate itself has its own case.
        self.version = fixtures.SPLIT
        patcher = patch.multiple(vsix, OUTPUT=self.output, source_state=lambda: dict(FALSE_SOURCE),
                                 extension_version=lambda: self.version)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.steps = []
        self.counter = 0

    def clock(self, _format):
        self.counter += 1
        return f"run-{self.counter}"

    def package(self, output):
        fixtures.universal_candidate(output / "publish", version=self.version, commit=FALSE_SOURCE["commit"],
                                     tree=FALSE_SOURCE["tree"])

    def factory(self, fail=None, on_package=True):
        def build(folder):
            steps = RecordingSteps(folder, fail, self.package if on_package else None)
            self.steps.append(steps)
            return steps
        return build

    def main(self, argv, **kwargs):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = vsix.main(argv, which=kwargs.pop("which", lambda tool: "/fictional/" + tool),
                             clock=self.clock, **kwargs)
        return code, out.getvalue(), err.getvalue()

    def test_force_build_is_required(self):
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            vsix.main([], which=lambda tool: "/fictional/" + tool)
        self.assertFalse(self.output.exists())

    def test_missing_prerequisites_fail_before_any_step_or_output(self):
        factory = self.factory()
        with self.assertRaisesRegex(SystemExit, "Missing prerequisites: npm"):
            self.main(["--force-build"], which=lambda tool: None if tool == "npm" else "/fictional/" + tool,
                      steps_factory=factory)
        self.assertEqual(self.steps, [])
        self.assertFalse(self.output.exists())

    def test_pre_split_version_stops_before_building(self):
        self.version = "1.8.23"
        with self.assertRaisesRegex(SystemExit, "predates the split"):
            self.main(["--force-build"], steps_factory=self.factory())
        self.assertEqual(self.steps, [])

    def test_sequence_outputs_and_receipt_describe_this_build(self):
        code, out, _ = self.main(["--force-build", "--channel", "stable"], steps_factory=self.factory())
        self.assertEqual(code, 0)
        steps, = self.steps
        self.assertEqual([name for name, _ in steps.commands], ["npm-ci", "contracts-i2", "compile", "package"])
        npm_ci = steps.commands[0][1]
        self.assertEqual(npm_ci[1:], ["ci", "--ignore-scripts", "--no-audit", "--no-fund"])
        self.assertTrue(steps.commands[1][1][-3].endswith("contract_types.py"))
        self.assertEqual(steps.commands[1][1][-2:], ["i2", "--check"])
        package = steps.commands[3][1]
        self.assertTrue(package[1].endswith("package-extension.py"))
        self.assertNotIn("--target", package)
        self.assertNotIn("--native-dir", package)
        run = self.output / "run-1"
        self.assertEqual(Path(package[package.index("--output") + 1]), run / "vsix")
        record = json.loads((run / "receipt.json").read_text(encoding="utf-8"))
        produced = run / "vsix/publish" / f"airk-{self.version}.vsix"
        self.assertEqual(record["result"], "built")
        self.assertEqual(record["target"], "universal")
        self.assertEqual((record["commit"], record["tree"], record["version"]),
                         (FALSE_SOURCE["commit"], FALSE_SOURCE["tree"], self.version))
        self.assertEqual(Path(record["vsix"]), produced)
        self.assertEqual(record["vsix_sha256"], fixtures.package.digest(produced.read_bytes()))
        self.assertIn(str(produced), out)
        self.assertIn(str(run / "receipt.json"), out)

    def test_force_build_never_reuses_an_earlier_vsix(self):
        self.assertEqual(self.main(["--force-build"], steps_factory=self.factory())[0], 0)
        first = json.loads((self.output / "run-1/receipt.json").read_text())
        code, out, _ = self.main(["--force-build"], steps_factory=self.factory())
        self.assertEqual(code, 0)
        second = json.loads((self.output / "run-2/receipt.json").read_text())
        self.assertNotEqual(first["vsix"], second["vsix"])
        self.assertTrue(second["vsix"].startswith(str(self.output / "run-2")))
        self.assertEqual([name for name, _ in self.steps[1].commands], ["npm-ci", "contracts-i2", "compile", "package"])
        self.assertNotIn(first["vsix"], out)
        # A clashing run folder is an error, never a reuse.
        with self.assertRaises(FileExistsError):
            vsix.main(["--force-build"], which=lambda tool: "/fictional/" + tool, clock=lambda _: "run-2",
                      steps_factory=self.factory())

    def test_failed_step_stops_packaging_and_announces_no_artifact(self):
        for failing in ("npm-ci", "contracts-i2", "compile", "package"):
            with self.subTest(step=failing):
                code, out, err = self.main(["--force-build"], steps_factory=self.factory(fail=failing))
                self.assertEqual(code, 1)
                steps = self.steps[-1]
                names = [name for name, _ in steps.commands]
                self.assertEqual(names[-1], failing)
                self.assertNotIn("package", names[:-1] if failing == "package" else names)
                run = self.output / f"run-{self.counter}"
                record = json.loads((run / "receipt.json").read_text())
                self.assertEqual(record["result"], "failed")
                self.assertIn(failing, record["error"])
                self.assertNotIn("vsix", record)
                self.assertEqual(out, "")
                self.assertIn("FAILED", err)

    def test_receipt_identity_must_match_this_checkout(self):
        def other_commit(output):
            fixtures.universal_candidate(output / "publish", version=self.version, commit="b" * 40, tree="d" * 40)
        def factory(folder):
            steps = RecordingSteps(folder, on_package=other_commit)
            self.steps.append(steps)
            return steps
        code, out, err = self.main(["--force-build"], steps_factory=factory)
        self.assertEqual(code, 1)
        self.assertIn("identity differs", err)

    def test_build_only_commands_never_install_or_publish(self):
        self.main(["--force-build", "--channel", "preview"], steps_factory=self.factory())
        for name, command in self.steps[0].commands:
            self.assertIsNone(FORBIDDEN.search(" ".join(command)), name)


class LocalServiceInstallerWrapper(unittest.TestCase):
    def test_missing_prerequisites_fail_before_any_step(self):
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(service.sys, "platform", "win32"), \
                patch.object(service.shutil, "which", lambda tool: None if tool in ("cargo", "git") else "/fictional/" + tool):
            with self.assertRaisesRegex(SystemExit, r"Missing prerequisites: cargo, git, Inno Setup ISCC"):
                service.prerequisites(Path(temporary) / "missing-ISCC.exe")

    def test_failed_admin_check_stops_compilation_and_packaging(self):
        calls = []

        class Admin:
            @staticmethod
            def check(output, runner=None):
                calls.append("admin")
                raise RuntimeError("npm run check failed (1)")

        class Runtime:
            @staticmethod
            def verify_static_inputs():
                calls.append("static-ort")

            @staticmethod
            def cargo_binary(*args, **kwargs):
                calls.append("cargo")

        class Signer:
            pass

        scripts = {"sign-native": Signer, "build-admin-assets": Admin, "build-service-runtime": Runtime,
                   "split_receipts": fixtures.package.receipts}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(service, "OUTPUT", Path(temporary)), \
                patch.object(service, "prerequisites", lambda iscc: None), \
                patch.object(service, "load_script", lambda name: scripts[name]), \
                patch.object(service.tomllib, "loads", lambda text: {"workspace": {"package": {"version": fixtures.SPLIT}}}), \
                patch.object(service, "compile_installer", lambda *args, **kwargs: calls.append("iscc")), \
                patch("sys.argv", ["build-local-service-installer.py", "--force-build", "--unsigned"]), \
                redirect_stdout(io.StringIO()) as out, redirect_stderr(io.StringIO()) as err:
            self.assertEqual(service.main(), 1)
            receipt, = Path(temporary).glob("*/receipt.json")
            record = json.loads(receipt.read_text())
        self.assertEqual(calls, ["admin"])
        self.assertEqual(record["result"], "failed")
        self.assertNotIn("installer", record)
        self.assertEqual(out.getvalue(), "")
        self.assertIn("FAILED", err.getvalue())

    def test_signed_build_without_signing_access_stops_before_staging(self):
        calls = []

        class Admin:
            @staticmethod
            def check(output, runner=None):
                calls.append("admin")

        class Runtime:
            @staticmethod
            def verify_static_inputs():
                calls.append("static-ort")

            @staticmethod
            def cargo_binary(name, *args, **kwargs):
                calls.append("cargo-" + name)
                return Path("/fictional") / f"{name}.exe"

            @staticmethod
            def check_lightweight_dependencies():
                calls.append("airkctl-proof")

        class LocalSigning:
            @staticmethod
            def provision():
                calls.append("provision")
                raise RuntimeError("Azure CLI sign-in required")

        scripts = {"build-admin-assets": Admin, "build-service-runtime": Runtime,
                   "local_signing": LocalSigning, "split_receipts": fixtures.package.receipts}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(service, "OUTPUT", Path(temporary)), \
                patch.object(service, "prerequisites", lambda iscc: None), \
                patch.object(service, "load_script", lambda name: scripts[name]), \
                patch.object(service, "digest", lambda path: "0" * 64), \
                patch.object(service, "stage", lambda *args: calls.append("stage")), \
                patch.object(service.tomllib, "loads", lambda text: {"workspace": {"package": {"version": fixtures.SPLIT}}}), \
                patch.object(service, "compile_installer", lambda *args, **kwargs: calls.append("iscc")), \
                patch("sys.argv", ["build-local-service-installer.py", "--force-build"]), \
                redirect_stdout(io.StringIO()) as out, redirect_stderr(io.StringIO()) as err:
            self.assertEqual(service.main(), 1)
            receipt, = Path(temporary).glob("*/receipt.json")
            record = json.loads(receipt.read_text())
        self.assertEqual(calls[-1], "provision")
        self.assertNotIn("stage", calls)
        self.assertNotIn("iscc", calls)
        self.assertEqual(record["result"], "failed")
        self.assertIn("Azure CLI sign-in required", record["error"])
        self.assertNotIn("installer", record)
        self.assertEqual(out.getvalue(), "")
        self.assertIn("FAILED", err.getvalue())

    def test_pre_split_versions_stop(self):
        class Signer:
            pass
        with patch.object(service, "prerequisites", lambda iscc: None), \
                patch.object(service, "load_script", lambda name: Signer if name == "sign-native" else fixtures.package.receipts), \
                patch.object(service.tomllib, "loads", lambda text: {"workspace": {"package": {"version": "1.8.23"}}}), \
                patch("sys.argv", ["build-local-service-installer.py", "--force-build", "--unsigned"]):
            with self.assertRaisesRegex(SystemExit, "predates the split"):
                service.main()


class PowerShellWrappers(unittest.TestCase):
    WRAPPERS = ("build-local-installers.ps1", "build-local-service-installer.ps1", "build-local-vsix.ps1")

    def test_wrappers_require_force_build_delegate_and_never_install_or_publish(self):
        for name in self.WRAPPERS:
            text = (ROOT / "scripts" / name).read_text(encoding="utf-8")
            with self.subTest(wrapper=name):
                self.assertIn("[switch] $ForceBuild", text)
                self.assertIn("if (-not $ForceBuild) { throw", text)
                self.assertIn("$ErrorActionPreference = 'Stop'", text)
                self.assertIsNone(FORBIDDEN.search(text))
        combined = (ROOT / "scripts/build-local-installers.ps1").read_text(encoding="utf-8")
        service_call = combined.index('& "$PSScriptRoot/build-local-service-installer.ps1" @service')
        vsix_call = combined.index('& "$PSScriptRoot/build-local-vsix.ps1" -ForceBuild')
        self.assertLess(service_call, vsix_call)
        self.assertIn("ForceBuild = $true", combined)
        for owner in ("build-local-vsix.py", "build-local-service-installer.py"):
            source = (ROOT / "scripts" / owner).read_text(encoding="utf-8")
            self.assertIsNone(FORBIDDEN.search(source.split('"""', 2)[2]), owner)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell 7 unavailable")
    def test_combined_wrapper_stops_at_the_first_failed_product(self):
        for service_exit, vsix_exit, expected, ran_vsix in ((0, 0, 0, True), (3, 0, 3, False), (0, 5, 5, True)):
            with self.subTest(service=service_exit, vsix=vsix_exit), tempfile.TemporaryDirectory() as temporary:
                scripts = Path(temporary)
                shutil.copyfile(ROOT / "scripts/build-local-installers.ps1", scripts / "build-local-installers.ps1")
                log = scripts / "calls.log"
                for name, code in (("build-local-service-installer.ps1", service_exit), ("build-local-vsix.ps1", vsix_exit)):
                    (scripts / name).write_text(
                        "param([switch]$ForceBuild,[switch]$Unsigned,[string]$Iscc,[string]$Channel)\n"
                        f"Add-Content -LiteralPath '{log}' -Value ('{name} ' + [bool]$ForceBuild + ' ' + [bool]$Unsigned)\n"
                        f"exit {code}\n", encoding="utf-8")
                result = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
                                         str(scripts / "build-local-installers.ps1"), "-ForceBuild", "-Unsigned"],
                                        capture_output=True, text=True, timeout=120)
                self.assertEqual(result.returncode, expected, result.stderr)
                calls = log.read_text(encoding="utf-8").splitlines()
                self.assertEqual(calls[0], "build-local-service-installer.ps1 True True")
                self.assertEqual(len(calls), 2 if ran_vsix else 1)
                if ran_vsix:
                    self.assertEqual(calls[1], "build-local-vsix.ps1 True False")
        with tempfile.TemporaryDirectory() as temporary:
            refused = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-File",
                                      str(ROOT / "scripts/build-local-installers.ps1")],
                                     capture_output=True, text=True, timeout=120, cwd=temporary)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("ForceBuild", refused.stdout + refused.stderr)


if __name__ == "__main__":
    unittest.main()
