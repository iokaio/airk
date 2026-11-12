# SPDX-License-Identifier: Apache-2.0
"""Runtime staging preserves source identity and runnable Mac development bundles."""
import hashlib
import importlib.util
import os
import plistlib
import shutil
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


class ServiceRuntimeBuildTests(unittest.TestCase):
    def test_static_identity_binds_verified_link_closure_and_optional_supplement(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("closure_identity_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "b").mkdir()
            (root / "build-receipt.json").write_text("original immutable producer fixture")
            environment = {"ORT_LIB_LOCATION":str(root / "b"),"ORT_LIB_PROFILE":"Release"}
            verified = {"receipt":{"original":True},"environment":environment,
                        "static_link_closure":{"schema":1,"archives":[{"path":"b/Release/_deps/re2-build/re2.lib","sha256":"a"*64}]}}
            provisioner = mock.Mock()
            provisioner.verify_build.return_value = verified
            with mock.patch.dict(builder.os.environ,environment), mock.patch.object(builder,"load_script",return_value=provisioner), mock.patch.object(builder.subprocess,"check_output",return_value="host: x86_64-pc-windows-msvc\n"):
                original = builder.verify_static_inputs()
                self.assertIsNone(original["static_link_supplement_sha256"])
                verified["static_link_closure"]["archives"][0]["sha256"] = "b"*64
                changed = builder.verify_static_inputs()
                self.assertEqual(original["build_receipt_sha256"],changed["build_receipt_sha256"])
                self.assertNotEqual(original["static_link_closure_sha256"],changed["static_link_closure_sha256"])
                self.assertNotEqual(original["verified_inputs_sha256"],changed["verified_inputs_sha256"])
                supplement = root / "link-supplement-receipt.json"
                supplement.write_text("real additional archive fixture receipt")
                verified["static_link_supplement"] = {"path":supplement.name,"sha256":builder.digest(supplement),"receipt":{"fresh_full_build":False}}
                supplemented = builder.verify_static_inputs()
                self.assertEqual(supplemented["static_link_supplement_sha256"],builder.digest(supplement))
                self.assertLess(len(builder.json.dumps(supplemented)),1024)
                supplement.write_text("changed receipt")
                with self.assertRaisesRegex(RuntimeError,"supplemental.*identity differs"):
                    builder.verify_static_inputs()
                verified.pop("static_link_supplement")
                recovery = root / "link-supplement-recovery.json"
                recovery.write_text("exact observed archive; command exit remains unknown")
                verified["static_link_recovery"] = {"path":recovery.name,"sha256":builder.digest(recovery),"receipt":{"command_exit_unknown":True,"command_pass":False}}
                recovered = builder.verify_static_inputs()
                self.assertEqual(recovered["static_link_recovery_sha256"],builder.digest(recovery))
                self.assertNotEqual(recovered["verified_inputs_sha256"],changed["verified_inputs_sha256"])
                self.assertFalse(verified["static_link_recovery"]["receipt"]["command_pass"])
                recovery.write_text("changed recovery input")
                with self.assertRaisesRegex(RuntimeError,"recovery receipt identity differs"):
                    builder.verify_static_inputs()
                verified.pop("static_link_recovery")
                verified["static_link_closure"]["archives"][0]["path"] = "x"*65536
                with self.assertRaisesRegex(RuntimeError,"identity exceeds"):
                    builder.verify_static_inputs()

    def test_model_smoke_requires_the_embedded_pinned_engine_and_complete_identity(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("model_probe_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        report = {"schema": 1, "source": "embedded", "engine": "onnxruntime-cpu/1.22.1", "dimensions": 384,
                  "repeat_equal": True, "profile": "minilm-l6-v2-u8-v1", "legacy_identity": "a" * 64, "pipeline_digest": "b" * 64}
        for field, value in ((None, None), ("schema", True), ("source", "directory"), ("engine", "onnxruntime-cpu/1.22.0"),
                             ("dimensions", 383), ("repeat_equal", 1), ("profile", "fp32"), ("pipeline_digest", "incomplete")):
            candidate = {**report, **({field: value} if field else {})}
            with self.subTest(field=field), mock.patch.object(builder.subprocess, "check_output", return_value=builder.json.dumps(candidate)) as probe:
                if field:
                    with self.assertRaisesRegex(RuntimeError, "smoke contract differs"):
                        builder.verify_model_smoke(Path("owned"), "linux", "1.8.14")
                else:
                    self.assertEqual(builder.verify_model_smoke(Path("owned"), "linux", "1.8.14"), report)
                self.assertEqual(probe.call_args.args[0], [str(Path("owned/airk")), "--verify-model-smoke"])

    def test_static_import_inspection_fails_for_external_ort_and_requires_native_tool_success(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("static_import_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for os_name, imports in (("windows", "onnxruntime.dll"), ("macos", "@rpath/libonnxruntime.1.22.1.dylib"), ("linux", "NEEDED libonnxruntime.so.1")):
            with self.subTest(os_name=os_name), mock.patch.object(builder, "windows_import_tool", return_value="dumpbin"), mock.patch.object(builder.subprocess, "run", return_value=mock.Mock(stdout=imports)):
                with self.assertRaisesRegex(RuntimeError, "external ONNX Runtime"):
                    builder.verify_static_imports(Path("owned image"), os_name)
        with mock.patch.object(builder.subprocess, "run", side_effect=FileNotFoundError("inspection tool unavailable")):
            with self.assertRaises(FileNotFoundError):
                builder.verify_static_imports(Path("owned image"), "linux")
        with mock.patch.object(builder.subprocess, "run", return_value=mock.Mock(stdout="NEEDED libc.so.6")) as inspect:
            report = builder.verify_static_imports(Path("owned image"), "linux")
            self.assertFalse(report["external_onnxruntime_imports"])
            self.assertEqual(inspect.call_args.args[0], ["readelf", "-d", "owned image"])
            self.assertTrue(inspect.call_args.kwargs["check"])

    def test_modern_mac_bundle_contains_only_the_main_and_never_copies_it(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("single_image_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for version in ("1.8.14", "1.8.23"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "airk").write_bytes(b"single compiled main")
                builder.arrange_macos_bundle(root, version)
                main = "AIRK.app/Contents/MacOS/airk"
                self.assertEqual(builder.runtime_executables("macos", version), (main,))
                self.assertEqual(builder.signing_names("macos", version), (main,))
                self.assertEqual(builder.launcher_name("macos", version), main)
                self.assertEqual(builder.inference_assets("macos", "aarch64", version), {})
                self.assertEqual(list((root / "AIRK.app/Contents/MacOS").iterdir()), [root / main])
                with mock.patch.object(builder.subprocess, "run") as signing:
                    builder.sign_development_macos_bundle(root)
                    self.assertEqual(signing.call_count, 2)
                    self.assertEqual(signing.call_args_list[0].args[0][-1], str(root / "AIRK.app"))
                    self.assertIn("--strict", signing.call_args_list[1].args[0])

    def test_modern_runtime_metadata_is_strict_and_model_probe_uses_main(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("single_metadata_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk").write_bytes(b"single image")
            with mock.patch.object(builder.subprocess, "check_output", return_value="airk 1.8.14\n"):
                metadata = builder.write_pair_metadata(root, "1.8.14", "linux", "x86_64")
            self.assertEqual(metadata["schema"], 2)
            self.assertNotIn("embed_sha256", metadata)
            files = {"airk": builder.digest(root / "airk")}
            self.assertTrue(builder.runtime_metadata_matches(metadata, "1.8.14", "linux", "x86_64", files))
            self.assertFalse(builder.runtime_metadata_matches({**metadata, "embed_sha256": "0" * 64}, "1.8.14", "linux", "x86_64", files))
            with mock.patch.object(builder.subprocess, "check_output", return_value="{}") as probe, mock.patch.object(builder, "load_script"):
                builder.verify_embedded_models(root, "linux", "1.8.14")
                self.assertEqual(probe.call_args.args[0], [str(root / "airk"), "--verify-bundled-models"])

    def test_historical_mac_bundle_has_a_separate_launcher_and_legacy_layout_stays_exact(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("standalone_image_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for version, expected in [("1.8.7", 3), ("1.8.8", 4), ("1.8.13", 4)]:
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "airk").write_bytes(b"compiled main before the app seal")
                (root / "airk-embed").write_bytes(b"compiled helper")
                builder.arrange_macos_bundle(root, version)
                names = builder.signing_names("macos", version)
                self.assertEqual(len(names), expected)
                launcher = root / builder.launcher_name("macos", version)
                self.assertEqual(launcher.read_bytes(), b"compiled main before the app seal")
                if version != "1.8.7":
                    self.assertEqual(launcher.name, "airk-launcher-image")
                    self.assertNotEqual(launcher, root / builder.runtime_names("macos")[0])
                    with mock.patch.object(builder.subprocess, "run") as signing:
                        builder.sign_development_macos_bundle(root)
                        self.assertEqual(signing.call_count, 5)
                        self.assertEqual(signing.call_args_list[2].args[0][-1], str(launcher))
                        self.assertEqual(signing.call_args_list[3].args[0][-1], str(root / "AIRK.app"))
                else:
                    self.assertFalse((root / "AIRK.app/Contents/MacOS/airk-launcher-image").exists())

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang"), "macOS standalone launcher/sealed-copy fixture")
    def test_standalone_launcher_remains_valid_after_sealing_and_copying(self):
        self._check_standalone_copy("-", None)

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang")
                         and os.environ.get("AIRK_TEST_MAC_SIGNING_IDENTITY")
                         and os.environ.get("AIRK_TEST_MAC_TEAM_ID"),
                         "explicit macOS Developer ID identity/team required; no notarization upload")
    def test_developer_id_standalone_launcher_survives_sealed_copy(self):
        identity = os.environ["AIRK_TEST_MAC_SIGNING_IDENTITY"]
        team = os.environ["AIRK_TEST_MAC_TEAM_ID"]
        self.assertTrue(identity.startswith("Developer ID Application:"))
        self.assertRegex(team, r"\A[A-Z0-9]{10}\Z")
        self._check_standalone_copy(identity, team)

    def _check_standalone_copy(self, identity, team):
        def checked(arguments, timeout=30):
            result = subprocess.run(arguments, capture_output=True, text=True, timeout=timeout)
            self.assertEqual(result.returncode, 0,
                             f"{arguments!r}\nstdout: {result.stdout[-4096:]}\nstderr: {result.stderr[-4096:]}")
            return result
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("copied_launcher_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk standalone image ") as temporary:
            root = Path(temporary).resolve(strict=True)
            source = root / "fixture.c"
            source.write_text('#include <stdio.h>\nint main(void) { puts("airk 1.8.8"); return 0; }\n', encoding="utf-8")
            bundled = root / builder.runtime_names("macos")[0]
            bundled.parent.mkdir(parents=True)
            (bundled.parent.parent / "Info.plist").write_bytes(builder.macos_bundle_files("1.8.8")["AIRK.app/Contents/Info.plist"])
            checked(["clang", str(source), "-o", str(bundled)])
            image = root / builder.launcher_name("macos", "1.8.8")
            shutil.copyfile(bundled, image)
            timestamp = "--timestamp" if team else "--timestamp=none"
            options = ["--force", "--sign", identity, "--options", "runtime", timestamp]
            checked(["/usr/bin/codesign", *options, str(image)], timeout=60)
            checked(["/usr/bin/codesign", *options, str(root / "AIRK.app")], timeout=60)
            checked(["/usr/bin/codesign", "--verify", "--strict", str(root / "AIRK.app")])
            # The app main remains bundle-bound; never weaken its strict check.
            copied_main = root / "invalid-main-copy"
            shutil.copyfile(bundled, copied_main)
            rejected = subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(copied_main)], capture_output=True, text=True, timeout=30)
            self.assertNotEqual(rejected.returncode, 0, "fixture failed to reproduce the sealed-main copy boundary")
            copied = root / "airk-launcher"
            shutil.copyfile(image, copied)
            copied.chmod(0o700)
            self.assertEqual(copied.read_bytes(), image.read_bytes())
            checked(["/usr/bin/codesign", "--verify", "--strict", str(copied)])
            result = checked([str(copied), "--version"], timeout=10)
            self.assertEqual(result.stdout.strip(), "airk 1.8.8")
            if team:
                specification = importlib.util.spec_from_file_location("signed_copy_fixture", script.parent / "sign-native.py")
                signer = importlib.util.module_from_spec(specification)
                specification.loader.exec_module(signer)
                component = {"platform": "Darwin", "binary_sha256": signer.digest(image), "team_id": team}
                observed = signer.verify_copied_launcher(root, "1.8.8", component)
                self.assertTrue(observed["strict_signature"])
                self.assertTrue(observed["standalone_version_probe"])

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("clang"), "macOS native signing/load fixture")
    def test_development_mac_helper_loads_its_adhoc_signed_library(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("native_load_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk-mac-library-load-") as temporary:
            root = Path(temporary).resolve(strict=True)
            (root / "library.c").write_text("int airk_fixture_value(void) { return 42; }\n", encoding="utf-8")
            (root / "loader.c").write_text('''#include <dlfcn.h>
#include <stdio.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!library) { fprintf(stderr, "%s\\n", dlerror()); return 3; }
    int (*value)(void) = dlsym(library, "airk_fixture_value");
    int result = value && value() == 42 ? 0 : 4;
    dlclose(library);
    return result;
}
''', encoding="utf-8")
            for name in ("airk", "airk-embed"):
                subprocess.run(["clang", "-mmacosx-version-min=13.3", str(root / "loader.c"), "-o", str(root / name)], check=True, capture_output=True, timeout=30)
            builder.arrange_macos_bundle(root, "1.6.4")
            library = root / "AIRK.app/Contents/MacOS/libonnxruntime.dylib"
            subprocess.run(["clang", "-mmacosx-version-min=13.3", "-dynamiclib", str(root / "library.c"), "-o", str(library)], check=True, capture_output=True, timeout=30)
            builder.sign_development_macos_bundle(root)
            helper = root / builder.runtime_names("macos")[1]
            result = subprocess.run([str(helper), str(library)], capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_mac_bundle_keeps_exactly_two_executables_and_static_channel_scope(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk").write_bytes(b"lightweight main fixture")
            (root / "airk-embed").write_bytes(b"inference helper fixture")
            builder.arrange_macos_bundle(root, "1.4.0")
            main, helper = builder.runtime_names("macos")
            self.assertEqual((root / main).read_bytes(), b"lightweight main fixture")
            self.assertEqual((root / helper).read_bytes(), b"inference helper fixture")
            self.assertFalse((root / "airk").exists())
            self.assertFalse((root / "airk-embed").exists())
            info = plistlib.loads((root / "AIRK.app/Contents/Info.plist").read_bytes())
            self.assertEqual(info["CFBundleExecutable"], "airk")
            for channel in ("stable", "preview"):
                name = f"io.iokaio.airk.service.{channel}"
                agent = plistlib.loads((root / f"AIRK.app/Contents/Library/LaunchAgents/{name}.plist").read_bytes())
                self.assertEqual(agent["Label"], name)
                self.assertEqual(agent["BundleProgram"], "Contents/MacOS/airk")
                self.assertEqual(agent["ProgramArguments"], ["airk", "runtime", "login-foreground", "--channel", channel])
                self.assertNotIn("Program", agent)
            with mock.patch.object(builder.subprocess, "run") as codesign:
                builder.sign_development_macos_bundle(root)
                self.assertEqual(codesign.call_count, 4)
                self.assertEqual(codesign.call_args_list[2].args[0][-1], str(root / "AIRK.app"))
                self.assertIn("--timestamp=none", codesign.call_args_list[0].args[0])
                for call in codesign.call_args_list[:3]:
                    command = call.args[0]
                    self.assertEqual(command[command.index("--sign") + 1], "-")
                    self.assertEqual(command[command.index("--options") + 1], "0")
    def test_mac_sealed_login_entry_changes_only_for_direct_service_versions(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for version, entry in (("1.8.13", "login-foreground"), ("1.8.14", "login-serve"), ("1.9.0", "login-serve")):
            resources = builder.macos_bundle_files(version)
            for channel in ("stable", "preview"):
                name = f"AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.{channel}.plist"
                definition = plistlib.loads(resources[name])
                self.assertEqual(definition, {
                    "Label": f"io.iokaio.airk.service.{channel}",
                    "BundleProgram": "Contents/MacOS/airk",
                    "ProgramArguments": ["airk", "runtime", entry, "--channel", channel],
                    "RunAtLoad": True, "KeepAlive": {"SuccessfulExit": False},
                    "ThrottleInterval": 30, "ProcessType": "Background",
                })

    def test_changed_source_discards_owned_staging_and_preserves_inputs(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk-build-original-fixture-") as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="9.8.7"\n', encoding="utf-8")
            suffix = ".exe" if sys.platform == "win32" else ""
            main = root / f"airk{suffix}"
            helper = root / f"airk-embed{suffix}"
            main.write_bytes(b"Original main build fixture.")
            helper.write_bytes(b"Original inference build fixture.")
            cli = root / f"airkctl{suffix}"
            cli.write_bytes(b"Original CLI build fixture.")
            inventory = root / "admin-assets.json"
            inventory.write_text("{}", encoding="utf-8")
            destination = root / "runtime"

            def artifact(name, _profile, _features, package="airk-cli"):
                return {"airk-embed": helper, "airkctl": cli}.get(name, main)

            def identity(arguments, **_kwargs):
                self.assertEqual(arguments[1], "--version")
                return f"{Path(arguments[0]).stem} 9.8.7\n"

            with (mock.patch.object(builder, "ROOT", root),
                  mock.patch.object(builder, "load_script"),
                  mock.patch.object(builder, "stage_inference_runtime"),
                  mock.patch.object(builder, "verify_embedded_models"),
                  mock.patch.object(builder, "check_lightweight_dependencies"),
                  mock.patch.object(builder, "verify_static_inputs"),
                  mock.patch.object(builder, "cargo_binary", side_effect=artifact),
                  mock.patch.object(builder.platform, "system", return_value="Windows" if suffix else "Linux"),
                  mock.patch.object(builder.platform, "machine", return_value="x86_64"),
                  mock.patch.object(builder, "source_identity", side_effect=[
                      {"sha256": "a" * 64, "files": 1}, {"sha256": "b" * 64, "files": 1}]),
                  mock.patch.object(builder.subprocess, "check_output", side_effect=identity),
                  mock.patch.object(sys, "argv", [str(script), "--destination", str(destination),
                                                  "--admin-inventory", str(inventory)])):
                with self.assertRaisesRegex(RuntimeError, "source changed"):
                    builder.main()
            self.assertFalse(destination.exists())
            self.assertFalse(any(path.name.startswith(".airk-runtime-stage-") for path in root.iterdir()))
            self.assertEqual(cli.read_bytes(), b"Original CLI build fixture.")
            self.assertEqual(main.read_bytes(), b"Original main build fixture.")
            self.assertEqual(helper.read_bytes(), b"Original inference build fixture.")

    def test_pair_versions_are_refused_without_building_a_helper(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("pair_refusal_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        with tempfile.TemporaryDirectory(prefix="airk-pair-refusal-") as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.13"\n', encoding="utf-8")
            destination = root / "runtime"
            with (mock.patch.object(builder, "ROOT", root),
                  mock.patch.object(builder, "cargo_binary", side_effect=AssertionError("no Cargo build")),
                  mock.patch.object(builder, "source_identity", side_effect=AssertionError("no source capture")),
                  mock.patch.dict(builder.os.environ, {"MACOSX_DEPLOYMENT_TARGET": "13.3"}),
                  mock.patch.object(sys, "argv", [str(script), "--destination", str(destination)])):
                with self.assertRaisesRegex(SystemExit, "historical runtime pair"):
                    builder.main()
            self.assertFalse(destination.exists())


def load_builder(name):
    script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
    specification = importlib.util.spec_from_file_location(name, script)
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    return builder


class SplitPairBuildTests(unittest.TestCase):
    """Split versions (>= 1.8.24) build and inventory airk and airkctl; earlier versions are unchanged."""

    def test_split_roster_selects_images_by_name(self):
        builder = load_builder("split_roster_builder")
        expected = {"windows": ("airk.exe", "airkctl.exe"), "linux": ("airk", "airkctl"),
                    "macos": ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl")}
        for os_name, images in expected.items():
            with self.subTest(os_name=os_name):
                self.assertEqual(builder.runtime_executables(os_name, "1.8.24"), images)
                self.assertEqual(builder.service_executable(os_name, "1.8.24"), images[0])
                self.assertEqual(builder.signing_names(os_name, "1.8.24"), images)
                self.assertEqual(builder.runtime_executables(os_name, "1.8.23"), images[:1])
        self.assertEqual(builder.version_command(Path("s"), "airk.exe", "1.8.24"),
                         [str(Path("s/airk.exe")), "__airk_role", "setup-interim", "--version"])
        self.assertEqual(builder.version_command(Path("s"), "airkctl.exe", "1.8.24"), [str(Path("s/airkctl.exe")), "--version"])
        self.assertEqual(builder.version_command(Path("s"), "airk.exe", "1.8.23"), [str(Path("s/airk.exe")), "--version"])

    def test_interface_versions_come_from_the_rust_owners(self):
        # Reviewed values: airk_core::service::PROTOCOL_VERSION and airkctl::contract::I2_CONTRACT.
        self.assertEqual(load_builder("interface_builder").interface_versions(), (1, 1))

    def test_split_pair_metadata_is_schema_3_and_checks_both_identities(self):
        builder = load_builder("split_metadata_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk").write_bytes(b"service image")
            (root / "airkctl").write_bytes(b"cli image")
            calls = []

            def identity(command, **_kwargs):
                calls.append(command[1:])
                return ("airk" if command[1] == "__airk_role" else "airkctl") + " 1.8.24\n"
            with mock.patch.object(builder.subprocess, "check_output", side_effect=identity):
                metadata = builder.write_pair_metadata(root, "1.8.24", "linux", "x86_64", "3" * 64)
            self.assertEqual(calls, [["__airk_role", "setup-interim", "--version"], ["--version"]])
            self.assertEqual(metadata, {"schema": 3, "version": "1.8.24", "os": "linux", "arch": "x86_64",
                                        "airk_sha256": hashlib.sha256(b"service image").hexdigest(),
                                        "airkctl_sha256": hashlib.sha256(b"cli image").hexdigest(),
                                        "i1_protocol": 1, "i2_contract": 1, "admin_inventory_sha256": "3" * 64})
            files = {"airk": metadata["airk_sha256"], "airkctl": metadata["airkctl_sha256"]}
            self.assertTrue(builder.runtime_metadata_matches(metadata, "1.8.24", "linux", "x86_64", files))
            self.assertFalse(builder.runtime_metadata_matches(metadata, "1.8.24", "linux", "x86_64", {"airk": files["airk"]}))
            self.assertFalse(builder.runtime_metadata_matches({**metadata, "schema": 2}, "1.8.24", "linux", "x86_64", files))
            self.assertFalse(builder.runtime_metadata_matches({**metadata, "embed_sha256": "0" * 64}, "1.8.24", "linux", "x86_64", files))
            with mock.patch.object(builder.subprocess, "check_output", return_value="airk 1.8.24\n"):
                with self.assertRaisesRegex(RuntimeError, "Unexpected executable identity"):
                    builder.write_pair_metadata(root, "1.8.24", "linux", "x86_64", "3" * 64)

    def test_cli_image_has_no_inference_dependency_import_or_code(self):
        builder = load_builder("split_cli_builder")
        with mock.patch.object(builder.subprocess, "check_output", return_value="airkctl v1.8.24\nairk-core v1.8.24\nserde v1.0.0\n") as tree:
            builder.check_lightweight_dependencies()
            self.assertEqual(tree.call_args.args[0][:5], ["cargo", "tree", "--locked", "-p", "airkctl"])
        for dependency in ("ort v2.0.0", "ort-sys v2.0.0", "tokenizers v0.21.0", "tract-onnx v0.21.0"):
            with self.subTest(dependency=dependency), mock.patch.object(
                    builder.subprocess, "check_output", return_value=f"airkctl v1.8.24\n{dependency}\n"):
                with self.assertRaisesRegex(RuntimeError, "inference dependencies"):
                    builder.check_lightweight_dependencies()
        with mock.patch.object(builder.subprocess, "check_output", return_value="airk-cli v1.8.24\n"):
            with self.assertRaisesRegex(RuntimeError, "did not report the airkctl package"):
                builder.check_lightweight_dependencies()
        with tempfile.TemporaryDirectory() as temporary:
            image = Path(temporary) / "airkctl"
            image.write_bytes(b"\0" * (1 << 20) + b"plain cli code")
            with mock.patch.object(builder.subprocess, "run", return_value=mock.Mock(stdout="NEEDED libc.so.6")):
                report = builder.verify_cli_imports(image, "linux")
            self.assertEqual(report, {"schema": 1, "command": ["readelf", "-d", str(image)], "stdout": "NEEDED libc.so.6",
                                      "external_onnxruntime_imports": False, "inference_symbols": False})
            # A marker straddling the 1 MiB read boundary is still found.
            image.write_bytes(b"\0" * ((1 << 20) - 4) + b"OrtGetApiBase")
            with mock.patch.object(builder.subprocess, "run", return_value=mock.Mock(stdout="NEEDED libc.so.6")):
                with self.assertRaisesRegex(RuntimeError, "inference code markers: \\['OrtGetApiBase'\\]"):
                    builder.verify_cli_imports(image, "linux")
            image.write_bytes(b"")
            with self.assertRaisesRegex(RuntimeError, "inspection bound"):
                builder.scan_inference_markers(image)

    def test_split_mac_bundle_nests_and_signs_airkctl_before_the_bundle(self):
        builder = load_builder("split_mac_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "airk").write_bytes(b"service")
            (root / "airkctl").write_bytes(b"cli")
            builder.arrange_macos_bundle(root, "1.8.24")
            self.assertEqual(sorted(p.name for p in (root / "AIRK.app/Contents/MacOS").iterdir()), ["airk", "airkctl"])
            self.assertFalse((root / "airkctl").exists())
            with mock.patch.object(builder.subprocess, "run") as signing:
                builder.sign_development_macos_bundle(root)
            targets = [call.args[0][-1] for call in signing.call_args_list]
            self.assertEqual(targets, [str(root / "AIRK.app/Contents/MacOS/airkctl"), str(root / "AIRK.app"), str(root / "AIRK.app")])
            self.assertIn("--verify", signing.call_args_list[2].args[0])

    def test_source_identity_covers_frontend_production_inputs(self):
        builder = load_builder("split_source_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = ["crates/airk-cli/src/lib.rs", "packages/admin/src/app.tsx", "packages/admin/package-lock.json",
                     "packages/contracts-gen/i3.ts", "fixtures/split/i3/admin_operations.json"]
            for name in names:
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("original " + name, encoding="utf-8")
            listing = mock.Mock(return_value=b"\0".join(name.encode() for name in names))
            with mock.patch.object(builder, "ROOT", root), mock.patch.object(builder.subprocess, "check_output", listing):
                original = builder.source_identity()
                pathspec = listing.call_args.args[0]
                for required in ("packages/admin", "packages/ui", "packages/contracts-gen", "crates",
                                 "fixtures/split/i3/admin_operations.json", "scripts/build-admin-assets.py",
                                 "scripts/split_receipts.py"):
                    self.assertIn(required, pathspec)
                for frontend in names[1:]:
                    (root / frontend).write_text("changed frontend input", encoding="utf-8")
                    self.assertNotEqual(builder.source_identity(), original, frontend)
                    (root / frontend).write_text("original " + frontend, encoding="utf-8")
                self.assertEqual(builder.source_identity(), original)

    def test_split_build_needs_the_checked_admin_inventory_before_compiling(self):
        builder = load_builder("split_admin_gate_builder")
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.8.24"\n', encoding="utf-8")
            destination = root / "runtime"
            with (mock.patch.object(builder, "ROOT", root),
                  mock.patch.object(builder, "cargo_binary", side_effect=AssertionError("no Cargo build")),
                  mock.patch.dict(builder.os.environ, {"MACOSX_DEPLOYMENT_TARGET": "13.3"}),
                  mock.patch.object(sys, "argv", [str(script), "--destination", str(destination),
                                                  "--admin-inventory", str(root / "missing.json")])):
                with self.assertRaisesRegex(SystemExit, "build-admin-assets.py"):
                    builder.main()
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
