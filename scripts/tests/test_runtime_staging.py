# SPDX-License-Identifier: Apache-2.0
"""Shared staged-image qualification and the local installer's actual driver."""
from contextlib import ExitStack, redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class LocalStagingTests(unittest.TestCase):
    def run_driver(self, drift=None, *, consume=False, signed=False):
        runtime = load("build-service-runtime")
        local = load("build-local-service-installer")
        base = json.loads((ROOT / "fixtures/split/receipts/build_receipt_2.json").read_text())["base"]
        with tempfile.TemporaryDirectory() as temporary, ExitStack() as stack:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.35"\n')
            (root / "Cargo.lock").write_text("frozen fixture lock")
            (root / "scripts").mkdir()
            (root / "scripts/signing-identity.json").write_bytes((ROOT / "scripts/signing-identity.json").read_bytes())
            for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
                (root / name).write_text("retained legal fixture " + name)
            images = root / "images"
            images.mkdir()
            for name in ("airk.exe", "airkctl.exe"):
                (images / name).write_bytes(b"signed image fixture " + name.encode())
            observations = {"source": dict(base["source"]), "static": dict(base["static_ort"]), "admin": dict(base["admin_assets"])}
            calls = []
            package = load("package-native")
            original_files = None
            inventory_file = root / "checked-admin.json"
            inventory_file.write_text("{}", encoding="utf-8")
            if consume:
                # Real canonical validator over a complete fictional C7 pair.
                fixtures = load("tests/test_package_native")
                package = fixtures.package
                pair, tested_build = fixtures.split_runtime(images, local.TARGET, dict(base["source"]),
                    version="1.8.35", admin=dict(base["admin_assets"]))
                observations["static"] = dict(tested_build["static_ort"])
                original_files = package.inventory(images)
                if drift == "consume-missing":
                    (images / "airkctl.exe").unlink()
                elif drift == "consume-mixed":
                    (images / "airkctl.exe").write_bytes(b"client from another build")
                elif drift == "consume-source":
                    observations["source"]["sha256"] = "f" * 64
                elif drift == "consume-admin":
                    observations["admin"]["bundle_sha256"] = "f" * 64
                elif drift == "consume-static":
                    observations["static"]["build_receipt_sha256"] = "f" * 64
                elif drift == "consume-extra":
                    (images / "unreviewed.dat").write_bytes(b"not an allowed payload")
                    fixtures.seal_split(images, pair, tested_build)
                elif drift == "consume-platform":
                    pair["os"] = "linux"
                    fixtures.seal_split(images, pair, tested_build)
                elif drift == "consume-profile":
                    tested_build["profile"] = "unreviewed"
                    fixtures.seal_split(images, pair, tested_build)
                elif drift == "consume-notice":
                    (images / "notices/static-onnx-runtime/inventory.json").write_text("{}", encoding="utf-8")
                    fixtures.seal_split(images, pair, tested_build)
            class Admin:
                @staticmethod
                def check(output, runner=None):
                    calls.append(("npm",))
                    output.write_text("{}")
                @staticmethod
                def block_from_inventory(inventory):
                    return dict(observations["admin"])
                @staticmethod
                def digests():
                    return {name: observations["admin"][name] for name in ("inventory_sha256", "bundle_sha256")}
            def native_pair(profile):
                if consume:
                    raise AssertionError("consume mode must never invoke Cargo")
                calls.append(("cargo", profile))
                return images / "airk.exe", images / "airkctl.exe"
            def metadata_command(command, **kwargs):
                if command[:2] == ["git", "rev-parse"]:
                    return "a" * 40
                if command[:2] == ["git", "status"]:
                    return ""
                if command[0] == "rustc":
                    return base["rustc"]
                self.assertEqual(command[1:], ["__airk_role", "setup-interim", "--version"]
                                 if Path(command[0]).stem == "airk" else ["--version"])
                if drift == "metadata-image":
                    (Path(command[0]).parent / "airkctl.exe").write_bytes(b"changed after import probe")
                elif drift == "metadata-static":
                    observations["static"]["build_receipt_sha256"] = "f" * 64
                return Path(command[0]).stem + " 1.8.35"
            def seal(staging, *args):
                (staging / "notices").mkdir()
                (staging / "notices/static-ORT-LICENSE").write_text("actual retained static notice")
                (staging / "airk-install.json").write_text("sealed fixture")
                if drift == "seal-static":
                    observations["static"]["build_receipt_sha256"] = "f" * 64
                elif drift == "seal-admin":
                    observations["admin"]["bundle_sha256"] = "f" * 64
                elif drift == "seal-image":
                    (staging / "airk.exe").write_bytes(b"changed during sealing")
            def embedded(*args):
                calls.append(("embedded",))
                if drift == "source":
                    observations["source"]["sha256"] = "f" * 64
                elif drift == "probe-image":
                    (args[0] / "airk.exe").write_bytes(b"changed during model probe")
            def compile_installer(steps, iscc, layout, output, artifact, uninstallers=None, name="iscc", allow_failure=False):
                calls.append(("iscc",))
                if allow_failure:
                    (uninstallers / "fixture.e64").write_bytes(b"unsigned uninstaller fixture")
                    return False
                (output / (artifact + ".exe")).write_bytes(b"compiled installer fixture")
                if drift == "packaging-source":
                    observations["source"]["sha256"] = "f" * 64
                elif drift == "payload":
                    (layout / "service/airk.exe").write_bytes(b"changed after seal")
                elif drift == "packaging-input":
                    (images / "airk.exe").write_bytes(b"changed tested input")
                elif drift == "packaging-copy":
                    (layout.parent / "tested-runtime/airk.exe").write_bytes(b"changed signing input")
                return True
            class Signer:
                def __init__(self, folder, commit):
                    self.receipts = {}
                    self.adapter = type("Adapter", (), {"provision": staticmethod(lambda: calls.append(("signing-client",)))})
                def sign(self, name, source, output):
                    calls.append(("sign", name))
                    output.parent.mkdir(parents=True, exist_ok=True)
                    output.write_bytes(source.read_bytes() + b" fictional Authenticode signature")
                    self.receipts[name] = {"source_sha256": local.digest(source),
                        "binary_sha256": local.digest(output), "identity": "fictional-signer"}
                    return output
            def imports(*args):
                calls.append(("cli-imports",))
                if drift == "static":
                    observations["static"]["build_receipt_sha256"] = "f" * 64
                elif drift == "admin":
                    observations["admin"]["bundle_sha256"] = "f" * 64
                elif drift == "cli-imports":
                    raise RuntimeError("CLI inference dependency refused")
                return base["airkctl_static_imports"]
            def model_smoke(staging, *args):
                calls.append(("model-smoke",))
                metadata = json.loads((staging / "airk-runtime.json").read_text())
                self.assertEqual(metadata["schema"], 3, "smoke observes the actual split receipt")
                self.assertEqual(metadata["airk_sha256"], hashlib.sha256((staging / "airk.exe").read_bytes()).hexdigest())
                return base["embedded_model_smoke"]
            for owner, key, value in [
                (local, "ROOT", root), (local, "OUTPUT", root / "out"),
                (local, "prerequisites", lambda iscc, **kwargs: None),
                (local, "load_script", lambda name: {"build-admin-assets": Admin, "build-service-runtime": runtime,
                    "split_receipts": runtime.split_receipts(), "package-native": package}[name]),
                (local, "compile_installer", compile_installer),
                (runtime, "source_identity", lambda: dict(observations["source"])),
                (runtime, "verify_static_inputs", lambda: dict(observations["static"])),
                (runtime, "cargo_pair", native_pair),
                (runtime, "check_lightweight_dependencies", lambda **kwargs: calls.append(("dependency-proof",))),
                (runtime, "verify_embedded_models", embedded),
                (runtime, "verify_static_imports", lambda *args: base["static_imports"]),
                (runtime, "verify_cli_imports", imports),
                (runtime, "verify_model_smoke", model_smoke),
                (runtime, "stage_static_notices", lambda *args: None),
                (runtime, "interface_versions", lambda: (1, 1)),
                (runtime, "seal_install_payload", seal),
                (runtime, "load_script", lambda name: Admin),
            ]:
                stack.enter_context(mock.patch.object(owner, key, value))
            if signed:
                stack.enter_context(mock.patch.object(local, "Signer", Signer))
            stack.enter_context(mock.patch.object(local.subprocess, "check_output", side_effect=metadata_command))
            mode = ["--runtime-dir", str(images), "--admin-inventory", str(inventory_file)] if consume else ["--force-build"]
            stack.enter_context(mock.patch.object(sys, "argv", ["build-local-service-installer.py", *mode,
                *([] if signed else ["--unsigned"])]))
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                result = local.main()
            receipt_path, = (root / "out").glob("*/receipt.json")
            receipt = json.loads(receipt_path.read_text())
            if result == 0:
                if consume:
                    self.assertEqual(package.inventory(images), original_files, "original tested pair remains unchanged")
                    self.assertEqual(receipt["input_runtime"]["images"],
                        {name: original_files[name] for name in ("airk.exe", "airkctl.exe")})
                    self.assertEqual(receipt["runtime_build"]["profile"], "release")
                layout = receipt_path.parent / "layout"
                self.assertEqual((layout / "notices/dependencies/static-ORT-LICENSE").read_text(), "actual retained static notice")
                for name, key in [("service/airk.exe", "airk_sha256"), ("bin/airkctl.exe", "airkctl_sha256")]:
                    actual = hashlib.sha256((layout / name).read_bytes()).hexdigest()
                    self.assertEqual(actual, receipt["runtime_build"]["runtime"][key])
                    self.assertEqual(actual, receipt["files"][name]["sha256"])
            return result, receipt, calls

    def test_actual_local_driver_qualifies_staged_pair_with_one_cargo_invocation(self):
        result, receipt, calls = self.run_driver()
        self.assertEqual(result, 0, receipt.get("error"))
        self.assertEqual(receipt["runtime_build"]["schema"], 2)
        self.assertEqual(receipt["runtime_build"]["runtime"]["schema"], 3)
        self.assertEqual(calls.count(("cargo", "release-local")), 1)
        self.assertLess(calls.index(("cli-imports",)), calls.index(("iscc",)))
        self.assertLess(calls.index(("model-smoke",)), calls.index(("iscc",)))

    def test_actual_driver_refuses_native_admin_static_import_and_packaging_drift(self):
        for drift in ("source", "admin", "static", "cli-imports", "probe-image", "metadata-image", "metadata-static",
                      "seal-image", "seal-static", "seal-admin", "packaging-source", "payload"):
            with self.subTest(drift=drift):
                result, receipt, calls = self.run_driver(drift)
                self.assertEqual(result, 1)
                self.assertEqual(receipt["result"], "failed")
                self.assertNotIn("installer", receipt)
                self.assertEqual(calls.count(("cargo", "release-local")), 1)
                if drift not in ("packaging-source", "payload"):
                    self.assertNotIn(("iscc",), calls)


    def test_existing_tested_pair_uses_canonical_validator_and_no_cargo_or_npm(self):
        for signed in (False, True):
            with self.subTest(signed=signed):
                result, receipt, calls = self.run_driver(consume=True, signed=signed)
                self.assertEqual(result, 0, receipt.get("error"))
                self.assertFalse(any(row[0] in ("cargo", "npm", "dependency-proof") for row in calls), calls)
                self.assertLess(calls.index(("cli-imports",)), calls.index(("iscc",)))
                self.assertLess(calls.index(("model-smoke",)), calls.index(("iscc",)))
                if signed:
                    self.assertEqual([row[1] for row in calls if row[0] == "sign"],
                        ["airk", "airkctl", "uninstaller", "installer"])
                    self.assertTrue(receipt["signed"])
                    self.assertEqual(receipt["signing"]["provenance"], "local")
                    self.assertNotEqual(receipt["input_runtime"]["images"]["airk.exe"],
                        receipt["files"]["service/airk.exe"]["sha256"])

    def test_existing_pair_refuses_invalid_closure_before_signing_or_compilation(self):
        for drift in ("consume-missing", "consume-mixed", "consume-source", "consume-admin", "consume-static",
                      "consume-extra", "consume-platform", "consume-profile", "consume-notice"):
            with self.subTest(drift=drift):
                result, receipt, calls = self.run_driver(drift, consume=True, signed=True)
                self.assertEqual(result, 1, receipt)
                self.assertEqual(receipt["result"], "failed")
                self.assertNotIn("installer", receipt)
                self.assertFalse(any(row[0] in ("cargo", "npm", "sign", "signing-client", "iscc") for row in calls), calls)

    def test_existing_pair_keeps_post_sign_probe_and_final_drift_checks(self):
        for drift in ("source", "admin", "static", "cli-imports", "probe-image", "metadata-image", "metadata-static",
                      "seal-image", "seal-static", "seal-admin", "packaging-source", "payload", "packaging-input", "packaging-copy"):
            with self.subTest(drift=drift):
                result, receipt, calls = self.run_driver(drift, consume=True, signed=True)
                self.assertEqual(result, 1, receipt)
                self.assertEqual(receipt["result"], "failed")
                self.assertNotIn("installer", receipt)
                self.assertFalse(any(row[0] in ("cargo", "npm", "dependency-proof") for row in calls), calls)
                if not drift.startswith("packaging") and drift != "payload":
                    self.assertNotIn(("iscc",), calls)


