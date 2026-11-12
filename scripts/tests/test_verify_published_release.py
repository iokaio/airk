# SPDX-License-Identifier: Apache-2.0
"""Consumer verification of published split releases; fictional bytes, nothing is executed."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("verify_published_release", ROOT / "scripts/verify-published-release.py")
published = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(published)
VERSION = "1.8.24"
COMMIT = "c0" * 20
TREE = "7e" * 20
# Reviewed split plan §9.5 placements; checked in, not derived from the verifier.
CASES = {"linux-x64": ("x86_64-unknown-linux-gnu", {"airk": "service/airk", "airkctl": "bin/airkctl"}, "tar.gz"),
         "win32-x64": ("x86_64-pc-windows-msvc", {"airk.exe": "service/airk.exe", "airkctl.exe": "bin/airkctl.exe"}, "exe")}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def base(name):
    return json.loads((ROOT / "fixtures/split/receipts" / f"{name}.json").read_text(encoding="utf-8"))["base"]


def write_zip(path, entries):
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in entries.items():
            archive.writestr(name, data)


def release_set(root, platform, channel="stable", vsix_extra=None, member_change=None):
    """A downloaded split release for one platform, built from the checked-in C7 receipt bases."""
    target, placements, extension = CASES[platform]
    pair = {name: b"fictional " + name.encode() for name in placements}
    pair.update({"LICENSE": b"license", "NOTICE": b"notice", "THIRD_PARTY_NOTICES.md": b"third party",
                 "airk-runtime.json": b"{}", "airk-build.json": b"{}"})
    pair["airk-install.json"] = json.dumps({"schema": 1, "files": {name: sha(data) for name, data in pair.items()}}).encode()
    native = root / f"airk-{VERSION}-{target}.zip"
    write_zip(native, pair)
    images = list(placements)
    candidate = {**base("native_candidate_3"), "version": VERSION, "target": target, "commit": COMMIT, "tree": TREE,
                 "archive": native.name, "archive_sha256": sha(native.read_bytes()),
                 "install_manifest_sha256": sha(pair["airk-install.json"]),
                 "payload": {name: sha(data) for name, data in pair.items()},
                 "airk_sha256": sha(pair[images[0]]), "airkctl_sha256": sha(pair[images[1]]),
                 "rustc": "rustc fixture\nhost: " + target, "signed_distribution": False, "signing": None}
    (root / f"airk-{VERSION}-{target}.native-manifest.json").write_text(json.dumps(candidate))
    host = {"package.json": b"{}", "out/extension.js": b"// host", "README.md": b"readme", "LICENSE": b"license"}
    stored = {"extension/" + ("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): data
              for name, data in host.items()}
    stored.update(vsix_extra or {})
    vsix = root / f"airk-{VERSION}.vsix"
    write_zip(vsix, stored)
    receipt = {**base("vsix_universal"), "version": VERSION, "commit": COMMIT, "tree": TREE, "archive": vsix.name,
               "archive_sha256": sha(vsix.read_bytes()), "payload": {name: sha(data) for name, data in host.items()}}
    (root / f"airk-{VERSION}.vsix-manifest.json").write_text(json.dumps(receipt))
    if channel == "stable":
        layout = {"payload/" + placements[name]: pair[name] for name in placements}
        layout.update({"payload/notices/" + name: pair[name] for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md")})
        layout["payload/payload-inventory.json"] = b'{"schema": "airk-service-payload/1"}\n'
        installer = root / f"airk-{VERSION}-{target}-installer.{extension}"
        if extension == "tar.gz":
            layout.update({"install.sh": b"#!/bin/sh\n", "README.txt": b"readme", "payload.sha256": b"roster"})
            members = {**layout, **(member_change or {})}
            with tarfile.open(installer, "w:gz") as archive:
                for name, data in members.items():
                    info = tarfile.TarInfo(installer.name.removesuffix(".tar.gz") + "/" + name)
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
        else:
            installer.write_bytes(b"MZ fictional signed installer")
        manifest = {"schema": "airk-standalone-installer/1", "version": VERSION, "target": target, "commit": COMMIT,
                    "runtime_install_sha256": candidate["install_manifest_sha256"], "runtime_source": candidate["source"],
                    "artifact": installer.name, "artifact_sha256": sha(installer.read_bytes()),
                    "payload_schema": "airk-service-payload/1",
                    "payload_inventory_sha256": sha(layout["payload/payload-inventory.json"]),
                    "payload": {name: sha(data) for name, data in layout.items()},
                    "signing": {"platform": "Linux", "trust": "unsigned"} if extension == "tar.gz" else {"platform": "Windows"}}
        (root / f"airk-{VERSION}-{target}-installer.manifest.json").write_text(json.dumps(manifest))
    assets = {path.name: sha(path.read_bytes()) for path in root.iterdir()}
    record = {"schema": "airk-github-release/1", "tag": "v" + VERSION, "package_version": VERSION, "commit": COMMIT,
              "tree": TREE, "channel": channel, "run_id": "456", "attempt": "1", "assets": assets}
    (root / "release.json").write_text(json.dumps(record))
    sums = {**assets, "release.json": sha((root / "release.json").read_bytes())}
    (root / "SHA256SUMS").write_text("".join(f"{value}  {name}\n" for name, value in sorted(sums.items())))
    return root


class PublishedSplitRelease(unittest.TestCase):
    def test_complete_split_releases_pass_for_each_channel_and_platform(self):
        for platform in CASES:
            for channel in ("stable", "preview"):
                with self.subTest(platform=platform, channel=channel), tempfile.TemporaryDirectory() as temporary:
                    result = published.verify(release_set(Path(temporary), platform, channel), VERSION, platform, COMMIT)
                    self.assertEqual(result["failed"], [])
                    self.assertIn("the VSIX contains no native code", result["passed"])
                    self.assertIn("both images (airk and airkctl) match the native manifest", result["passed"])
                    self.assertEqual(result["installer_sha256"] is not None, channel == "stable")
                    if channel == "stable":
                        self.assertIn("installer places both tested images", result["passed"])
                    windows_note = any("Windows/macOS installer" in item for item in result["not_verified"])
                    self.assertEqual(windows_note, channel == "stable" and platform == "win32-x64")

    def test_native_vsix_changed_installer_member_and_missing_installer_fail(self):
        for name, build, expected in (
                ("native VSIX", dict(vsix_extra={"extension/native/airk.exe": b"MZ"}), "the VSIX contains no native code"),
                ("tar member", dict(member_change={"payload/bin/airkctl": b"replaced"}),
                 "installer archive files match the installer manifest"),
                ("extra member", dict(member_change={"payload/extra": b"unlisted"}),
                 "installer archive files match the installer manifest")):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                result = published.verify(release_set(Path(temporary), "linux-x64", **build), VERSION, "linux-x64")
                self.assertTrue(any(expected in failure for failure in result["failed"]), result["failed"])
        with tempfile.TemporaryDirectory() as temporary:
            root = release_set(Path(temporary), "win32-x64")
            record = json.loads((root / "release.json").read_text())
            del record["assets"]["airk-1.8.24-x86_64-pc-windows-msvc-installer.exe"]
            (root / "release.json").write_text(json.dumps(record))
            result = published.verify(root, VERSION, "win32-x64")
            self.assertIn("the release publishes an installer for this platform exactly when its channel requires one",
                          result["failed"])

    def test_installer_from_another_pair_or_without_its_inventory_fails(self):
        for change, expected in ((("runtime_install_sha256", "0" * 64), "installer was packaged from this release's service pair"),
                                 (("payload_schema", None), "installer pins its service payload inventory")):
            with self.subTest(field=change[0]), tempfile.TemporaryDirectory() as temporary:
                root = release_set(Path(temporary), "win32-x64")
                path = root / "airk-1.8.24-x86_64-pc-windows-msvc-installer.manifest.json"
                manifest = json.loads(path.read_text())
                manifest[change[0]] = change[1]
                path.write_text(json.dumps(manifest))
                result = published.verify(root, VERSION, "win32-x64")
                self.assertIn(expected, result["failed"])
                self.assertIn(f"{path.name} matches release.json", result["failed"])

    def test_pair_image_drift_from_its_manifest_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = release_set(Path(temporary), "linux-x64", "preview")
            path = root / "airk-1.8.24-x86_64-unknown-linux-gnu.native-manifest.json"
            manifest = json.loads(path.read_text())
            manifest["airkctl_sha256"] = "1" * 64
            path.write_text(json.dumps(manifest))
            result = published.verify(root, VERSION, "linux-x64")
            self.assertIn("both images (airk and airkctl) match the native manifest", result["failed"])


if __name__ == "__main__":
    unittest.main()
