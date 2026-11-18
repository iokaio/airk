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


def _mac_fixture_signing_options(identity, team, keychain=None):
    """Use the existing explicit keychain only when supplied by the test caller."""
    timestamp = "--timestamp" if team else "--timestamp=none"
    options = ["--force", "--sign", identity, "--options", "runtime", timestamp]
    if keychain:
        options += ["--keychain", str(keychain)]
    return options


class ServiceRuntimeBuildTests(unittest.TestCase):
    def test_static_identity_binds_actual_aggregate_descriptor_and_refuses_mixed_or_tampered_inputs(self):
        specification = importlib.util.spec_from_file_location(
            'aggregate_identity_builder',Path(__file__).resolve().parents[1]/'build-service-runtime.py')
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for target, archive in [('x86_64-pc-windows-msvc','onnxruntime.lib'),
                                ('aarch64-apple-darwin','libonnxruntime.a')]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                (root/'aggregate').mkdir()
                (root/'build-receipt.json').write_bytes(b'original immutable C++ producer')
                receipt = {'schema':1,'result':'genuine_aggregate_not_yet_final_link_or_model_qualified',
                           'plan':{'schema':1,'target':target,'layout':'genuine-monolithic-static',
                                   'inputs':[{'path':'original/archive','sha256':'a'*64}]},
                           'command':{'exit_code':0,'owned_descendants_settled':True},
                           'output':{'path':'aggregate/'+archive,'bytes':123456,'sha256':'b'*64}}
                receipt_file = root/'static-aggregate-receipt.json'
                receipt_file.write_text(builder.json.dumps(receipt),encoding='utf-8')
                descriptor = {'path':receipt_file.name,'sha256':builder.digest(receipt_file),'receipt':receipt}
                environment = {'ORT_LIB_LOCATION':str(root/'aggregate'),'ORT_LIB_PROFILE':'Release'}
                verified = {'receipt':{'original':True},'environment':environment,'static_aggregate':descriptor}
                provisioner = mock.Mock()
                provisioner.verify_build.return_value = verified
                def expected_digest(value):
                    return hashlib.sha256(builder.json.dumps(value,sort_keys=True,separators=(',',':'),
                        ensure_ascii=False,allow_nan=False).encode()).hexdigest()
                with mock.patch.dict(builder.os.environ,environment), \
                     mock.patch.object(builder,'load_script',return_value=provisioner), \
                     mock.patch.object(builder.subprocess,'check_output',return_value='host: '+target+'\n'):
                    observed = builder.verify_static_inputs()
                    self.assertEqual(observed['static_link_closure_sha256'],expected_digest(descriptor))
                    self.assertNotEqual(observed['static_link_closure_sha256'],expected_digest(receipt['plan']))
                    self.assertEqual(observed['verified_inputs_sha256'],expected_digest(verified))
                    self.assertEqual(observed['build_receipt_sha256'],builder.digest(root/'build-receipt.json'))
                    self.assertIsNone(observed['static_link_supplement_sha256'])
                    self.assertIsNone(observed['static_link_recovery_sha256'])
                    for key,value in [('static_link_closure',{'schema':1}),
                                      ('static_link_supplement',{}),('static_link_recovery',{})]:
                        verified[key] = value
                        with self.assertRaisesRegex(RuntimeError,'mix aggregate and legacy'):
                            builder.verify_static_inputs()
                        del verified[key]
                    descriptor['path'] = 'unreviewed.json'
                    with self.assertRaisesRegex(RuntimeError,'aggregate receipt identity'):
                        builder.verify_static_inputs()
                    descriptor['path'] = receipt_file.name
                    descriptor['receipt'] = {**receipt,'schema':True}
                    with self.assertRaisesRegex(RuntimeError,'aggregate receipt identity'):
                        builder.verify_static_inputs()
                    descriptor['receipt'] = receipt
                    original_receipt_bytes = receipt_file.read_bytes()
                    receipt_file.write_bytes(b'x'*65_537)
                    with self.assertRaisesRegex(RuntimeError,'aggregate receipt identity'):
                        builder.verify_static_inputs()
                    receipt_file.write_bytes(original_receipt_bytes)
                    receipt_file.write_bytes(b'changed receipt')
                    with self.assertRaisesRegex(RuntimeError,'aggregate receipt identity'):
                        builder.verify_static_inputs()
                    verified.pop('static_aggregate')
                    with self.assertRaisesRegex(RuntimeError,'lack a verified consumer'):
                        builder.verify_static_inputs()

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
        report = {"schema": 1, "source": "embedded", "engine": "onnxruntime-cpu/1.30.0", "dimensions": 384,
                  "repeat_equal": True, "profile": "minilm-l6-v2-u8-v3", "legacy_identity": "a" * 64, "pipeline_digest": "b" * 64}
        for field, value in ((None, None), ("schema", True), ("source", "directory"), ("engine", "onnxruntime-cpu/1.22.1"),
                             ("dimensions", 383), ("repeat_equal", 1), ("profile", "fp32"), ("profile", "minilm-l6-v2-u8-v1"), ("pipeline_digest", "incomplete")):
            candidate = {**report, **({field: value} if field else {})}
            with self.subTest(field=field), mock.patch.object(builder.subprocess, "check_output", return_value=builder.json.dumps(candidate)) as probe:
                if field:
                    with self.assertRaisesRegex(RuntimeError, "smoke contract differs"):
                        builder.verify_model_smoke(Path("owned"), "linux", "1.9.0")
                else:
                    self.assertEqual(builder.verify_model_smoke(Path("owned"), "linux", "1.9.0"), report)
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

    def test_mac_fixture_signing_keychain_is_explicit_and_optional(self):
        self.assertEqual(_mac_fixture_signing_options("-", None),
                         ["--force", "--sign", "-", "--options", "runtime", "--timestamp=none"])
        identity = "Developer ID Application: Fixture (ABCDEFGHIJ)"
        expected = ["--force", "--sign", identity, "--options", "runtime", "--timestamp"]
        self.assertEqual(_mac_fixture_signing_options(identity, "ABCDEFGHIJ"), expected)
        keychain = "/Users/example/Library/Keychains/airk signing.keychain-db"
        self.assertEqual(_mac_fixture_signing_options(identity, "ABCDEFGHIJ", keychain),
                         expected + ["--keychain", keychain])

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
        self._check_standalone_copy(identity, team, os.environ.get("AIRK_TEST_MAC_KEYCHAIN"))

    def _check_standalone_copy(self, identity, team, keychain=None):
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
            options = _mac_fixture_signing_options(identity, team, keychain)
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
    def test_current_mac_signed_login_plist_matches_independent_i4_fixture(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("current_mac_login_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        expected = (script.parents[1] / "fixtures/split/i4/macos_current_login.plist").read_bytes()
        resources = builder.macos_bundle_files("1.9.0")
        self.assertEqual(resources["AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.stable.plist"], expected)
        for channel in ("stable", "preview"):
            observed = plistlib.loads(resources[f"AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.{channel}.plist"])
            self.assertEqual(observed["ProgramArguments"], ["airk", "__airk_role", "service-login-current"])
            self.assertEqual(observed["Label"], "io.iokaio.airk.service." + channel)

    def test_mac_sealed_login_entry_changes_only_for_direct_service_versions(self):
        script = Path(__file__).resolve().parents[1] / "build-service-runtime.py"
        specification = importlib.util.spec_from_file_location("service_runtime_builder", script)
        builder = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(builder)
        for version, arguments in (("1.8.13", ["airk", "runtime", "login-foreground"]), ("1.8.14", ["airk", "runtime", "login-serve"]), ("1.8.99", ["airk", "runtime", "login-serve"]), ("1.9.0", ["airk", "__airk_role", "service-login-current"]), ("1.9.1", ["airk", "__airk_role", "service-login-current"])):
            resources = builder.macos_bundle_files(version)
            for channel in ("stable", "preview"):
                name = f"AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.{channel}.plist"
                definition = plistlib.loads(resources[name])
                self.assertEqual(definition, {
                    "Label": f"io.iokaio.airk.service.{channel}",
                    "BundleProgram": "Contents/MacOS/airk",
                    "ProgramArguments": arguments if version in ("1.9.0", "1.9.1") else arguments + ["--channel", channel],
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

    def test_source_identity_seals_the_compiled_signer_policy(self):
        builder = load_builder("signer_source_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            policy = root / "scripts/signing-identity.json"
            policy.parent.mkdir()
            policy.write_bytes(b'{"publisher":"reviewed fixture"}')
            def listing(command, **_kwargs):
                return b"scripts/signing-identity.json\0" if "scripts/signing-identity.json" in command else b""
            with mock.patch.object(builder, "ROOT", root), mock.patch.object(builder.subprocess, "check_output", side_effect=listing):
                before = builder.source_identity()
                policy.write_bytes(b'{"publisher":"different signer policy"}')
                self.assertNotEqual(builder.source_identity(), before)
                self.assertEqual(before["files"], 1)

    def test_source_identity_seals_the_actual_aggregate_verifier(self):
        builder = load_builder('aggregate_verifier_source')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            verifier = root/'scripts/static-onnx-aggregate.py'
            verifier.parent.mkdir()
            verifier.write_bytes(b'original genuine aggregate verifier')
            def listing(command, **_kwargs):
                return b'scripts/static-onnx-aggregate.py\0' if 'scripts/static-onnx-aggregate.py' in command else b''
            with mock.patch.object(builder,'ROOT',root), \
                 mock.patch.object(builder.subprocess,'check_output',side_effect=listing):
                before = builder.source_identity()
                verifier.write_bytes(b'changed verifier policy')
                self.assertNotEqual(builder.source_identity(),before)
                self.assertEqual(before['files'],1)

    def test_new_external_literal_and_manifest_embeds_enter_the_source_seal(self):
        builder = load_builder("external_embed_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "crates/core/src/lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text('const HELP: &str = include_str!("../../../help/topic.txt");\n'
                              'const MODEL: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/weights.bin"));\n')
            for name in ("help/topic.txt", "models/weights.bin"):
                path = root / name
                path.parent.mkdir()
                path.write_bytes(b"original independent embed")
            with (mock.patch.object(builder, "ROOT", root),
                  mock.patch.object(builder.subprocess, "check_output", return_value=b"crates/core/src/lib.rs\0")):
                before = builder.source_identity()
                self.assertEqual(before["files"], 3)
                for name in ("help/topic.txt", "models/weights.bin"):
                    path = root / name
                    path.write_bytes(b"different compiled bytes")
                    self.assertNotEqual(builder.source_identity(), before, name)
                    path.write_bytes(b"original independent embed")
                (root / "help/topic.txt").unlink()
                with self.assertRaisesRegex(RuntimeError, "Compiled source input is missing"):
                    builder.source_identity()


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
        # `cargo tree --prefix depth`: a depth number before each package.
        with mock.patch.object(builder.subprocess, "check_output", return_value="0airkctl v1.8.24\n1airk-core v1.8.24\n2serde v1.0.0\n") as tree:
            builder.check_lightweight_dependencies()
            self.assertEqual(tree.call_args.args[0][:5], ["cargo", "tree", "--locked", "-p", "airkctl"])
        for dependency in ("ort v2.0.0", "ort-sys v2.0.0", "tokenizers v0.21.0", "tract-onnx v0.21.0"):
            with self.subTest(dependency=dependency), mock.patch.object(
                    builder.subprocess, "check_output", return_value=f"0airkctl v1.8.24\n1{dependency}\n"):
                with self.assertRaisesRegex(RuntimeError, "inference dependencies"):
                    builder.check_lightweight_dependencies()
        with mock.patch.object(builder.subprocess, "check_output", return_value="0airk-cli v1.8.24\n"):
            with self.assertRaisesRegex(RuntimeError, "did not report the airkctl package"):
                builder.check_lightweight_dependencies()
        # The paired build resolves both roots together; only airkctl's closure counts.
        unified = "0airk-cli v1.8.24\n1ort v2.0.0\n\n0airkctl v1.8.24\n1airk-core v1.8.24\n"
        with mock.patch.object(builder.subprocess, "check_output", return_value=unified) as tree:
            builder.check_lightweight_dependencies(unified=True)
            self.assertIn("airk-cli/local-inference", tree.call_args.args[0])
        leaked = "0airk-cli v1.8.24\n1ort v2.0.0\n\n0airkctl v1.8.24\n1ort v2.0.0\n"
        with mock.patch.object(builder.subprocess, "check_output", return_value=leaked):
            with self.assertRaisesRegex(RuntimeError, "inference dependencies"):
                builder.check_lightweight_dependencies(unified=True)
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

    def test_generated_budget_bytes_invalidate_runtime_source_identity_without_admitting_neighbors(self):
        builder = load_builder("budget_source_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "--quiet", str(root)], check=True, capture_output=True)
            budget = root / "packages/vscode/src/generated/arbiterBudget.ts"
            budget.parent.mkdir(parents=True)
            original_bytes = b"export const READ_OPERATION_DEADLINE_MS = 120000;\n"
            budget.write_bytes(original_bytes)
            neighbor = budget.with_name("unreviewedBudget.ts")
            neighbor.write_bytes(b"private unrelated source")
            # Exercise the real Git pathspec selection, including untracked public
            # generation outputs. A mocked listing could hide a missing source row.
            with mock.patch.object(builder, "ROOT", root):
                before = builder.source_identity()
                self.assertEqual(before["files"], 1)
                neighbor.write_bytes(b"changed unrelated source")
                self.assertEqual(builder.source_identity(), before)
                budget.write_bytes(b"export const READ_OPERATION_DEADLINE_MS = 120001;\n")
                self.assertNotEqual(builder.source_identity()["sha256"], before["sha256"])
                budget.write_bytes(original_bytes)
                self.assertEqual(builder.source_identity(), before)

    def test_source_identity_covers_frontend_production_inputs(self):
        builder = load_builder("split_source_builder")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = ["crates/airk-cli/src/lib.rs", "packages/admin/src/app.tsx", "packages/admin/package-lock.json",
                     "packages/contracts-gen/i3.ts", "fixtures/split/i3/admin_operations.json",
                     "packages/vscode/src/uiPilotContract.ts", "packages/vscode/src/generated/i2.ts"]
            for name in names:
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("original " + name, encoding="utf-8")
            listing = mock.Mock(return_value=b"\0".join(name.encode() for name in names))
            with mock.patch.object(builder, "ROOT", root), mock.patch.object(builder.subprocess, "check_output", listing):
                original = builder.source_identity()
                pathspec = listing.call_args.args[0]
                for required in ("packages/admin", "packages/ui", "packages/contracts-gen", "crates",
                                 "fixtures/split/i3/admin_operations.json", "scripts/build-admin-assets.py",
                                 "scripts/split_receipts.py", "packages/vscode/src/uiPilotContract.ts",
                                 "packages/vscode/src/generated/i2.ts"):
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


class GpuServiceRuntimeBuildTests(unittest.TestCase):
    def builder(self):
        specification = importlib.util.spec_from_file_location(
            "gpu_runtime_builder", Path(__file__).resolve().parents[1] / "build-service-runtime.py")
        module = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(module)
        return module

    def handoff(self, root):
        return {"environment": {"LIBONNXRUNTIME_NO_PKG_CONFIG": "1", "ORT_SKIP_DOWNLOAD": "1",
                "ORT_PREFER_DYNAMIC_LINK": "0", "ORT_LIB_LOCATION": str(root / "gpu-aggregate"),
                "ORT_LIB_PROFILE": "Release"},
                "static_ort": {"build_receipt_sha256": "a" * 64, "target": "x86_64-pc-windows-msvc",
                    "static_link_closure_sha256": "b" * 64, "static_link_supplement_sha256": None,
                    "static_link_recovery_sha256": None, "verified_inputs_sha256": "c" * 64},
                "activation_pins_available": False}

    def test_gpu_handoff_requires_supported_host_before_any_proof_or_cargo(self):
        builder = self.builder()
        for system, machine in [("Linux", "x86_64"), ("Darwin", "arm64"), ("Windows", "arm64")]:
            with self.subTest(system=system, machine=machine), \
                 mock.patch.object(builder.platform, "system", return_value=system), \
                 mock.patch.object(builder.platform, "machine", return_value=machine), \
                 mock.patch.object(builder, "load_script") as load, \
                 mock.patch.object(builder.subprocess, "run") as child:
                with self.assertRaisesRegex(RuntimeError, "only on Windows x86_64"):
                    builder.cuda_handoff(Path("not-opened"), "a" * 64)
                load.assert_not_called()
                child.assert_not_called()

    def test_gpu_handoff_refuses_absent_or_nonliteral_pin_before_owner(self):
        builder = self.builder()
        for pin in [None, "", "A" * 64, "a" * 63, "not-a-pin"]:
            with self.subTest(pin=pin), \
                 mock.patch.object(builder.platform, "system", return_value="Windows"), \
                 mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
                 mock.patch.object(builder, "load_script") as load:
                with self.assertRaisesRegex(RuntimeError, "explicit reviewed CUDA"):
                    builder.cuda_handoff(Path("not-opened"), pin)
                load.assert_not_called()

    def test_gpu_handoff_preserves_maintained_refusal_and_never_runs_cargo(self):
        builder = self.builder()
        owner = mock.Mock()
        owner.verify_handoff.side_effect = ValueError("original incomplete producer/aggregate")
        with mock.patch.object(builder.platform, "system", return_value="Windows"), \
             mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
             mock.patch.object(builder, "load_script", return_value=owner) as load, \
             mock.patch.object(builder.subprocess, "run") as child:
            with self.assertRaisesRegex(ValueError, "original incomplete producer"):
                builder.cuda_handoff(Path("environment.json"), "a" * 64)
            self.assertEqual(load.call_args.args, ("cuda-runtime-environment",))
            child.assert_not_called()

    def test_gpu_handoff_uses_only_exact_five_static_values_and_current_target(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            bad_values = [("ORT_SKIP_DOWNLOAD", "0"), ("ORT_PREFER_DYNAMIC_LINK", "1"),
                          ("LIBONNXRUNTIME_NO_PKG_CONFIG", "0"), ("ORT_LIB_PROFILE", "Debug"),
                          ("ORT_LIB_LOCATION", "relative"), ("UNREVIEWED_EXTRA", "1")]
            for field, bad in bad_values + [(None, "aarch64-apple-darwin")]:
                value = self.handoff(root)
                if field is None:
                    value["static_ort"]["target"] = bad
                else:
                    value["environment"][field] = bad
                owner = mock.Mock()
                owner.verify_handoff.return_value = value
                with self.subTest(field=field), \
                     mock.patch.object(builder.platform, "system", return_value="Windows"), \
                     mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
                     mock.patch.object(builder, "load_script", return_value=owner):
                    with self.assertRaisesRegex(RuntimeError, "environment or target differs"):
                        builder.cuda_handoff(root / "environment.json", "a" * 64)

    def test_gpu_environment_is_child_only_and_drops_ambient_activation_claims(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            value = self.handoff(root)
            before_value = builder.json.dumps(value, sort_keys=True)
            owner = mock.Mock()
            owner.verify_handoff.return_value = value
            ambient = {"PATH": "original-user-path", "ORT_LIB_LOCATION": "original-cpu-input",
                       "AIRK_APPROVED_CUDA_MANIFEST_SHA256": "d" * 64,
                       "AIRK_STATIC_CUDA_BUILD_SHA256": "e" * 64}
            with mock.patch.dict(builder.os.environ, ambient, clear=True), \
                 mock.patch.object(builder.platform, "system", return_value="Windows"), \
                 mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
                 mock.patch.object(builder, "load_script", return_value=owner):
                verified, child = builder.cuda_handoff(root / "environment.json", "f" * 64)
                owner.verify_handoff.assert_called_once_with(root / "environment.json", "f" * 64, builder.ROOT)
                self.assertEqual(dict(builder.os.environ), ambient)
                self.assertEqual(child["PATH"], "original-user-path")
                self.assertEqual(child["ORT_LIB_LOCATION"], str(root / "gpu-aggregate"))
                self.assertNotIn("AIRK_APPROVED_CUDA_MANIFEST_SHA256", child)
                self.assertNotIn("AIRK_STATIC_CUDA_BUILD_SHA256", child)
                self.assertEqual(builder.json.dumps(verified, sort_keys=True), before_value)
                self.assertEqual(set(verified["static_ort"]), {"build_receipt_sha256", "target",
                    "static_link_closure_sha256", "static_link_supplement_sha256",
                    "static_link_recovery_sha256", "verified_inputs_sha256"})

    def test_bounded_fresh_handoff_deadline_reaches_maintained_owner(self):
        builder = self.builder()
        root = Path(__file__).resolve().parents[2]
        owner = mock.Mock()
        owner.verify_handoff.return_value = self.handoff(root)
        with mock.patch.object(builder.platform, "system", return_value="Windows"), \
             mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
             mock.patch.object(builder.time, "monotonic", return_value=10), \
             mock.patch.object(builder, "load_script", return_value=owner):
            builder.cuda_handoff(root / "environment.json", "f" * 64, 600)
            owner.verify_handoff.assert_called_once_with(
                root / "environment.json", "f" * 64, builder.ROOT, deadline=610,
            )
            for seconds in (True, 0, 601, float("nan")):
                with self.subTest(seconds=seconds), self.assertRaisesRegex(RuntimeError, "bounded"):
                    builder.cuda_handoff(root / "environment.json", "f" * 64, seconds)
            self.assertEqual(owner.verify_handoff.call_count, 1)

    def test_paired_cargo_cpu_default_and_explicit_gpu_match_exact_tree_features(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for name in ["airk.exe", "airkctl.exe"]:
                (root / name).write_bytes(b"synthetic command-selection image; never executed")
            events = "\n".join(builder.json.dumps({"reason": "compiler-artifact",
                      "target": {"name": name}, "executable": str(root / (name + ".exe"))})
                      for name in ["airk", "airkctl"])
            for gpu, features in [(False, "airk-cli/local-inference"), (True, "airk-cli/gpu-cuda")]:
                child = {"proof": "child-only"} if gpu else None
                with self.subTest(gpu=gpu), \
                     mock.patch.object(builder.subprocess, "run", return_value=mock.Mock(stdout=events)) as cargo, \
                     mock.patch.object(builder.subprocess, "check_output", return_value="0airkctl v1\n1airk-core v1\n") as tree:
                    if gpu:
                        builder.cargo_pair("debug", features=features, environment=child)
                        builder.check_lightweight_dependencies(unified=True, features=features, environment=child)
                    else:
                        builder.cargo_pair("debug")
                        builder.check_lightweight_dependencies(unified=True)
                    build_args = cargo.call_args.args[0]
                    tree_args = tree.call_args.args[0]
                    self.assertEqual(build_args[build_args.index("--features") + 1], features)
                    self.assertEqual(tree_args[tree_args.index("--features") + 1], features)
                    self.assertIn("--locked", build_args)
                    self.assertIn("--timings", build_args)
                    self.assertEqual(cargo.call_count, 1)
                    self.assertEqual(cargo.call_args.kwargs.get("env"), child)
                    self.assertEqual(tree.call_args.kwargs.get("env"), child)

    def test_gpu_unification_does_not_admit_inference_into_airkctl(self):
        builder = self.builder()
        with mock.patch.object(builder.subprocess, "check_output", return_value="0airkctl v1\n1ort v2\n"):
            with self.assertRaisesRegex(RuntimeError, "airkctl includes inference"):
                builder.check_lightweight_dependencies(unified=True, features="airk-cli/gpu-cuda",
                                                       environment={"proof": "child-only"})


class GpuShippingRuntimeBuildTests(unittest.TestCase):
    builder = GpuServiceRuntimeBuildTests.builder
    handoff = GpuServiceRuntimeBuildTests.handoff
    def native(self, root):
        files = {"airk-cuda-runtime.json": "d" * 64,
                 "onnxruntime_providers_cuda.dll": "e" * 64,
                 "onnxruntime_providers_shared.dll": "f" * 64,
                 "airk-cuda-build-provenance.json": "1" * 64,
                 "airk-cuda-notice-cuda-license.txt": "2" * 64,
                 "airk-cuda-notice-cudnn-license.txt": "7" * 64,
                 "airk-cuda-notice-onnxruntime-license.txt": "8" * 64}
        return {"files": files, "source_root": root,
                "native_projection": {"path": str(root / "projection.json"), "bytes": 123, "sha256": "3" * 64},
                "native_execution_receipt": {"path": str(root / "execution.json"), "bytes": 456, "sha256": "4" * 64},
                "static_core_build": "a" * 64,
                "descriptor": {"name": "airk-cuda-runtime.json", "bytes": 20, "sha256": "d" * 64},
                "activation": False, "qualification": False}

    def prepare(self, builder, root, handoff, native):
        public = mock.Mock()
        public.require_native_activation_projection.return_value = native
        with mock.patch.object(builder, "cuda_handoff", return_value=(handoff, {"PATH": "original"})), \
             mock.patch.object(builder, "load_script", return_value=public):
            result = builder.prepare_cuda_inputs(root / "environment.json", "5" * 64,
                root / "projection.json", "3" * 64, root / "execution.json", "4" * 64)
        return result, public

    def test_native_projection_binds_actual_static_build_and_exact_child_compile_pins(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            handoff, native = self.handoff(root), self.native(root)
            result, public = self.prepare(builder, root, handoff, native)
            public.require_native_activation_projection.assert_called_once_with(
                root / "projection.json", "3" * 64, handoff,
                native_execution_receipt=(root / "execution.json", "4" * 64), repo_root=builder.ROOT)
            self.assertEqual(result["environment"], {"PATH": "original",
                "AIRK_STATIC_CUDA_BUILD_SHA256": "a" * 64,
                "AIRK_APPROVED_CUDA_MANIFEST_SHA256": "d" * 64})
            expected_keys = {"build_receipt_sha256", "target", "static_link_closure_sha256",
                "static_link_supplement_sha256", "static_link_recovery_sha256", "verified_inputs_sha256"}
            self.assertEqual(set(result["static_ort"]), expected_keys)
            for key in expected_keys - {"verified_inputs_sha256"}:
                self.assertEqual(result["static_ort"][key], handoff["static_ort"][key])
            original = result["static_ort"]["verified_inputs_sha256"]
            self.assertNotEqual(original, handoff["static_ort"]["verified_inputs_sha256"])
            native["files"]["airk-cuda-notice-cuda-license.txt"] = "6" * 64
            changed, _ = self.prepare(builder, root, handoff, native)
            self.assertNotEqual(changed["static_ort"]["verified_inputs_sha256"], original)

    def test_native_projection_refuses_mixed_build_unbound_receipt_and_activation_claims(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for mutation in ["static_core_build", "projection", "execution", "descriptor", "activation", "qualification"]:
                native = self.native(root)
                if mutation == "projection":
                    native["native_projection"]["sha256"] = "7" * 64
                elif mutation == "execution":
                    native["native_execution_receipt"]["sha256"] = "7" * 64
                elif mutation == "descriptor":
                    native["descriptor"]["name"] = "unreviewed-runtime.json"
                elif mutation in ("activation", "qualification"):
                    native[mutation] = True
                else:
                    native[mutation] = "7" * 64
                with self.subTest(mutation=mutation), self.assertRaisesRegex(RuntimeError, "same-build input pins"):
                    self.prepare(builder, root, self.handoff(root), native)

    def test_reverification_refuses_static_or_native_proof_drift(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            handoff, native = self.handoff(root), self.native(root)
            value, _ = self.prepare(builder, root, handoff, native)
            public = mock.Mock()
            public.reverify_native_projection.return_value = native
            with mock.patch.object(builder, "cuda_handoff", return_value=(handoff, {})), \
                 mock.patch.object(builder, "load_script", return_value=public):
                self.assertEqual(builder.verify_static_inputs(value), value["static_ort"])
                public.reverify_native_projection.return_value = {**native, "static_core_build": "8" * 64}
                with self.assertRaisesRegex(RuntimeError, "projection changed"):
                    builder.verify_static_inputs(value)
            with mock.patch.object(builder, "cuda_handoff", return_value=({**handoff, "unreviewed": True}, {})):
                with self.assertRaisesRegex(RuntimeError, "build/aggregate inputs changed"):
                    builder.verify_static_inputs(value)

    def test_exact_owned_staging_rejects_roster_or_file_drift(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "airk-cuda-runtime.json").write_bytes(b"native owned test descriptor bytes")
            native = {"files": {"airk-cuda-runtime.json": builder.digest(root / "airk-cuda-runtime.json")}}
            value = {"projection": native, "handoff": {"genuine_owner_boundary": "synthetic control"}}
            public = mock.Mock()
            public.stage_native_projection.return_value = dict(native["files"])
            with mock.patch.object(builder, "load_script", return_value=public):
                builder.stage_cuda_projection(value, root)
                public.stage_native_projection.return_value = {}
                with self.assertRaisesRegex(RuntimeError, "staging roster differs"):
                    builder.stage_cuda_projection(value, root)
                public.stage_native_projection.return_value = dict(native["files"])
                (root / "airk-cuda-runtime.json").write_bytes(b"actual bytes drift")
                with self.assertRaisesRegex(RuntimeError, "Staged native CUDA bytes changed"):
                    builder.stage_cuda_projection(value, root)

    def main_args(self, root):
        return ["builder", "--destination", str(root / "output"), "--admin-inventory", str(root / "admin.json"),
                "--cuda-environment", str(root / "environment.json"), "--cuda-environment-sha256", "5" * 64,
                "--cuda-projection", str(root / "projection.json"), "--cuda-projection-sha256", "3" * 64,
                "--cuda-native-execution-receipt", str(root / "execution.json"),
                "--cuda-native-execution-receipt-sha256", "4" * 64]

    def test_main_optin_routes_exact_paired_feature_environment_and_qualification_owner(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.9.0"\n')
            (root / "admin.json").write_text("{}")
            for name in ["airk.exe", "airkctl.exe"]:
                (root / name).write_bytes(b"synthetic artifact for argv-only control; never executed")
            child = {"PATH": "original", "AIRK_STATIC_CUDA_BUILD_SHA256": "a" * 64,
                     "AIRK_APPROVED_CUDA_MANIFEST_SHA256": "d" * 64}
            cuda = {"environment": child, "static_ort": {"opaque": "same owned six-field value"}}
            with mock.patch.object(builder, "ROOT", root), \
                 mock.patch.object(builder.platform, "system", return_value="Windows"), \
                 mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
                 mock.patch.object(builder, "load_script"), \
                 mock.patch.object(builder, "source_identity", return_value={"sha256": "e" * 64, "files": 1}), \
                 mock.patch.object(builder, "prepare_cuda_inputs", return_value=cuda) as prepare, \
                 mock.patch.object(builder, "verify_static_inputs", side_effect=AssertionError("CPU verifier must not be called")), \
                 mock.patch.object(builder, "check_lightweight_dependencies") as tree, \
                 mock.patch.object(builder, "cargo_binary", side_effect=AssertionError("separate CPU build must not run")), \
                 mock.patch.object(builder, "cargo_pair", return_value=(root / "airk.exe", root / "airkctl.exe")) as cargo, \
                 mock.patch.object(builder, "qualify_staged_runtime") as qualify, \
                 mock.patch.object(sys, "argv", self.main_args(root)):
                builder.main()
                prepare.assert_called_once_with(root / "environment.json", "5" * 64,
                    root / "projection.json", "3" * 64, root / "execution.json", "4" * 64)
                cargo.assert_called_once_with("debug", features="airk-cli/gpu-cuda", environment=child)
                tree.assert_called_once_with(unified=True, features="airk-cli/gpu-cuda", environment=child)
                self.assertIs(qualify.call_args.kwargs["cuda"], cuda)
                self.assertEqual((root / "output/airk.exe").read_bytes(), (root / "airk.exe").read_bytes())

    def test_main_native_proof_refusal_prevents_cargo_and_publication(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "Cargo.toml").write_text('[workspace.package]\nversion="1.9.0"\n')
            (root / "admin.json").write_text("{}")
            with mock.patch.object(builder, "ROOT", root), \
                 mock.patch.object(builder.platform, "system", return_value="Windows"), \
                 mock.patch.object(builder.platform, "machine", return_value="AMD64"), \
                 mock.patch.object(builder, "load_script"), \
                 mock.patch.object(builder, "source_identity", return_value={"sha256": "e" * 64, "files": 1}), \
                 mock.patch.object(builder, "prepare_cuda_inputs", side_effect=ValueError("actual native custody is incomplete")), \
                 mock.patch.object(builder, "cargo_pair") as cargo, \
                 mock.patch.object(builder, "cargo_binary") as separate, \
                 mock.patch.object(sys, "argv", self.main_args(root)):
                with self.assertRaisesRegex(ValueError, "actual native custody"):
                    builder.main()
                cargo.assert_not_called()
                separate.assert_not_called()
                self.assertFalse((root / "output").exists())

    def test_main_partial_gpu_flags_refuse_before_any_owner_or_output(self):
        builder = self.builder()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            with mock.patch.object(builder, "load_script") as owner, \
                 mock.patch.object(builder, "cargo_pair") as cargo, \
                 mock.patch.object(sys, "argv", ["builder", "--destination", str(root / "output"),
                                                 "--cuda-environment", str(root / "environment.json")]):
                with self.assertRaisesRegex(SystemExit, "All six explicit CUDA"):
                    builder.main()
                owner.assert_not_called()
                cargo.assert_not_called()
                self.assertFalse((root / "output").exists())

    def test_gpu_verifier_sources_enter_exact_source_seal_without_neighbor_directory(self):
        builder = self.builder()
        names = ["scripts/cuda-runtime-environment.py", "scripts/cuda-package-provenance.py",
                 "scripts/cuda-native-execution.py", "scripts/cuda-static-runtime.py",
                 "scripts/cuda-static-archive.py", "scripts/cuda-static-aggregate.py",
                 "scripts/cuda-static-continuation.py", "scripts/cuda-static-continuation-r12.py",
                 "scripts/cuda-static-checkpointed.py"]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "scripts").mkdir()
            for name in names:
                (root / name).write_bytes(b"original source-seal fixture")
            neighbor = root / "scripts/not-an-input.py"
            neighbor.write_bytes(b"unrelated neighboring file")
            with mock.patch.object(builder, "ROOT", root), \
                 mock.patch.object(builder.subprocess, "check_output", return_value=b"\0".join(n.encode() for n in names)) as listing:
                original = builder.source_identity()
                for name in names:
                    self.assertIn(name, listing.call_args.args[0])
                self.assertNotIn("scripts", listing.call_args.args[0])
                neighbor.write_bytes(b"unrelated changed neighbor")
                self.assertEqual(builder.source_identity(), original)
                for name in names:
                    path = root / name
                    path.write_bytes(b"changed declared verifier source")
                    self.assertNotEqual(builder.source_identity(), original)
                    path.write_bytes(b"original source-seal fixture")


if __name__ == "__main__":
    unittest.main()
