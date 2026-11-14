# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("package_native", Path(__file__).parents[1] / "package-native.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ArchiveContracts(unittest.TestCase):
    def test_single_image_inventory_rejects_helpers_libraries_and_legacy_metadata(self):
        builder = package.runtime_builder()
        for os_name, target, main in (("linux", "x86_64-unknown-linux-gnu", "airk"),
                                      ("windows", "x86_64-pc-windows-msvc", "airk.exe"),
                                      ("macos", "aarch64-apple-darwin", "AIRK.app/Contents/MacOS/airk")):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                source = {"sha256": "1" * 64, "files": 1}
                legal = ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json",
                         "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json")
                for name in (main, *legal):
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"fictional single-image fixture")
                (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
                lock = json.loads((builder.ROOT / "scripts/static-onnx-runtime-lock.json").read_text())
                prefix = "notices/static-onnx-runtime/"
                source_names = ["notices/" + name + "/LICENSE" for name in ["onnxruntime", *[row["name"] for row in lock["dependencies"]]]]
                source_names.append("notices/onnxruntime/ThirdPartyNotices.txt")
                static_names = [builder.packaged_static_notice_path(name) for name in source_names]
                for name in static_names:
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"fictional static legal bytes")
                (root / prefix / "inventory.json").write_text(json.dumps({"schema": 2, "ort_version": lock["ort_version"], "target": target,
                    "build_receipt_sha256": "e" * 64,
                    "lock_sha256": builder.digest(builder.ROOT / "scripts/static-onnx-runtime-lock.json"),
                    "sources": {name: {"path": source, "bytes": (root / name).stat().st_size, "sha256": package.sha256(root / name)} for name, source in zip(static_names, source_names)},
                    "files": {name: package.sha256(root / name) for name in static_names}}), encoding="utf-8")
                if os_name == "macos":
                    for name, data in builder.macos_bundle_files("1.8.14").items():
                        path = root / name
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes(data)
                metadata = dict(schema=2, version="1.8.14", os=os_name,
                                arch="aarch64" if os_name == "macos" else "x86_64", airk_sha256=package.sha256(root / main))
                def seal():
                    (root / "airk-runtime.json").write_text(json.dumps(metadata), encoding="utf-8")
                    (root / "airk-build.json").write_text(json.dumps(dict(schema=1, source=source, runtime=metadata)), encoding="utf-8")
                    install = {key: metadata[key] for key in ("version", "os", "arch")}
                    install.update(schema=1, files={name: value for name, value in package.inventory(root).items() if name != "airk-install.json"})
                    (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
                seal()
                files, executables = package.validate_runtime(root, "1.8.14", target, source)
                self.assertEqual(executables, (main,))
                self.assertIn(main, files)
                for forbidden in ("airk-embed", "airk-launcher-image", "onnxruntime.dll", "libonnxruntime.dylib", "model.onnx"):
                    (root / forbidden).write_bytes(b"must not ship")
                    seal()
                    with self.assertRaisesRegex(ValueError, "Unexpected runtime payload"):
                        package.validate_runtime(root, "1.8.14", target, source)
                    (root / forbidden).unlink()
                metadata["embed_sha256"] = "0" * 64
                seal()
                with self.assertRaisesRegex(ValueError, "executable digests"):
                    package.validate_runtime(root, "1.8.14", target, source)

    def test_current_mac_archive_requires_and_preserves_standalone_launcher_image(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            builder = package.runtime_builder()
            version = "1.8.8"
            for name in ("airk", "airk-embed"):
                (root / name).write_bytes(name.encode())
            builder.arrange_macos_bundle(root, version)
            main, helper = builder.runtime_names("macos")
            launcher = builder.launcher_name("macos", version)
            for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"fictional fixture legal metadata")
            (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
            source = {"sha256": "1" * 64, "files": 4}
            pair = {"schema": 1, "version": version, "os": "macos", "arch": "aarch64",
                    "airk_sha256": package.sha256(root / main), "embed_sha256": package.sha256(root / helper)}
            (root / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
            (root / "airk-build.json").write_text(json.dumps({"schema": 1, "source": source, "runtime": pair}), encoding="utf-8")
            install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
            def seal():
                install["files"] = {name: value for name, value in package.inventory(root).items() if name != "airk-install.json"}
                (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
            seal()
            # These original format fixtures isolate launcher inventory/modes;
            # they do not qualify inference assets or actual signatures.
            with patch.object(package, "runtime_builder", return_value=builder), patch.object(builder, "inference_assets", return_value={}):
                files, executables = package.validate_runtime(root, version, "aarch64-apple-darwin", source)
                self.assertEqual(executables, (main, helper, launcher))
                archive = root / "candidate.zip"
                package.write_archive(root, archive, files, executables)
                with zipfile.ZipFile(archive) as zipped:
                    self.assertEqual((zipped.getinfo(launcher).external_attr >> 16) & 0o777, 0o755)
                archive.unlink()
                (root / launcher).unlink()
                seal()
                with self.assertRaisesRegex(ValueError, "launcher image is missing"):
                    package.validate_runtime(root, version, "aarch64-apple-darwin", source)

    def test_signed_packaging_reuses_ci_metadata_without_invoking_rust(self):
        # Reviewed pairs: a pre-split version reuses /2 metadata, a split version only /3.
        for checked_in, schema, other in (("1.8.23", "airk-native-candidate/2", "airk-native-candidate/3"),
                                          ("1.8.24", "airk-native-candidate/3", "airk-native-candidate/2")):
            with self.subTest(version=checked_in):
                self.check_signed_packaging_metadata(checked_in, schema, other)

    def check_signed_packaging_metadata(self, checked_in, schema, other):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("Cargo.lock", "rust-toolchain.toml"):
                (root / name).write_bytes((package.ROOT / name).read_bytes())
            (root / "Cargo.toml").write_text(f'[workspace.package]\nversion = "{checked_in}"\n', encoding="utf-8")
            target = "x86_64-pc-windows-msvc"
            receipt = {"schema": schema, "target": target,
                       "version": checked_in, "tree": "a" * 40, "dirty_checkout": False,
                       "signed_distribution": False, "rustc": "rustc fixture\nhost: " + target,
                       "cargo": "cargo fixture", "cargo_lock_sha256": package.sha256(root / "Cargo.lock"),
                       "toolchain_file_sha256": package.sha256(root / "rust-toolchain.toml")}
            path = root / "manifest.json"
            def git_only(*command):
                self.assertEqual(command, ("git", "rev-parse", "HEAD^{tree}"))
                return "a" * 40
            with patch.object(package, "ROOT", root), patch.object(package, "capture", side_effect=git_only):
                for mutation in (None, "version", "target", "tree", "cargo_lock_sha256", "rustc", "schema"):
                    bad = {**receipt, **({mutation: other if mutation == "schema" else "wrong"} if mutation else {})}
                    path.write_text(json.dumps(bad), encoding="utf-8")
                    if mutation:
                        with self.assertRaises(ValueError):
                            package.source_packaging_metadata(path, target, True)
                    else:
                        self.assertEqual(package.source_packaging_metadata(path, target, True), receipt)
                        with self.assertRaises(ValueError):
                            package.source_packaging_metadata(path, target, False)

    def test_mac_bundle_archive_has_two_executables_and_rejects_resealed_foreign_resources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            builder = package.runtime_builder()
            main, helper = builder.runtime_names("macos")
            for name in ("airk", "airk-embed"):
                (root / name).write_bytes(name.encode())
            builder.arrange_macos_bundle(root, "1.4.0")
            for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"original fixture legal metadata")
            (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
            source = {"sha256": "1" * 64, "files": 4}
            pair = {"schema": 1, "version": "1.4.0", "os": "macos", "arch": "aarch64",
                    "airk_sha256": package.sha256(root / main), "embed_sha256": package.sha256(root / helper)}
            (root / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
            (root / "airk-build.json").write_text(json.dumps({"schema": 1, "source": source, "runtime": pair}), encoding="utf-8")
            install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
            install["files"] = package.inventory(root)
            (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
            files, executable = package.validate_runtime(root, "1.4.0", "aarch64-apple-darwin", source)
            self.assertEqual(set(executable), {main, helper})
            archive = root / "fixture.zip"
            package.write_archive(root, archive, files, executable)
            with zipfile.ZipFile(archive) as zipped:
                for name in executable:
                    self.assertEqual((zipped.getinfo(name).external_attr >> 16) & 0o777, 0o755)
            archive.unlink()
            info = root / "AIRK.app/Contents/Info.plist"
            info.write_bytes(info.read_bytes().replace(b"<string>airk</string>", b"<string>foreign</string>"))
            install["files"][info.relative_to(root).as_posix()] = package.sha256(info)
            (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "bundle resources differ"):
                package.validate_runtime(root, "1.4.0", "aarch64-apple-darwin", source)
    def test_payload_budget_precedes_hashing_and_hard_links_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "airk"
            original.write_bytes(b"oversize fixture")
            with patch.object(package, "MAX_PAYLOAD_BYTES", 8), patch.object(package, "sha256") as hashed:
                with self.assertRaisesRegex(ValueError, "installer budget"):
                    package.inventory(root)
                hashed.assert_not_called()
            alias = root / "alias"
            os.link(original, alias)
            with self.assertRaisesRegex(ValueError, "Aliased"):
                package.inventory(root)
            self.assertEqual(original.read_bytes(), b"oversize fixture")
            alias.unlink()
            with self.assertRaisesRegex(ValueError, "oversized"):
                package.bounded_json(original, 4)

    def test_pair_requires_exact_source_inventory_and_both_executable_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("airk", "airk-embed"):
                (root / name).write_bytes(name.encode())
            pair = {"schema": 1, "version": "1.4.0", "os": "linux", "arch": "x86_64",
                    "airk_sha256": package.sha256(root / "airk"), "embed_sha256": package.sha256(root / "airk-embed")}
            source = {"sha256": "1" * 64, "files": 4}
            for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json", "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(b"fixture legal/metadata")
            (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
            (root / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
            (root / "airk-build.json").write_text(json.dumps({"schema": 1, "source": source, "runtime": pair}), encoding="utf-8")
            install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
            install["files"] = package.inventory(root)
            (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
            files, executables = package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", source)
            archive = root.parent / (root.name + ".zip")
            try:
                package.write_archive(root, archive, files, executables)
                with zipfile.ZipFile(archive) as zipped:
                    for name in executables:
                        self.assertEqual((zipped.getinfo(name).external_attr >> 16) & 0o777, 0o755)
                with self.assertRaisesRegex(ValueError, "source/pair"):
                    package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", {"sha256": "2" * 64})
                (root / "airk-embed").write_bytes(b"modified")
                with self.assertRaisesRegex(ValueError, "inventory differs"):
                    package.validate_runtime(root, "1.4.0", "x86_64-unknown-linux-gnu", source)
            finally:
                archive.unlink(missing_ok=True)

    def test_allowlist_permissions_and_tamper_detection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / "stage"
            stage.mkdir()
            (stage / "airk").write_bytes(b"fictional executable fixture")
            (stage / "LICENSE").write_text("fixture license", encoding="utf-8")
            (root / "private.env").write_text("excluded fixture", encoding="utf-8")
            expected = package.inventory(stage)
            archive = root / "candidate.zip"
            package.write_archive(stage, archive, expected, "airk")
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(set(zipped.namelist()), {"airk", "LICENSE"})
                self.assertEqual((zipped.getinfo("airk").external_attr >> 16) & 0o777, 0o755)
            changed = dict(expected, airk="0" * 64)
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                package.verify_archive(archive, changed, "airk")
            with zipfile.ZipFile(archive, "a") as zipped:
                zipped.writestr("unexpected.txt", "must fail")
            with self.assertRaisesRegex(ValueError, "entries differ"):
                package.verify_archive(archive, expected, "airk")

    def test_missing_executable_bit_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "airk").write_bytes(b"fixture")
            archive = root / "bad.zip"
            with zipfile.ZipFile(archive, "w") as zipped:
                info = zipfile.ZipInfo("airk")
                info.external_attr = 0o100644 << 16
                zipped.writestr(info, b"fixture")
            with self.assertRaisesRegex(ValueError, "executable permission"):
                package.verify_archive(archive, {"airk": package.sha256(root / "airk")}, "airk")


SPLIT_VERSION = "1.8.24"
# Reviewed literal expectations (split_receipts.IMAGES is the code under test's roster).
SPLIT_IMAGES = {"x86_64-unknown-linux-gnu": ("airk", "airkctl"),
                "x86_64-pc-windows-msvc": ("airk.exe", "airkctl.exe"),
                "aarch64-apple-darwin": ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl")}
ADMIN_BLOCK = {"bundle_sha256": "4" * 64, "inventory_sha256": "3" * 64, "node": "v24.15.0", "npm": "11.6.2",
               "locks": {"packages/admin/package-lock.json": "9" * 64}}


def split_runtime(root, target, source, version=SPLIT_VERSION, admin=ADMIN_BLOCK, images=None):
    """A complete fictional split payload: both images, legal/static notices, schema-3 pair, schema-2 build."""
    builder = package.runtime_builder()
    os_name = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    arch = "aarch64" if target.startswith("aarch64-") else "x86_64"
    images = SPLIT_IMAGES[target] if images is None else images
    legal = ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json",
             "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json")
    for name in (*images, *legal):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fictional split fixture " + name.encode())
    (root / "notices/inventory.json").write_text("[]", encoding="utf-8")
    lock = json.loads((builder.ROOT / "scripts/static-onnx-runtime-lock.json").read_text())
    prefix = "notices/static-onnx-runtime/"
    source_names = ["notices/" + name + "/LICENSE" for name in ["onnxruntime", *[row["name"] for row in lock["dependencies"]]]]
    source_names.append("notices/onnxruntime/ThirdPartyNotices.txt")
    static_names = [builder.packaged_static_notice_path(name) for name in source_names]
    for name in static_names:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fictional static legal bytes")
    (root / prefix / "inventory.json").write_text(json.dumps({"schema": 2, "ort_version": lock["ort_version"], "target": target,
        "build_receipt_sha256": "e" * 64,
        "lock_sha256": builder.digest(builder.ROOT / "scripts/static-onnx-runtime-lock.json"),
        "sources": {name: {"path": src, "bytes": (root / name).stat().st_size, "sha256": package.sha256(root / name)}
                    for name, src in zip(static_names, source_names)},
        "files": {name: package.sha256(root / name) for name in static_names}}), encoding="utf-8")
    if os_name == "macos":
        for name, data in builder.macos_bundle_files(version).items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    digests = [package.sha256(root / name) if (root / name).is_file() else "0" * 64 for name in SPLIT_IMAGES[target]]
    pair = {"schema": 3, "version": version, "os": os_name, "arch": arch, "airk_sha256": digests[0],
            "airkctl_sha256": digests[1], "i1_protocol": 1, "i2_contract": 1,
            "admin_inventory_sha256": admin["inventory_sha256"]}
    imports = {"schema": 1, "command": ["inspect", "airk"], "stdout": "fixture imports\n", "external_onnxruntime_imports": False}
    build = {"schema": 2, "source": source, "profile": "release", "runtime": pair, "rustc": "rustc fixture\nhost: " + target,
             "static_ort": {"build_receipt_sha256": "6" * 64, "target": target, "static_link_closure_sha256": "7" * 64,
                            "static_link_supplement_sha256": None, "static_link_recovery_sha256": None,
                            "verified_inputs_sha256": "8" * 64},
             "static_imports": imports, "airkctl_static_imports": {**imports, "inference_symbols": False},
             "embedded_model_smoke": {"schema": 1}, "admin_assets": admin}
    seal_split(root, pair, build)
    return pair, build


def seal_split(root, pair, build):
    (root / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
    (root / "airk-build.json").write_text(json.dumps(build), encoding="utf-8")
    install = {"schema": 1, "version": pair["version"], "os": pair["os"], "arch": pair["arch"]}
    (root / "airk-install.json").unlink(missing_ok=True)
    install["files"] = package.inventory(root)
    (root / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")


class SplitPairContracts(unittest.TestCase):
    source = {"sha256": "1" * 64, "files": 4}

    def test_trained_model_notices_require_exact_authenticated_five_file_closure(self):
        # Independent roster and literal rights pins: no expected set derived from the producer.
        companions = {
            "minilm-v2-LICENSE.txt": ("LICENSE", "9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd"),
            "minilm-v2-TRAINING-NOTICES.md": ("TRAINING-NOTICES.md", "2daba18214e777ecb68e1154c26eb9d50c9f8ef25895e4dcc8af3d9d1275e4be"),
            "minilm-v2-provenance.json": ("training-provenance.json", "6f6a29bff30ef64212b0c6fc56095d94df3184856e488023820d995fbb68da81"),
            "CC-BY-4.0.txt": ("CC-BY-4.0.txt", "9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411"),
        }
        for target in SPLIT_IMAGES:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                pair, build = split_runtime(root, target, self.source, version="1.9.0")
                notice_root = root / "notices/all-MiniLM-L6-v2"
                for source_name, (name, digest) in companions.items():
                    content = (package.ROOT / "third_party/models" / source_name).read_bytes()
                    self.assertEqual(hashlib.sha256(content).hexdigest(), digest)
                    (notice_root / name).write_bytes(content)
                (notice_root / "provenance.json").write_bytes(
                    (package.ROOT / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
                seal_split(root, pair, build)
                files, _ = package.validate_runtime(root, "1.9.0", target, self.source)
                self.assertEqual({name.removeprefix("notices/all-MiniLM-L6-v2/") for name in files
                                  if name.startswith("notices/all-MiniLM-L6-v2/")},
                                 {"LICENSE", "TRAINING-NOTICES.md", "training-provenance.json",
                                  "CC-BY-4.0.txt", "provenance.json"})
                # Reseal the outer inventory to prove it cannot authorize changed legal bytes.
                for name in ("LICENSE", "TRAINING-NOTICES.md", "training-provenance.json",
                             "CC-BY-4.0.txt", "provenance.json"):
                    path = notice_root / name
                    original = path.read_bytes()
                    for replacement in (None, original + b"tampered"):
                        if replacement is None:
                            path.unlink()
                        else:
                            path.write_bytes(replacement)
                        seal_split(root, pair, build)
                        with self.assertRaisesRegex(ValueError, "Pinned trained-model notice"):
                            package.validate_runtime(root, "1.9.0", target, self.source)
                        path.write_bytes(original)
                (notice_root / "unreviewed.txt").write_bytes(b"extra")
                seal_split(root, pair, build)
                with self.assertRaisesRegex(ValueError, "outside native/legal allowlist"):
                    package.validate_runtime(root, "1.9.0", target, self.source)

    def test_split_pair_validates_both_images_by_name_on_every_platform(self):
        for target, images in SPLIT_IMAGES.items():
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                split_runtime(root, target, self.source)
                files, executables = package.validate_runtime(root, SPLIT_VERSION, target, self.source)
                self.assertEqual(executables, images)
                self.assertTrue(set(images) <= set(files))
                archive = root.parent / (root.name + ".zip")
                try:
                    package.write_archive(root, archive, files, executables)
                    with zipfile.ZipFile(archive) as zipped:
                        for name in images:
                            self.assertEqual((zipped.getinfo(name).external_attr >> 16) & 0o777, 0o755)
                finally:
                    archive.unlink(missing_ok=True)

    def test_split_pair_refuses_a_missing_image_including_airkctl_without_airk(self):
        target = "x86_64-pc-windows-msvc"
        for present, missing in ((("airkctl.exe",), "airk.exe"), (("airk.exe",), "airkctl.exe")):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                split_runtime(root, target, self.source, images=present)
                with self.assertRaisesRegex(ValueError, "executable missing: " + missing.replace(".", r"\.")):
                    package.validate_runtime(root, SPLIT_VERSION, target, self.source)

    def test_split_and_pre_split_receipts_never_qualify_each_other(self):
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pair, build = split_runtime(root, target, self.source)
            # A split payload at a pre-split version is refused at its pair schema.
            with self.assertRaisesRegex(ValueError, "platform/version differs"):
                package.validate_runtime(root, "1.8.23", target, self.source)
            # A pre-split (schema 2 pair, schema 1 build) receipt at a split version is refused.
            single = {"schema": 2, "version": SPLIT_VERSION, "os": "linux", "arch": "x86_64", "airk_sha256": pair["airk_sha256"]}
            seal_split(root, single, {"schema": 1, "source": self.source, "runtime": single})
            with self.assertRaisesRegex(ValueError, "platform/version differs"):
                package.validate_runtime(root, SPLIT_VERSION, target, self.source)
            seal_split(root, pair, {**build, "schema": 1})
            with self.assertRaisesRegex(ValueError, "source/pair"):
                package.validate_runtime(root, SPLIT_VERSION, target, self.source)

    def test_split_pair_refuses_mismatched_admin_binding_and_image_digests(self):
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pair, build = split_runtime(root, target, self.source)
            other = {**build, "admin_assets": {**build["admin_assets"], "inventory_sha256": "5" * 64}}
            seal_split(root, pair, other)
            with self.assertRaisesRegex(ValueError, "split receipt contract: inventory_mismatch"):
                package.validate_runtime(root, SPLIT_VERSION, target, self.source)
            swapped = {**pair, "airk_sha256": pair["airkctl_sha256"], "airkctl_sha256": pair["airk_sha256"]}
            seal_split(root, swapped, {**build, "runtime": swapped})
            with self.assertRaisesRegex(ValueError, "executable digests"):
                package.validate_runtime(root, SPLIT_VERSION, target, self.source)

    def test_split_candidate_binds_both_images_admin_and_interfaces_without_vsix_target(self):
        target = "x86_64-pc-windows-msvc"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pair, build = split_runtime(root, target, self.source)
            files, executables = package.validate_runtime(root, SPLIT_VERSION, target, self.source)
            common = {"version": SPLIT_VERSION, "target": target, "commit": "c" * 40, "tree": "d" * 40,
                      "dirty_checkout": False, "source": self.source, "cargo_lock_sha256": "6" * 64,
                      "toolchain_file_sha256": "7" * 64, "rustc": "rustc fixture\nhost: " + target, "cargo": "cargo fixture",
                      "archive": f"airk-{SPLIT_VERSION}-{target}.zip", "archive_sha256": "8" * 64,
                      "install_manifest_sha256": files["airk-install.json"], "payload": files,
                      "bundled_model": {"manifest": "fixture.json"}, "embedded_models": {}, "signed_distribution": False,
                      "signing": None}
            receipt = package.split_candidate(common, files, executables, root, {"schema": 1, "profile": "fixture"})
            self.assertEqual(receipt["schema"], "airk-native-candidate/3")
            self.assertNotIn("vsix_target", receipt)
            self.assertEqual((receipt["airk_sha256"], receipt["airkctl_sha256"]), (files["airk.exe"], files["airkctl.exe"]))
            self.assertEqual(receipt["admin_assets"], {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64})
            self.assertEqual((receipt["i1_protocol"], receipt["i2_contract"]), (1, 1))
            with self.assertRaises(ValueError):
                package.split_candidate({**common, "archive": "airk-other.zip"}, files, executables, root, {"schema": 1})


if __name__ == "__main__":
    unittest.main()
