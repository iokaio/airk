# SPDX-License-Identifier: Apache-2.0
"""Archive/installer contracts; fictional bytes do not qualify Linux execution."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("package_standalone_linux", Path(__file__).parents[1] / "package-standalone-linux.py")
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


def fixture(base):
    """Use the real native legal/inventory validator with tiny fictional data."""
    runtime, publish = base / "runtime", base / "native/publish"
    runtime.mkdir(); publish.mkdir(parents=True)
    builder = package.native.runtime_builder()
    version = package.tomllib.loads((package.ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    source = {"sha256": "1" * 64, "files": 1}
    for name in ("airk", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json",
                 "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
        path = runtime / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fictional native/legal fixture\n")
    (runtime / "notices/inventory.json").write_text("[]", encoding="utf-8")
    lock = json.loads((package.ROOT / "scripts/static-onnx-runtime-lock.json").read_text(encoding="utf-8"))
    sources = ["notices/" + name + "/LICENSE" for name in ["onnxruntime", *[row["name"] for row in lock["dependencies"]]]]
    sources.append("notices/onnxruntime/ThirdPartyNotices.txt")
    notice_names = [builder.packaged_static_notice_path(name) for name in sources]
    for name in notice_names:
        path = runtime / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fictional static dependency license\n")
    metadata = {"schema": 2, "ort_version": "1.22.1", "target": package.TARGET,
                "build_receipt_sha256": "e" * 64,
                "lock_sha256": builder.digest(package.ROOT / "scripts/static-onnx-runtime-lock.json"),
                "sources": {name: {"path": source_name, "bytes": (runtime / name).stat().st_size,
                                   "sha256": package.native.sha256(runtime / name)} for name, source_name in zip(notice_names, sources)},
                "files": {name: package.native.sha256(runtime / name) for name in notice_names}}
    (runtime / "notices/static-onnx-runtime/inventory.json").write_text(json.dumps(metadata), encoding="utf-8")
    pair = {"schema": 2, "version": version, "os": "linux", "arch": "x86_64", "airk_sha256": package.native.sha256(runtime / "airk")}
    (runtime / "airk-runtime.json").write_text(json.dumps(pair), encoding="utf-8")
    (runtime / "airk-build.json").write_text(json.dumps({"schema": 1, "source": source, "runtime": pair}), encoding="utf-8")
    install = {"schema": 1, "version": version, "os": "linux", "arch": "x86_64", "files": package.native.inventory(runtime)}
    (runtime / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
    payload, executables = package.native.validate_runtime(runtime, version, package.TARGET, source)
    archive = publish / f"airk-{version}-{package.TARGET}.zip"
    package.native.write_archive(runtime, archive, payload, executables)
    receipt = {"schema": "airk-native-candidate/2", "version": version, "target": package.TARGET,
               "commit": "c" * 40, "tree": "d" * 40, "source": source, "dirty_checkout": False,
               "signed_distribution": False, "signing": None,
               "archive": archive.name, "archive_sha256": package.native.sha256(archive), "payload": payload,
               "install_manifest_sha256": payload["airk-install.json"],
               "cargo_lock_sha256": package.native.sha256(package.ROOT / "Cargo.lock"),
               "toolchain_file_sha256": package.native.sha256(package.ROOT / "rust-toolchain.toml")}
    seal_receipt(publish, receipt)
    return publish, receipt, builder


def seal_receipt(publish, receipt):
    manifest = publish / "manifest.json"
    manifest.write_text(json.dumps(receipt), encoding="utf-8")
    (publish / "SHA256SUMS").write_text(f"{package.native.sha256(publish / receipt['archive'])}  {receipt['archive']}\n"
        f"{package.native.sha256(manifest)}  manifest.json\n", encoding="utf-8", newline="\n")


class LinuxInstallerContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=package.ROOT / "artifacts", prefix="linux-installer-contract-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)

    def environment(self, builder, receipt):
        return (patch.object(package.native, "runtime_builder", return_value=builder),
                patch.object(builder, "source_identity", return_value=receipt["source"]),
                patch.object(package.native, "capture", side_effect=lambda *args: "c" * 40 if args[-1] == "HEAD" else "d" * 40))

    def test_offline_archive_preserves_exact_native_payload_legal_and_unsigned_identity(self):
        native, original, builder = fixture(self.base)
        first = self.base / "first"
        a, b, c = self.environment(builder, original)
        with a, b, c:
            receipt = package.package_candidate(native, first)
            other = package.package_candidate(native, self.base / "second")
        self.assertEqual(receipt["artifact_sha256"], other["artifact_sha256"], "identical inputs produce stable installer bytes")
        self.assertEqual(receipt["installer_kind"], "linux_tar")
        self.assertEqual(receipt["code_signing"], "unsigned")
        self.assertEqual(receipt["signing"], {"platform": "Linux", "trust": "unsigned"})
        self.assertIs(receipt["editor_extension_included"], False)
        self.assertEqual(receipt["runtime_source"], original["source"])
        self.assertEqual(receipt["runtime_install_sha256"], original["payload"]["airk-install.json"])
        self.assertEqual({name.removeprefix("runtime/"): digest for name, digest in receipt["payload"].items() if name.startswith("runtime/")}, original["payload"])
        artifact = first / "publish" / receipt["artifact"]
        package.verify_installer(artifact, receipt["payload"], receipt["artifact"].removesuffix(".tar.gz"))
        with tarfile.open(artifact, "r:gz") as archive:
            prefix = receipt["artifact"].removesuffix(".tar.gz") + "/"
            shell = archive.extractfile(prefix + "install.sh").read().decode()
            readme = archive.extractfile(prefix + "README.txt").read().decode()
        self.assertIn("setup --runtime-source", shell)
        self.assertIn("--approved-sha256 '" + receipt["runtime_install_sha256"] + "'", shell)
        self.assertIn("sha256sum --check --strict --status payload.sha256", shell)
        self.assertIn('"$@"', shell)
        self.assertNotIn("curl ", shell)
        self.assertNotIn("sudo ", shell)
        self.assertIn("unsigned", readme)
        self.assertIn("gh attestation verify", readme)
        self.assertIn("--signer-workflow iokaio/airk/.github/workflows/github-release.yml", readme)
        self.assertIn("No Python, jq", readme)
        self.assertNotIn("@VERSION@", shell)

    def test_wrong_platform_signature_source_and_manifest_digests_fail_before_output(self):
        native, receipt, builder = fixture(self.base)
        for field, value in (("target", "aarch64-apple-darwin"), ("signed_distribution", True),
                             ("source", {"sha256": "f" * 64, "files": 1}), ("archive_sha256", "a" * 64)):
            with self.subTest(field=field):
                changed = {**receipt, field: value}
                seal_receipt(native, changed)
                a, b, c = self.environment(builder, receipt)
                with a, b, c, self.assertRaises(ValueError):
                    package.package_candidate(native, self.base / ("bad-" + field))
                self.assertFalse((self.base / ("bad-" + field)).exists())
        seal_receipt(native, receipt)

    def test_candidate_unknown_file_and_hard_link_are_refused(self):
        native, receipt, builder = fixture(self.base)
        extra = native / "extra.vsix"
        extra.write_bytes(b"not part of a standalone native package")
        with self.assertRaisesRegex(ValueError, "Unexpected native candidate"):
            package.candidate(native)
        extra.unlink()
        alias = self.base / "external-link"
        os.link(native / receipt["archive"], alias)
        try:
            with self.assertRaises(ValueError):
                package.candidate(native)
        finally:
            alias.unlink()

    def test_archive_verifier_rejects_traversal_links_duplicates_modes_and_changed_bytes(self):
        expected = {"install.sh": hashlib.sha256(b"one").hexdigest()}
        for kind in ("traversal", "link", "duplicate", "mode", "bytes", "missing"):
            archive = self.base / (kind + ".tar.gz")
            with tarfile.open(archive, "w:gz") as output:
                if kind == "missing":
                    continue
                name = "outside/../install.sh" if kind == "traversal" else "package/install.sh"
                member = tarfile.TarInfo(name); member.mode = 0o644 if kind == "mode" else 0o755
                data = b"two" if kind == "bytes" else b"one"; member.size = len(data)
                if kind == "link":
                    member.type = tarfile.SYMTYPE; member.linkname = "/foreign"; member.size = 0
                output.addfile(member, io.BytesIO(data) if member.isfile() else None)
                if kind == "duplicate":
                    output.addfile(member, io.BytesIO(data))
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                package.verify_installer(archive, expected, "package")

    def test_archive_bounds_are_checked_before_member_payload_reads(self):
        archive = self.base / "large.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            member = tarfile.TarInfo("package/install.sh"); member.mode = 0o755; member.size = 9
            output.addfile(member, io.BytesIO(b"123456789"))
        with patch.object(package, "MAX_INSTALLER_BYTES", 8), self.assertRaisesRegex(ValueError, "oversized"):
            package.verify_installer(archive, {"install.sh": hashlib.sha256(b"123456789").hexdigest()}, "package")

    def test_output_and_native_paths_remain_bounded_and_relative(self):
        for name in ("../foreign", "/absolute", "runtime/../airk", "runtime//airk", "runtime\\airk", "runtime/space name"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                package.safe_name(name)
        with self.assertRaises(ValueError):
            package.render_template("install.sh.in", {})


if __name__ == "__main__":
    unittest.main()
