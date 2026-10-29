# SPDX-License-Identifier: Apache-2.0
"""Production model and library boundaries, including stale package inputs."""
import copy
import importlib.util
from pathlib import Path
import struct
import plistlib
import tempfile
import unittest
from unittest.mock import patch


def script(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).parents[1] / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Int8Distribution(unittest.TestCase):
    def test_mac_notices_are_resources_and_all_staged_assets_match_package_paths(self):
        builder = script("build-service-runtime")
        provisioner = script("provision-onnx-runtime")
        for system, architecture in [("macos", "aarch64"), ("macos", "x86_64"),
                                     ("windows", "x86_64"), ("linux", "x86_64")]:
            with self.subTest(system=system, architecture=architecture), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = system + "-" + architecture
                prepared = root / "inputs" / target
                prepared.mkdir(parents=True)
                rows = provisioner.entries(target)
                for row in rows:
                    (prepared / row["name"]).write_bytes(row["name"].encode())
                with patch.dict("os.environ", AIRK_BUNDLED_RUNTIME_DIR=str(root / "inputs")), \
                     patch.object(provisioner, "provision", return_value=rows), \
                     patch.object(builder, "load_script", return_value=provisioner):
                    builder.stage_inference_runtime(root / "out", system, architecture)
                    expected = builder.inference_assets(system, architecture)
                actual = {p.relative_to(root / "out").as_posix() for p in (root / "out").rglob("*") if p.is_file()}
                self.assertEqual(actual, set(expected))
                for name, row in expected.items():
                    self.assertEqual((root / "out" / name).read_bytes(), row["name"].encode())
                    if system == "macos":
                        parent = "AIRK.app/Contents/Resources/onnxruntime/" if row["name"].startswith("ONNXRUNTIME-") else "AIRK.app/Contents/MacOS/"
                        self.assertEqual(name, parent + row["name"])

    def test_mac_minimum_includes_the_native_inference_library(self):
        builder = script("build-service-runtime")
        for version, minimum in [("1.6.0", "13.3"), ("1.4.4", "13.0")]:
            info = plistlib.loads(builder.macos_bundle_files(version)["AIRK.app/Contents/Info.plist"])
            self.assertEqual(info["LSMinimumSystemVersion"], minimum)

    def test_container_runtime_inputs_survive_a_target_cache_overlay(self):
        builder = script("build-service-runtime")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prepared = root / "runtime-assets/linux-x86_64"
            prepared.mkdir(parents=True)
            (prepared / "libonnxruntime.so").write_bytes(b"library fixture")
            provisioner = script("provision-onnx-runtime")
            def provision(directory, target):
                self.assertEqual(directory, prepared)
                self.assertEqual(target, "linux-x86_64")
                return [{"name": "libonnxruntime.so"}]
            provisioner.provision = provision
            with patch.dict("os.environ", AIRK_BUNDLED_RUNTIME_DIR=str(root / "runtime-assets")), \
                 patch.object(builder, "load_script", return_value=provisioner):
                builder.stage_inference_runtime(root / "out", "linux", "x86_64")
            self.assertEqual((root / "out/libonnxruntime.so").read_bytes(), b"library fixture")

    def test_fp32_or_unapproved_weights_cannot_replace_or_join_bundled_profiles(self):
        policy = script("int8-policy")
        catalog = policy.catalog()
        report = {"schema": 1, "fp32_included": False, "profiles": copy.deepcopy(catalog["profiles"])}
        policy.verify_embedded(report)
        for field, value in [("precision", "fp32"), ("id", "legacy-fp32")]:
            altered = copy.deepcopy(catalog)
            altered["profiles"][0][field] = value
            with self.assertRaises(ValueError): policy.validate_catalog(altered)
        altered = copy.deepcopy(catalog)
        old = "6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452"
        altered["profiles"][0]["manifest"]["model_sha256"] = old
        next(a for a in altered["profiles"][0]["assets"] if a["name"] == "model.onnx")["sha256"] = old
        with self.assertRaises(ValueError): policy.validate_catalog(altered)
        for change in (lambda r: r["profiles"].append(r["profiles"][0]),
                       lambda r: r["profiles"].pop(),
                       lambda r: r["profiles"][0]["manifest"].update(model_sha256=old),
                       lambda r: r.update(fp32_included=True)):
            altered = copy.deepcopy(report)
            change(altered)
            with self.assertRaises(ValueError): policy.verify_embedded(altered)

    def test_macho_identity_allows_only_signature_bytes_and_size_fields(self):
        macho = script("macho-identity")
        data = bytearray(256)
        def put(offset, value): struct.pack_into("<I", data, offset, value)
        put(0, 0xfeedfacf); put(16, 2); put(20, 88)
        put(32, 0x19); put(36, 72)
        data[40:56] = b"__LINKEDIT" + bytes(6)
        put(64, 4096); put(72, 128); put(80, 128)
        put(104, 0x1D); put(108, 16); put(112, 240); put(116, 16)
        original = macho.payload_digest(data)
        data.extend(bytes([7]) * 16); put(80, 144); put(116, 32)
        self.assertEqual(macho.payload_digest(data), original)
        data[150] ^= 1
        self.assertNotEqual(macho.payload_digest(data), original)
        put(116, 33)
        with self.assertRaises(ValueError): macho.payload_digest(data)
        for end in range(120):
            with self.assertRaises(ValueError): macho.payload_digest(data[:end])

    def test_offline_runtime_requires_exact_platform_libraries_and_notices(self):
        runtime = script("provision-onnx-runtime")
        for platform in ("windows-x86_64", "linux-x86_64", "macos-aarch64", "macos-x86_64"):
            rows = runtime.entries(platform)
            self.assertTrue(any(a["name"].endswith((".dll", ".so", ".dylib")) for a in rows))
            self.assertTrue({"ONNXRUNTIME-LICENSE", "ONNXRUNTIME-ThirdPartyNotices.txt"}.issubset(a["name"] for a in rows))
        with self.assertRaises(ValueError): runtime.entries("unknown-cpu")


if __name__ == "__main__":
    unittest.main()