class ContainerDriverTests(unittest.TestCase):
    def run_driver(self, drift=False):
        container = load("build-service-container")
        runtime = container.builder
        base = json.loads((ROOT / "fixtures/split/receipts/build_receipt_2.json").read_text())["base"]
        with tempfile.TemporaryDirectory() as temporary, ExitStack() as stack:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.35"\n')
            (root / "Cargo.lock").write_text("frozen fixture")
            inventory = root / "artifacts/admin/admin-assets.json"
            inventory.parent.mkdir(parents=True)
            inventory.write_text("{}")
            images = root / "images"
            images.mkdir()
            for name in ("airk", "airkctl"):
                (images / name).write_bytes(b"Linux image fixture " + name.encode())
            source = dict(base["source"])
            inputs = dict(base["static_ort"], target="x86_64-unknown-linux-gnu")
            calls = []
            class Admin:
                @staticmethod
                def block_from_inventory(document):
                    return base["admin_assets"]
            def embedded(*args):
                if drift:
                    source["sha256"] = "f" * 64
            def pair(profile):
                calls.append(["cargo-pair", profile])
                return images / "airk", images / "airkctl"
            def probe(command, **kwargs):
                calls.append(command)
                if command[0] == "rustc":
                    return "rustc fixture\nhost: x86_64-unknown-linux-gnu"
                if command[-2:] == ["runtime", "self-test"]:
                    return '{"self_test":"passed","version":"1.8.35"}'
                self.assertEqual(command[-1], "--version")
                return Path(command[0]).stem + " 1.8.35"
            def seal(staging, *args):
                (staging / "airk-install.json").write_text("fixed approved inventory fixture")
            def model_smoke(staging, *args):
                metadata = json.loads((staging / "airk-runtime.json").read_text())
                self.assertEqual(metadata["schema"], 3)
                self.assertEqual(metadata["airkctl_sha256"], hashlib.sha256((staging / "airkctl").read_bytes()).hexdigest())
                return base["embedded_model_smoke"]
            for owner, key, value in [
                (container, "ROOT", root),
                (container, "container_source_identity", lambda: dict(source)),
                (container.platform, "system", lambda: "Linux"),
                (container.platform, "machine", lambda: "x86_64"),
                (runtime, "verify_static_inputs", lambda: dict(inputs)),
                (runtime, "cargo_pair", pair),
                (runtime, "cargo_binary", lambda *args: self.fail("split driver must build a matched pair")),
                (runtime, "check_lightweight_dependencies", lambda **kwargs: calls.append(["cli-closure", kwargs])),
                (runtime, "load_script", lambda name: Admin),
                (runtime, "verify_embedded_models", embedded),
                (runtime, "verify_static_imports", lambda *args: base["static_imports"]),
                (runtime, "verify_cli_imports", lambda *args: base["airkctl_static_imports"]),
                (runtime, "verify_model_smoke", model_smoke),
                (runtime, "stage_static_notices", lambda *args: None),
                (runtime, "interface_versions", lambda: (1, 1)),
                (runtime, "seal_install_payload", seal),
            ]:
                stack.enter_context(mock.patch.object(owner, key, value))
            stack.enter_context(mock.patch.object(container.subprocess, "check_output", side_effect=probe))
            output = root / "out"
            stack.enter_context(mock.patch.object(sys, "argv", ["build-service-container.py", "--destination", str(output)]))
            if drift:
                with self.assertRaisesRegex(RuntimeError, "source changed"):
                    container.main()
                self.assertFalse((output / "airk-container").exists())
                return
            with redirect_stdout(io.StringIO()):
                container.main()
            self.assertIn(["cargo-pair", "release"], calls)
            self.assertIn(["cli-closure", {"unified": True}], calls)
            staged = output / "runtime"
            self.assertEqual((staged / "airkctl").read_bytes(), b"Linux image fixture airkctl")
            metadata = json.loads((staged / "airk-runtime.json").read_text())
            self.assertEqual(metadata["schema"], 3)
            self.assertEqual(metadata["airkctl_sha256"], hashlib.sha256((staged / "airkctl").read_bytes()).hexdigest())
            build = json.loads((staged / "airk-build.json").read_text())
            self.assertEqual(build["schema"], 2)
            self.assertEqual(build["runtime"], metadata)
            pin = hashlib.sha256((staged / "airk-install.json").read_bytes()).hexdigest()
            self.assertEqual((output / "airk-container").read_text(), '#!/bin/sh\nexec /usr/lib/airk/airk __airk_role setup-interim runtime container-host --source /usr/lib/airk --approved-sha256 ' + pin + ' "$@"\n')
            self.assertIn([str(staged / "airk"), "__airk_role", "setup-interim", "runtime", "self-test"], calls)

    def test_current_version_driver_stages_pair_and_current_receipts_with_fixed_approval(self):
        self.run_driver()

    def test_current_version_driver_does_not_emit_entrypoint_after_source_drift(self):
        self.run_driver(drift=True)


if __name__ == "__main__":
    unittest.main()
