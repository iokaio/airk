# SPDX-License-Identifier: Apache-2.0
import importlib.util
import copy
import io
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import tarfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("github_release", Path(__file__).parents[1] / "github-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
COMMIT = "a" * 40
VERSION = "0.1.0"
TAG = "v0.1.0"
SUFFIX = COMMIT + "-123-1"


def write_receipt(directory, receipt):
    path = directory / "manifest.json"
    path.write_text(json.dumps(receipt), encoding="utf-8")
    (directory / "SHA256SUMS").write_text(
        f'{release.digest(directory / receipt["archive"])}  {receipt["archive"]}\n'
        f'{release.digest(path)}  manifest.json\n', encoding="utf-8")


def native_fixture(directory, target, signed=False):
    directory.mkdir(parents=True)
    executable = "airk.exe" if "windows" in target else "airk"
    files = {executable: b"signed fixture" if signed else b"original fixture",
             "LICENSE": b"fixture license", "NOTICE": b"fixture notice", "THIRD_PARTY_NOTICES.md": b"fixture"}
    payload = {name: release.hashlib.sha256(data).hexdigest() for name, data in files.items()}
    archive = directory / f"airk-{VERSION}-{target}.zip"
    with zipfile.ZipFile(archive, "w") as zipped:
        for name, data in files.items():
            info = zipfile.ZipInfo(name)
            info.external_attr = (0o100755 if name == executable else 0o100644) << 16
            zipped.writestr(info, data)
    signing = None
    if signed:
        signing = {"schema": "airk-native-signing/1", "target": target, "commit": COMMIT,
                   "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                   "source_sha256": release.hashlib.sha256(b"original fixture").hexdigest(),
                   "binary_sha256": payload[executable], "identity": "c" * 40}
        if "windows" in target:
            signing.update(platform="Windows", azure=release.signer.AZURE_SIGNING,
                           verification={"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "c" * 40})
        else:
            signing.update(platform="Darwin", team_id="5PX7D99PXA",
                           verification={"trust": "apple_developer_id", "team_id": "5PX7D99PXA",
                                         "timestamp_present": True, "hardened_runtime": True},
                           notarization={"status": "Accepted", "id": "fixture-notary-id"})
    receipt = {"schema": "airk-native-candidate/1", "target": target, "vsix_target": release.native.TARGETS[target],
               "version": VERSION, "commit": COMMIT, "dirty_checkout": False,
               "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
               "toolchain_file_sha256": release.digest(release.ROOT / "rust-toolchain.toml"),
               "signed_distribution": signed, "signing": signing, "archive": archive.name,
               "archive_sha256": release.digest(archive), "payload": payload}
    write_receipt(directory, receipt)
    return files, receipt


def fixtures(directory, channel="preview"):
    for target, platform in release.native.TARGETS.items():
        signed = target in release.SIGNED_TARGETS
        prefix = "airk-signed" if signed else "airk"
        native_dir = directory / f"{prefix}-native-{target}-{SUFFIX}"
        files, receipt = native_fixture(native_dir, target, signed)
        if signed:
            native_fixture(directory / f"airk-native-{target}-{SUFFIX}", target)
        vsix_dir = directory / f"{prefix}-vsix-{target}-{SUFFIX}"
        vsix_dir.mkdir()
        executable = "airk.exe" if "windows" in target else "airk"
        payload = {"package.json": b"{}", "README.md": b"fixture", "LICENSE": b"fixture",
                   "native/manifest.json": (native_dir / "manifest.json").read_bytes(),
                   **{"native/" + name: data for name, data in files.items()}}
        archive = vsix_dir / f"airk-{VERSION}-{platform}.vsix"
        with zipfile.ZipFile(archive, "w") as zipped:
            for name, data in payload.items():
                normalized = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
                info = zipfile.ZipInfo("extension/" + normalized)
                info.external_attr = (0o100755 if name == "native/" + executable else 0o100644) << 16
                zipped.writestr(info, data)
            zipped.writestr("[Content_Types].xml", "<Types/>")
            flag = '<Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true"/>' if channel == "preview" else ""
            zipped.writestr("extension.vsixmanifest", f'<PackageManifest><Identity Id="airk" Publisher="iokaio" Version="{VERSION}" TargetPlatform="{platform}"/><DisplayName>AIRK</DisplayName>{flag}</PackageManifest>')
        vsix = {"schema": "airk-extension-candidate/1", "commit": COMMIT, "version": VERSION, "platform": platform,
                "extension_id": "iokaio.airk", "display_name": "AIRK", "pre_release": channel == "preview",
                "archive": archive.name, "archive_sha256": release.digest(archive),
                "native_manifest_sha256": release.digest(native_dir / "manifest.json"),
                "native_archive_sha256": receipt["archive_sha256"],
                "npm_lock_sha256": release.digest(release.ROOT / "packages/vscode/package-lock.json"),
                "payload": {name: release.hashlib.sha256(data).hexdigest() for name, data in payload.items()}}
        write_receipt(vsix_dir, vsix)


PAIR_VERSION = "1.4.4"


def pair_files(target, signed=False, version=PAIR_VERSION):
    """Fictional bytes in the real service layout; no native trust is asserted."""
    operating_system = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    builder = release.native.runtime_builder()
    executables = builder.runtime_executables(operating_system, version)
    files = {name: ("signed " if signed else "tested ").encode() + name.encode() for name in executables}
    for name in builder.signing_names(operating_system, version):
        files.setdefault(name, ("signed " if signed else "tested ").encode() + name.encode())
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "notices/bom.cdx.json",
                 "notices/all-MiniLM-L6-v2/LICENSE", "notices/all-MiniLM-L6-v2/provenance.json"):
        files[name] = b"fictional legal metadata"
    files["notices/inventory.json"] = b"[]"
    if builder.single_image_version(version):
        lock = json.loads((builder.ROOT / "scripts/static-onnx-runtime-lock.json").read_text())
        prefix = "notices/static-onnx-runtime/"
        originals = {"notices/" + name + "/LICENSE": b"fictional static dependency license" for name in ["onnxruntime", *[row["name"] for row in lock["dependencies"]]]}
        originals["notices/onnxruntime/ThirdPartyNotices.txt"] = b"fictional static notices"
        notices = {builder.packaged_static_notice_path(name): data for name, data in originals.items()}
        files.update(notices)
        files[prefix + "inventory.json"] = json.dumps({"schema": 2, "ort_version": lock["ort_version"], "target": target,
            "build_receipt_sha256": "e" * 64,
            "sources": {builder.packaged_static_notice_path(name): {"path": name, "bytes": len(data), "sha256": release.hashlib.sha256(data).hexdigest()} for name, data in originals.items()},
            "lock_sha256": builder.digest(builder.ROOT / "scripts/static-onnx-runtime-lock.json"),
            "files": {name: release.hashlib.sha256(data).hexdigest() for name, data in notices.items()}}).encode()
    if operating_system == "macos":
        files.update(builder.macos_bundle_files(version))
        files["AIRK.app/Contents/_CodeSignature/CodeResources"] = b"signed seal" if signed else b"ad-hoc seal"
        if signed:
            files["AIRK.app/Contents/CodeResources"] = b"fictional stapled ticket"
    pair = {"schema": 2 if builder.single_image_version(version) else 1, "version": version, "os": operating_system,
            "arch": "aarch64" if target.startswith("aarch64-") else "x86_64",
            "airk_sha256": release.hashlib.sha256(files[executables[0]]).hexdigest()}
    if not builder.single_image_version(version):
        pair["embed_sha256"] = release.hashlib.sha256(files[executables[1]]).hexdigest()
    files["airk-runtime.json"] = json.dumps(pair).encode()
    files["airk-build.json"] = json.dumps({"schema": 1, "runtime": pair,
        "source": {"sha256": "d" * 64, "files": 4}}).encode()
    seal_pair_files(files)
    return files, executables


def seal_pair_files(files):
    pair = json.loads(files["airk-runtime.json"])
    install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
    install["schema"] = 1
    install["files"] = {name: release.hashlib.sha256(data).hexdigest()
                        for name, data in files.items() if name != "airk-install.json"}
    files["airk-install.json"] = json.dumps(install).encode()


def save_pair_candidate(directory, files, receipt, executables):
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / receipt["archive"]
    with zipfile.ZipFile(archive, "w") as zipped:
        for name, data in files.items():
            info = zipfile.ZipInfo(name)
            info.external_attr = (0o100755 if name in executables or name.endswith("/airk-launcher-image") else 0o100644) << 16
            zipped.writestr(info, data)
    receipt.update(archive_sha256=release.digest(archive),
                   payload={name: release.hashlib.sha256(data).hexdigest() for name, data in files.items()},
                   install_manifest_sha256=release.hashlib.sha256(files["airk-install.json"]).hexdigest())
    write_receipt(directory, receipt)


def pair_fixture(directory, target, signed=False, version=PAIR_VERSION):
    files, executables = pair_files(target, signed, version)
    original, _ = pair_files(target, version=version)
    signing = None
    if signed:
        components = {}
        builder = release.native.runtime_builder()
        for name in builder.signing_names("windows" if "windows" in target else "macos", version):
            component = {"schema": "airk-native-signing/1", "target": target, "commit": COMMIT,
                         "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                         "source_sha256": release.hashlib.sha256(original[name]).hexdigest(),
                         "binary_sha256": release.hashlib.sha256(files[name]).hexdigest(), "identity": "c" * 40}
            if "windows" in target:
                component.update(platform="Windows", azure=release.signer.AZURE_SIGNING,
                    verification={"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "c" * 40})
            else:
                component.update(platform="Darwin", team_id="5PX7D99PXA",
                    verification={"trust": "apple_developer_id", "team_id": "5PX7D99PXA",
                                  "timestamp_present": True, "hardened_runtime": True},
                    notarization={"status": "Accepted", "id": "fictional-app-job", "stapled": True, "scope": "AIRK.app"})
            components[name] = component
        signing = {"schema": "airk-service-pair-signing/1", "target": target, "commit": COMMIT,
                   "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                   "source_install_sha256": release.hashlib.sha256(original["airk-install.json"]).hexdigest(),
                   "install_manifest_sha256": release.hashlib.sha256(files["airk-install.json"]).hexdigest(),
                   "components": components}
        if "apple" in target and builder.standalone_launcher_version(version):
            image = builder.launcher_name("macos", version)
            signing["copied_launcher"] = {"schema": 1, "binary_sha256": release.hashlib.sha256(files[image]).hexdigest(),
                                          "version": version, "strict_signature": True, "standalone_version_probe": True}
    receipt = {"schema": "airk-native-candidate/2", "target": target, "vsix_target": release.native.TARGETS[target],
               "version": version, "commit": COMMIT, "dirty_checkout": False,
               "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
               "toolchain_file_sha256": release.digest(release.ROOT / "rust-toolchain.toml"),
               "source": {"sha256": "d" * 64, "files": 4}, "signed_distribution": signed, "signing": signing,
               "archive": f"airk-{version}-{target}.zip"}
    if release.native.runtime_builder().int8_version(version):
        policy = release.native.runtime_builder().load_script("int8-policy")
        receipt["embedded_models"] = {"schema": 1, "fp32_included": False, "profiles": policy.catalog()["profiles"]}
    save_pair_candidate(directory, files, receipt, executables)
    return files, receipt, executables


def pair_vsix(directory, native_dir, files, native_receipt, executables):
    version = native_receipt["version"]
    directory.mkdir(parents=True)
    platform = native_receipt["vsix_target"]
    wrapper = {"schema": 2, "version": version, "vsix_target": platform, "runtime_dir": "runtime",
               "install_manifest_sha256": native_receipt["install_manifest_sha256"],
               "payload": {name: native_receipt["payload"][name] for name in executables}}
    files = {"package.json": b"{}", "README.md": b"fixture", "LICENSE": b"fixture",
             "native/manifest.json": json.dumps(wrapper).encode(),
             "native/candidate.json": (native_dir / "manifest.json").read_bytes(),
             **{"native/runtime/" + name: data for name, data in files.items()}}
    if tuple(map(int, version.split("."))) >= (1, 8, 12):
        files["package.json"] = json.dumps({"name": "airk", "publisher": "iokaio", "version": version}).encode()
        for name in release.extension.package.update_inventory_paths(native=True):
            files.setdefault(name, b"fictional extension update graph")
        update_files = {name: release.hashlib.sha256(files[name]).hexdigest()
                        for name in release.extension.package.update_inventory_paths(native=True)}
        files["airk-update.json"] = release.extension.package.encode_update_inventory(
            update_files, version, files["package.json"].decode())
    archive = directory / f"airk-{version}-{platform}.vsix"
    with zipfile.ZipFile(archive, "w") as zipped:
        for name, data in files.items():
            normalized = "readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name
            info = zipfile.ZipInfo("extension/" + normalized)
            info.external_attr = (0o100755 if name in {"native/runtime/" + entry for entry in executables} else 0o100644) << 16
            zipped.writestr(info, data)
        zipped.writestr("[Content_Types].xml", "<Types/>")
        zipped.writestr("extension.vsixmanifest", f'<PackageManifest><Identity Id="airk" Publisher="iokaio" Version="{version}" TargetPlatform="{platform}"/><DisplayName>AIRK</DisplayName></PackageManifest>')
    receipt = {"schema": "airk-extension-candidate/1", "commit": COMMIT, "version": version, "platform": platform,
               "extension_id": "iokaio.airk", "display_name": "AIRK", "pre_release": False,
               "archive": archive.name, "archive_sha256": release.digest(archive),
               "native_manifest_sha256": release.digest(native_dir / "manifest.json"),
               "native_archive_sha256": native_receipt["archive_sha256"],
               "npm_lock_sha256": release.digest(release.ROOT / "packages/vscode/package-lock.json"),
               "payload": {name: release.hashlib.sha256(data).hexdigest() for name, data in files.items()}}
    write_receipt(directory, receipt)


# Reviewed split plan §9.5 program layout and I4 signing claims (checked-in; not derived from the packagers).
SPLIT_LAYOUT_IMAGES = {"x86_64-pc-windows-msvc": {"airk.exe": "service/airk.exe", "airkctl.exe": "bin/airkctl.exe"},
                       "x86_64-unknown-linux-gnu": {"airk": "service/airk", "airkctl": "bin/airkctl"}}
SPLIT_EXECUTABLES = {"windows": {"airkctl": "bin/airkctl.exe", "airk": "service/airk.exe"},
                     "linux": {"airk": "service/airk", "airkctl": "bin/airkctl"},
                     "macos": {"airk": "AIRK.app/Contents/MacOS/airk", "airkctl": "AIRK.app/Contents/MacOS/airkctl"}}
SPLIT_SIGNING_CLAIMS = {"windows": {"platform": "windows", "publisher": "CN=Ioka LLC, O=Ioka LLC, L=Mapleton, S=Utah, C=US",
                                    "identity_eku": "1.3.6.1.4.1.311.97.88309284.513035131.587831003.613935669"},
                        "linux": {"platform": "linux", "signed": False, "integrity": "record-sha256"},
                        "macos": {"platform": "macos", "team_id": "5PX7D99PXA"}}


def split_installer_payload(files, target, version, artifact, admin=None, layout_change=None):
    """Fictional split installer contents: payload/ (the §9.5 layout and its inventory) plus wrappers."""
    os_name = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    images = SPLIT_LAYOUT_IMAGES.get(target, {})
    layout = {}
    for name, data in files.items():
        if name in ("airk-install.json", "airk-runtime.json", "airk-build.json"):
            continue
        layout[images.get(name, "notices/" + name if name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md") else name)] = data
    if os_name == "windows":
        layout["notices/inno-setup/LICENSE.txt"] = b"Fictional compiler license"
    sha = lambda data: release.hashlib.sha256(data).hexdigest()
    inventory = {"schema": "airk-service-payload/1", "version": version, "channel": "stable", "target": target,
                 "executables": SPLIT_EXECUTABLES[os_name],
                 "files": {name: {"sha256": sha(data), "bytes": len(data)} for name, data in sorted(layout.items())},
                 "admin_assets": admin or {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64},
                 "signing": SPLIT_SIGNING_CLAIMS[os_name]}
    if layout_change:
        layout.update(layout_change)
    layout["payload-inventory.json"] = (json.dumps(inventory, indent=1) + "\n").encode()
    payload = {"payload/" + name: data for name, data in layout.items()}
    if os_name == "windows":
        return payload
    sums = "".join(f"{sha(payload[path])}  {path}\n" for path in sorted(payload)).encode()
    payload["payload.sha256"] = sums
    if os_name == "macos":
        mac = release.module("test_split_installer_mac", "package-standalone-macos.py")
        payload.update(mac.split_installer_files(version, target, sha(layout["payload-inventory.json"]), sha(sums),
                                                 sha(layout["AIRK.app/Contents/MacOS/airk"]), len(layout)))
    else:
        linux = release.module("test_split_installer_linux", "package-standalone-linux.py")
        values = {"VERSION": version, "ARCHIVE_NAME": artifact, "DIRECTORY_NAME": artifact.removesuffix(".tar.gz"),
                  "PAYLOAD_FILE_COUNT": len(layout), "PAYLOAD_SUMS_SHA256": sha(sums),
                  "INVENTORY_SHA256": sha(layout["payload-inventory.json"]), "AIRK_SHA256": sha(layout["service/airk"])}
        payload.update({name: linux.render_template(name + ".in", values).encode() for name in ("install.sh", "README.txt")})
    return payload


class MainReleaseFixture(unittest.TestCase):
    def setUp(self):
        # These retained fixtures describe main producers. A manual campaign CI
        # runner must not silently change the context of historical assertions.
        environment = patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"})
        environment.start()
        self.addCleanup(environment.stop)


class ServicePairReleaseContracts(MainReleaseFixture):
    def standalone_fixture(self, directory, target, version="1.8.14", make_pair=None, split_options=None):
        files, runtime, _ = (make_pair or pair_fixture)(directory / "native", target, target in release.SIGNED_TARGETS, version)
        output = directory / "installer"
        output.mkdir()
        artifact = release.standalone.artifact(version, target)
        payload = {"runtime/" + name: data for name, data in files.items()}
        split = release.receipts.split_version(version)
        if split:
            payload = split_installer_payload(files, target, version, artifact, **(split_options or {}))
        elif "apple" in target:
            mac = release.module("test_installer_mac", "package-standalone-macos.py")
            payload.update(mac.installer_files(version, target, runtime["install_manifest_sha256"],
                                               runtime["payload"]["AIRK.app/Contents/MacOS/airk"]))
        elif "linux" in target:
            linux = release.module("test_installer_linux", "package-standalone-linux.py")
            sums = "".join(f"{value}  runtime/{path}\n" for path, value in sorted(runtime["payload"].items())).encode()
            values = {"VERSION": version, "ARCHIVE_NAME": artifact, "DIRECTORY_NAME": artifact.removesuffix(".tar.gz"),
                "PAYLOAD_FILE_COUNT": len(runtime["payload"]), "INSTALL_MANIFEST_SHA256": runtime["install_manifest_sha256"],
                "PAYLOAD_SUMS_SHA256": release.hashlib.sha256(sums).hexdigest()}
            payload.update({"payload.sha256": sums, **{name: linux.render_template(name + ".in", values).encode()
                           for name in ("install.sh", "README.txt")}})
        else:
            payload["notices/inno-setup/LICENSE.txt"] = b"Fictional compiler license"
        if "linux" in target:
            with tarfile.open(output / artifact, "w:gz", format=tarfile.GNU_FORMAT) as archive:
                for name, encoded in payload.items():
                    info = tarfile.TarInfo(artifact.removesuffix(".tar.gz") + "/" + name)
                    info.size = len(encoded)
                    info.mode = 0o755 if name in ("install.sh", "runtime/airk", "payload/service/airk", "payload/bin/airkctl") else 0o644
                    archive.addfile(info, io.BytesIO(encoded))
        else:
            (output / artifact).write_bytes(b"Fictional OS installer; these tests assert no actual platform trust")
        receipt = {"schema": "airk-standalone-installer/1", "version": version, "target": target,
            "commit": COMMIT, "cargo_lock_sha256": runtime["cargo_lock_sha256"],
            "runtime_install_sha256": runtime["install_manifest_sha256"], "runtime_source": runtime["source"],
            "artifact": artifact, "artifact_sha256": release.digest(output / artifact),
            "installer_kind": release.standalone.TARGETS[target][2],
            "payload": {name: release.hashlib.sha256(data).hexdigest() for name, data in payload.items()}}
        if "windows" in target:
            verification = {"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "c" * 40}
            receipt.update(signing={"platform": "Windows", "identity": "c" * 40, "azure": release.signer.AZURE_SIGNING,
                "binary_sha256": receipt["artifact_sha256"], "verification": verification},
                uninstaller={"identity": "c" * 40, "azure": release.signer.AZURE_SIGNING,
                    "unsigned_sha256": "1" * 64, "binary_sha256": "2" * 64, "verification": dict(verification)},
                compiler={"version": "6.7.1.0", "sha256": "3" * 64},
                installer_source_sha256=release.digest(release.ROOT / "installers/windows" / ("airk-service.iss" if split else "airk.iss")))
        elif "apple" in target:
            receipt.update(runtime_signing=runtime["signing"], runtime_signing_sha256="1" * 64,
                signing={"platform": "Darwin", "identity": "c" * 40, "team_id": "5PX7D99PXA",
                    "certificate_sha256": "4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1",
                    "trust": "apple_developer_id", "timestamp_present": True,
                    "notarization": {"id": "fictional-notary-job", "status": "Accepted", "stapled": True, "scope": artifact}})
        else:
            receipt["signing"] = {"platform": "Linux", "trust": "unsigned"}
            receipt["native_archive_sha256"] = runtime["archive_sha256"]
        if split:
            receipt.update(payload_schema="airk-service-payload/1",
                           payload_inventory_sha256=receipt["payload"]["payload/payload-inventory.json"])
        self.write_installer(output, receipt)
        return output, runtime, receipt

    def write_installer(self, output, receipt):
        (output / "installer.json").write_text(json.dumps(receipt))
        (output / "SHA256SUMS").write_text(
            f'{release.digest(output / receipt["artifact"])}  {receipt["artifact"]}\n'
            f'{release.digest(output / "installer.json")}  installer.json\n')

    def test_standalone_installers_bind_actual_artifact_and_matching_verified_native_candidate(self):
        for target in release.native.TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                output, native, receipt = self.standalone_fixture(Path(temporary), target)
                self.assertEqual(release.verify_installer(output, target, COMMIT, "1.8.14", native)[0], receipt)
                for field, changed in (("commit", "b" * 40), ("runtime_install_sha256", "0" * 64),
                                       ("runtime_source", {"sha256": "0" * 64, "files": 4}),
                                       ("artifact_sha256", "0" * 64), ("target", "other")):
                    wrong = copy.deepcopy(receipt)
                    wrong[field] = changed
                    self.write_installer(output, wrong)
                    with self.subTest(field=field), self.assertRaisesRegex(ValueError, "identity differs"):
                        release.verify_installer(output, target, COMMIT, "1.8.14", native)
                wrong = copy.deepcopy(receipt)
                wrong["payload"]["runtime/airk-install.json"] = "0" * 64
                self.write_installer(output, wrong)
                with self.assertRaisesRegex(ValueError, "different native runtime"):
                    release.verify_installer(output, target, COMMIT, "1.8.14", native)
                # An untouched manifest cannot authorize a different executable.
                wrong = copy.deepcopy(receipt)
                executable = "AIRK.app/Contents/MacOS/airk" if "apple" in target else "airk.exe" if "windows" in target else "airk"
                wrong["payload"]["runtime/" + executable] = "0" * 64
                self.assertEqual(wrong["payload"]["runtime/airk-install.json"], receipt["payload"]["runtime/airk-install.json"])
                self.write_installer(output, wrong)
                with self.assertRaisesRegex(ValueError, "different native runtime"):
                    release.verify_installer(output, target, COMMIT, "1.8.14", native)

    def test_installer_signing_evidence_is_platform_specific_and_cannot_be_relabelled(self):
        for target in release.native.TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                output, native, receipt = self.standalone_fixture(Path(temporary), target)
                wrong = copy.deepcopy(receipt)
                if "windows" in target:
                    wrong["uninstaller"]["verification"]["timestamp_present"] = False
                elif "apple" in target:
                    wrong["signing"]["notarization"]["stapled"] = False
                else:
                    wrong["signing"]["trust"] = "os_signed"
                self.write_installer(output, wrong)
                with self.assertRaisesRegex(ValueError, "Authenticode|notarization|unsigned"):
                    release.verify_installer(output, target, COMMIT, "1.8.14", native)

    def test_installer_publication_boundary_preserves_historical_and_preview_contracts(self):
        self.assertFalse(release.standalone.required("1.4.4", "stable"))
        self.assertFalse(release.standalone.required("1.8.14", "preview"))
        self.assertTrue(release.standalone.required("1.8.14", "stable"))
        self.assertTrue(release.standalone.required("1.9.0", "stable"))

    def test_installer_bootstrap_hashes_must_match_reviewed_source_even_with_resealed_metadata(self):
        for target in release.native.TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                output, native, receipt = self.standalone_fixture(Path(temporary), target)
                if "windows" in target:
                    receipt["installer_source_sha256"] = "0" * 64
                elif "apple" in target:
                    receipt["payload"]["Install AIRK.command"] = "0" * 64
                else:
                    receipt["payload"]["install.sh"] = "0" * 64
                self.write_installer(output, receipt)
                with self.assertRaisesRegex(ValueError, "provenance differs|bootstrap differs"):
                    release.verify_installer(output, target, COMMIT, "1.8.14", native)

    def test_modern_stable_assembly_and_bundle_recheck_require_all_installer_pairs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = root / "inputs"
            inputs.mkdir()
            version = "1.8.14"
            for target in release.native.TARGETS:
                signed = target in release.SIGNED_TARGETS
                prefix = "airk-signed" if signed else "airk"
                native_dir = inputs / f"{prefix}-native-{target}-{SUFFIX}"
                files, receipt, executables = pair_fixture(native_dir, target, signed, version)
                if signed:
                    pair_fixture(inputs / f"airk-native-{target}-{SUFFIX}", target, False, version)
                pair_vsix(inputs / f"{prefix}-vsix-{target}-{SUFFIX}", native_dir, files, receipt, executables)
                fixture = root / target
                fixture.mkdir()
                installer, _, _ = self.standalone_fixture(fixture, target)
                shutil.copytree(installer, inputs / f"airk-installer-{target}-{COMMIT}-123")
            record = release.assemble(inputs, root / "output", COMMIT, version, "v" + version, "stable", "123", "1")
            self.assertEqual(len(record["assets"]), 24)
            self.assertEqual(release.verify_bundle(root / "output", COMMIT, version, "v" + version, "123", "1"), record)
            # Missing input refuses before a publication directory is created.
            shutil.rmtree(inputs / f"airk-installer-x86_64-unknown-linux-gnu-{COMMIT}-123")
            with self.assertRaisesRegex(ValueError, "standalone installer files"):
                release.assemble(inputs, root / "missing-output", COMMIT, version, "v" + version, "stable", "123", "1")
            self.assertFalse((root / "missing-output").exists())
            # A metadata mismatch remains invalid even if generic bundle hashes
            # are recomputed; the native/installer relationship is revalidated.
            manifest = release.standalone.manifest(version, "x86_64-pc-windows-msvc")
            changed = json.loads((root / "output" / manifest).read_text())
            changed["runtime_source"]["sha256"] = "0" * 64
            (root / "output" / manifest).write_text(json.dumps(changed))
            record["assets"][manifest] = release.digest(root / "output" / manifest)
            (root / "output/release.json").write_text(json.dumps(record))
            (root / "output/SHA256SUMS").write_text("".join(
                f"{release.digest(path)}  {path.name}\n" for path in sorted((root / "output").iterdir()) if path.name != "SHA256SUMS"))
            with self.assertRaisesRegex(ValueError, "identity differs"):
                release.verify_bundle(root / "output", COMMIT, version, "v" + version, "123", "1")

    def test_modern_single_image_release_keeps_full_signed_bundle_without_launcher_copy(self):
        for target in release.native.TARGETS:
            signed = target in release.SIGNED_TARGETS
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                files, receipt, executables = pair_fixture(root / "candidate", target, signed, "1.8.14")
                self.assertEqual(len(executables), 1)
                self.assertNotIn("copied_launcher", receipt.get("signing") or {})
                self.assertFalse(any("airk-embed" in name or "airk-launcher" in name or "libonnxruntime" in name for name in files))
                observed, _ = release.verify_native(root / "candidate", target, COMMIT, "1.8.14", signed)
                self.assertEqual(observed, receipt)
                files["airk-embed"] = b"forbidden resurrected helper"
                seal_pair_files(files)
                save_pair_candidate(root / "candidate", files, receipt, executables)
                with self.assertRaisesRegex(ValueError, "Unexpected native payload"):
                    release.verify_native(root / "candidate", target, COMMIT, "1.8.14", signed)

    def test_all_platforms_verify_and_restore_the_real_service_layout(self):
        for target in release.native.TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                files, receipt, executables = pair_fixture(root / "candidate", target)
                observed, archive = release.verify_native(root / "candidate", target, COMMIT, PAIR_VERSION, False)
                self.assertEqual(observed, receipt)
                with zipfile.ZipFile(archive) as zipped:
                    for name in executables:
                        self.assertEqual((zipped.getinfo(name).external_attr >> 16) & 0o777, 0o755)
                if target not in release.SIGNED_TARGETS:
                    continue
                for name in ("Cargo.lock", "rust-toolchain.toml"):
                    (root / name).write_bytes((release.ROOT / name).read_bytes())
                with patch.object(release, "ROOT", root):
                    release.stage_signing(root / "candidate", target, COMMIT, PAIR_VERSION)
                    output = root / "artifacts/runtime" / target
                    self.assertEqual(release.native.inventory(output), receipt["payload"])
                    release.native.validate_runtime(output, PAIR_VERSION, target, receipt["source"])
                    for name in executables:
                        self.assertEqual((output / name).read_bytes(), files[name])
                    with self.assertRaises(FileExistsError):
                        release.stage_signing(root / "candidate", target, COMMIT, PAIR_VERSION)

    def test_promoted_merge_ref_candidate_verifies_only_with_the_identical_tree(self):
        target = "x86_64-pc-windows-msvc"
        for case in ("same tree", "no tree expected", "other tree", "missing receipt tree"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "candidate"
                files, receipt, executables = pair_fixture(directory, target)
                receipt = {**receipt, "commit": "d" * 40}
                if case != "missing receipt tree":
                    receipt["tree"] = "c" * 40
                save_pair_candidate(directory, files, receipt, executables)
                expected = None if case == "no tree expected" else "b" * 40 if case == "other tree" else "c" * 40
                if case == "same tree":
                    observed, _ = release.verify_native(directory, target, COMMIT, PAIR_VERSION, False, tree=expected)
                    self.assertEqual(observed["commit"], "d" * 40)
                else:
                    with self.assertRaisesRegex(ValueError, "provenance"):
                        release.verify_native(directory, target, COMMIT, PAIR_VERSION, False, tree=expected)

    def test_paired_release_assembles_all_platforms_with_original_producer_bindings(self):
        for mutation in (None, "component_source", "source_install"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for target in release.native.TARGETS:
                    signed = target in release.SIGNED_TARGETS
                    original = root / "in" / f"airk-native-{target}-{COMMIT}-123-1"
                    pair_fixture(original, target)
                    prefix = "airk-signed" if signed else "airk"
                    directory = root / "in" / f"{prefix}-native-{target}-{COMMIT}-456"
                    files, receipt, executables = pair_fixture(directory, target, signed)
                    if target == "aarch64-apple-darwin" and mutation:
                        if mutation == "component_source":
                            receipt["signing"]["components"][executables[1]]["source_sha256"] = "0" * 64
                        else:
                            receipt["signing"]["source_install_sha256"] = "0" * 64
                        write_receipt(directory, receipt)
                    pair_vsix(root / "in" / f"{prefix}-vsix-{target}-{COMMIT}-456", directory, files, receipt, executables)
                producer = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1")
                with patch.object(release.producers, "verify_record") as verified:
                    if mutation:
                        with self.assertRaisesRegex(ValueError, "did not originate"):
                            release.assemble(root / "in", root / "out", COMMIT, PAIR_VERSION, "v" + PAIR_VERSION, "stable", "456", "1", producer)
                        self.assertFalse((root / "out").exists())
                    else:
                        repair = {"tooling_commit": "b" * 40, "source_commit": COMMIT}
                        record = release.assemble(root / "in", root / "out", COMMIT, PAIR_VERSION, "v" + PAIR_VERSION, "stable", "456", "1", producer, repair=repair)
                        self.assertEqual(record["producer"], producer)
                        self.assertEqual(record["release_tooling_repair"], repair)
                        self.assertIn(f"/blob/{COMMIT}/docs/release-{PAIR_VERSION}.md", (root / "out" / "release-notes.md").read_text())
                        self.assertEqual(record["commit"], COMMIT)
                        self.assertEqual(len(record["assets"]), 16)
                        self.assertEqual(release.verify_bundle(root / "out", COMMIT, PAIR_VERSION, "v" + PAIR_VERSION, "456", "1"), record)
                    verified.assert_called_once_with(producer, COMMIT)
                if mutation is None:
                    assets = [{"id": index + 1, "name": path.name, "size": path.stat().st_size,
                               "digest": "sha256:" + release.digest(path)}
                              for index, path in enumerate((root / "out").iterdir())]
                    remote = {"id": 7, "draft": False, "tag_name": "v" + PAIR_VERSION,
                              "target_commitish": COMMIT, "prerelease": False, "assets": assets}
                    def api(path, **_kwargs):
                        return {"object": {"sha": COMMIT}} if path.startswith("git/ref/") else remote
                    with patch.object(release.producers, "api", side_effect=api), \
                            patch.object(release.producers.subprocess, "check_output", return_value=(root / "out/release.json").read_bytes()), \
                            patch.object(release.producers, "verify_record") as verified:
                        release.producers.verify_tag("v" + PAIR_VERSION, COMMIT)
                        verified.assert_called_once_with(producer, COMMIT)
                    with patch.object(release, "context", return_value=(repair["tooling_commit"], PAIR_VERSION, "v" + PAIR_VERSION)), \
                            patch.dict(os.environ, {"GITHUB_RUN_ID": "456", "GITHUB_RUN_ATTEMPT": "2"}), \
                            patch.object(release, "release_id_for_tag", return_value=7), \
                            patch.object(release, "github", side_effect=api), patch.object(release.subprocess, "run") as run:
                        release.publish(root / "out", resume=True, source_commit=COMMIT)
                        run.assert_not_called()

    def test_mac_pair_rejects_resealed_layout_and_signing_tampering(self):
        for mutation in ("flat_names", "foreign_plist", "missing_helper", "unexpected_path", "missing_ticket",
                         "crossed_notary", "unstapled", "wrong_scope", "component_digest", "flat_component", "install_digest"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "candidate"
                target = "aarch64-apple-darwin"
                files, receipt, executables = pair_fixture(directory, target, True)
                if mutation == "flat_names":
                    for name in executables:
                        files[Path(name).name] = files.pop(name)
                elif mutation == "foreign_plist":
                    files["AIRK.app/Contents/Info.plist"] = b"resealed foreign app configuration"
                elif mutation == "missing_helper":
                    files.pop(executables[1])
                elif mutation == "unexpected_path":
                    files["AIRK.app/Contents/extra-private.txt"] = b"fictional excluded payload"
                elif mutation == "missing_ticket":
                    files.pop("AIRK.app/Contents/CodeResources")
                elif mutation == "flat_component":
                    receipt["signing"]["components"]["airk"] = receipt["signing"]["components"].pop(executables[0])
                elif mutation == "install_digest":
                    receipt["signing"]["install_manifest_sha256"] = "0" * 64
                else:
                    component = receipt["signing"]["components"][executables[1]]
                    if mutation == "component_digest":
                        component["binary_sha256"] = "0" * 64
                    else:
                        field, value = {"crossed_notary": ("id", "another-app-job"),
                                        "unstapled": ("stapled", False), "wrong_scope": ("scope", "airk-embed")}[mutation]
                        component["notarization"][field] = value
                seal_pair_files(files)
                save_pair_candidate(directory, files, receipt, executables)
                with self.assertRaises(ValueError):
                    release.verify_native(directory, target, COMMIT, PAIR_VERSION, True)


SPLIT = "1.8.24"
SPLIT_TREE = "c" * 40
# Reviewed, checked-in split release catalog (split plan §10.5.4; never derived from the code under test).
SPLIT_TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin")
SPLIT_SIGNED = {"x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin"}
SPLIT_PREVIEW_ASSETS = {"airk-1.8.24.vsix", "airk-1.8.24.vsix-manifest.json",
                        *(f"airk-1.8.24-{target}{suffix}" for target in SPLIT_TARGETS
                          for suffix in (".zip", ".native-manifest.json"))}
SPLIT_STABLE_ASSETS = SPLIT_PREVIEW_ASSETS | {
    *(f"airk-1.8.24-{target}-installer.manifest.json" for target in SPLIT_TARGETS),
    "airk-1.8.24-x86_64-unknown-linux-gnu-installer.tar.gz", "airk-1.8.24-x86_64-pc-windows-msvc-installer.exe",
    "airk-1.8.24-aarch64-apple-darwin-installer.dmg", "airk-1.8.24-x86_64-apple-darwin-installer.dmg"}
SPLIT_ADMIN = {"bundle_sha256": "4" * 64, "inventory_sha256": "3" * 64, "node": "v24.15.0", "npm": "11.6.2",
               "locks": {"packages/admin/package-lock.json": "9" * 64}}
universal = release.module("universal_fixtures", "tests/test_package_extension.py")
promotion = release.module("split_promote_vsix", "promote-vsix.py")


def split_files(target, signed=False, version=SPLIT, admin=SPLIT_ADMIN, images=True):
    """Fictional bytes in the split pair layout: schema-3 pair metadata and schema-2 build receipt."""
    files, executables = pair_files(target, signed, version)
    if not images:
        files.pop(executables[1])
    operating_system = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    pair = {"schema": 3, "version": version, "os": operating_system,
            "arch": "aarch64" if target.startswith("aarch64-") else "x86_64",
            "airk_sha256": release.hashlib.sha256(files[executables[0]]).hexdigest(),
            "airkctl_sha256": release.hashlib.sha256(("signed " if signed else "tested ").encode()
                                                     + executables[1].encode()).hexdigest(),
            "i1_protocol": 1, "i2_contract": 1, "admin_inventory_sha256": admin["inventory_sha256"]}
    imports = {"schema": 1, "command": ["inspect", "airk"], "stdout": "fixture imports\n", "external_onnxruntime_imports": False}
    build = {"schema": 2, "source": {"sha256": "d" * 64, "files": 4}, "profile": "release", "runtime": pair,
             "rustc": "rustc fixture\nhost: " + target,
             "static_ort": {"build_receipt_sha256": "6" * 64, "target": target, "static_link_closure_sha256": "7" * 64,
                            "static_link_supplement_sha256": None, "static_link_recovery_sha256": None,
                            "verified_inputs_sha256": "8" * 64},
             "static_imports": imports, "airkctl_static_imports": {**imports, "inference_symbols": False},
             "embedded_model_smoke": {"schema": 1}, "admin_assets": admin}
    files["airk-runtime.json"] = json.dumps(pair).encode()
    files["airk-build.json"] = json.dumps(build).encode()
    seal_pair_files(files)
    return files, executables


def split_fixture(directory, target, signed=False, version=SPLIT, admin=SPLIT_ADMIN, images=True):
    files, executables = split_files(target, signed, version, admin, images)
    original, _ = split_files(target, version=version, admin=admin)
    signing = None
    if signed:
        builder = release.native.runtime_builder()
        components = {}
        for name in builder.signing_names("windows" if "windows" in target else "macos", version):
            component = {"schema": "airk-native-signing/1", "target": target, "commit": COMMIT,
                         "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                         "source_sha256": release.hashlib.sha256(original[name]).hexdigest(),
                         "binary_sha256": release.hashlib.sha256(files[name]).hexdigest() if name in files else "0" * 64,
                         "identity": "c" * 40}
            if "windows" in target:
                component.update(platform="Windows", azure=release.signer.AZURE_SIGNING,
                    verification={"trust": "authenticode_valid", "timestamp_present": True, "signer_thumbprint": "c" * 40})
            else:
                component.update(platform="Darwin", team_id="5PX7D99PXA",
                    verification={"trust": "apple_developer_id", "team_id": "5PX7D99PXA",
                                  "timestamp_present": True, "hardened_runtime": True},
                    notarization={"status": "Accepted", "id": "fictional-app-job", "stapled": True, "scope": "AIRK.app"})
            components[name] = component
        signing = {"schema": "airk-service-pair-signing/1", "target": target, "commit": COMMIT,
                   "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
                   "source_install_sha256": release.hashlib.sha256(original["airk-install.json"]).hexdigest(),
                   "install_manifest_sha256": release.hashlib.sha256(files["airk-install.json"]).hexdigest(),
                   "components": components}
    pair = json.loads(files["airk-runtime.json"])
    policy = release.native.runtime_builder().load_script("int8-policy")
    receipt = {"schema": "airk-native-candidate/3", "version": version, "target": target, "commit": COMMIT,
               "tree": SPLIT_TREE, "dirty_checkout": False, "source": {"sha256": "d" * 64, "files": 4},
               "cargo_lock_sha256": release.digest(release.ROOT / "Cargo.lock"),
               "toolchain_file_sha256": release.digest(release.ROOT / "rust-toolchain.toml"),
               "rustc": "rustc fixture\nhost: " + target, "cargo": "cargo fixture",
               "archive": f"airk-{version}-{target}.zip", "airk_sha256": pair["airk_sha256"],
               "airkctl_sha256": pair["airkctl_sha256"],
               "admin_assets": {"inventory_sha256": admin["inventory_sha256"], "bundle_sha256": admin["bundle_sha256"]},
               "i1_protocol": 1, "i2_contract": 1, "bundled_model": {"manifest": "fixture-model.json"},
               "embedded_models": {"schema": 1, "fp32_included": False, "profiles": policy.catalog()["profiles"]},
               "embedded_model_smoke": {"schema": 1, "profile": "fixture"}, "signed_distribution": signed,
               "signing": signing, "qualification": "native service pair candidate; installed acceptance and oldest OS not qualified"}
    save_pair_candidate(directory, files, receipt, executables)
    return files, receipt, executables


def split_producer():
    names = sorted({f"airk-native-{target}-{COMMIT}-123-1" for target in SPLIT_TARGETS}
                   | {f"airk-vsix-universal-{COMMIT}-123-1"})
    return dict(schema="airk-ci-producer/1", repository="iokaio/airk", commit=COMMIT, workflow_id=42,
                run_id="123", attempt="1", event="push",
                artifacts={name: dict(id=index + 1, digest="sha256:" + f"{index:x}" * 64) for index, name in enumerate(names)})


class SplitReleaseContracts(MainReleaseFixture):
    standalone_fixture = ServicePairReleaseContracts.standalone_fixture
    write_installer = ServicePairReleaseContracts.write_installer

    def split_inputs(self, root, channel, producer=None):
        producer = producer or split_producer()
        inputs = root / "in"
        for target in SPLIT_TARGETS:
            original = root / "originals" / target
            split_fixture(original, target)
            release.write_source_record(inputs / f"airk-source-{target}-{COMMIT}-456", original, producer, target, COMMIT)
            signed = target in SPLIT_SIGNED
            native_dir = inputs / f"{'airk-signed' if signed else 'airk'}-native-{target}-{COMMIT}-456"
            if signed:
                split_fixture(native_dir, target, True)
            else:
                shutil.copytree(original, native_dir)
            if channel == "stable":
                fixture = root / "installers" / target
                fixture.mkdir(parents=True)
                installer, _, _ = self.standalone_fixture(fixture, target, SPLIT, split_fixture)
                shutil.copytree(installer, inputs / f"airk-installer-{target}-{COMMIT}-456")
        tested = inputs / f"airk-vsix-universal-{COMMIT}-123-1"
        universal.universal_candidate(tested, commit=COMMIT, tree=SPLIT_TREE)
        if channel == "preview":
            promotion.promote_universal(tested, inputs / f"airk-vsix-universal-{COMMIT}-456", COMMIT, SPLIT, "preview")
        else:
            shutil.copytree(tested, inputs / f"airk-vsix-universal-{COMMIT}-456")
        return inputs, producer

    def assemble(self, inputs, output, channel, producer):
        return release.assemble(inputs, output, COMMIT, SPLIT, "v" + SPLIT, channel, "456", "1", producer,
                                pinned=True, source_records=True)

    def test_split_pair_verifies_both_images_admin_identity_and_restores_both_for_signing(self):
        for target in SPLIT_TARGETS:
            for signed in ((False, True) if target in SPLIT_SIGNED else (False,)):
                with self.subTest(target=target, signed=signed), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    files, receipt, executables = split_fixture(root / "candidate", target, signed)
                    self.assertEqual(len(executables), 2)
                    observed, _ = release.verify_native(root / "candidate", target, COMMIT, SPLIT, signed)
                    self.assertEqual(observed, receipt)
                    # A split candidate never qualifies a pre-split version, and vice versa.
                    with self.assertRaisesRegex(ValueError, "provenance or signing state"):
                        release.verify_native(root / "candidate", target, COMMIT, "1.8.23", signed)
                    if signed:
                        continue
                    pair_fixture(root / "pre-split", target, False, SPLIT)
                    with self.assertRaisesRegex(ValueError, "provenance or signing state"):
                        release.verify_native(root / "pre-split", target, COMMIT, SPLIT, False)
                    if target not in SPLIT_SIGNED:
                        continue
                    for name in ("Cargo.lock", "rust-toolchain.toml"):
                        (root / name).write_bytes((release.ROOT / name).read_bytes())
                    with patch.object(release, "ROOT", root):
                        release.stage_signing(root / "candidate", target, COMMIT, SPLIT)
                    output = root / "artifacts/runtime" / target
                    self.assertEqual(release.native.inventory(output), receipt["payload"])
                    for name in executables:
                        self.assertEqual((output / name).read_bytes(), files[name])

    def test_split_pair_refuses_missing_image_admin_drift_and_local_or_platform_receipts(self):
        target = "x86_64-pc-windows-msvc"
        for mutation, reason in (("missing_airkctl", "split receipt contract: inventory_mismatch"),
                                 ("admin_receipt", "admin, interface or image identities"),
                                 ("vsix_target", "provenance or signing state"),
                                 ("local_signing", "split receipt contract"),
                                 ("component_signing", "split receipt contract: schema_mismatch")):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "candidate"
                signed = mutation in ("local_signing", "component_signing")
                files, receipt, executables = split_fixture(directory, target, signed, images=mutation != "missing_airkctl")
                if mutation == "admin_receipt":
                    receipt["admin_assets"] = {**receipt["admin_assets"], "bundle_sha256": "5" * 64}
                elif mutation == "vsix_target":
                    receipt["vsix_target"] = "win32-x64"
                elif mutation == "local_signing":
                    receipt["signing"] = {**receipt["signing"], "schema": "airk-service-pair-signing-local/1"}
                elif mutation == "component_signing":
                    receipt["signing"] = next(iter(receipt["signing"]["components"].values()))
                write_receipt(directory, receipt)
                with self.assertRaisesRegex(ValueError, reason):
                    release.verify_native(directory, target, COMMIT, SPLIT, signed)

    def test_split_release_assembles_pairs_installers_and_one_universal_vsix(self):
        for channel, expected in (("preview", SPLIT_PREVIEW_ASSETS), ("stable", SPLIT_STABLE_ASSETS)):
            with self.subTest(channel=channel), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                inputs, producer = self.split_inputs(root, channel)
                with patch.object(release.producers, "verify_record") as remote:
                    record = self.assemble(inputs, root / "out", channel, producer)
                    remote.assert_not_called()
                self.assertEqual(set(record["assets"]), expected)
                self.assertEqual(record["producer"], producer)
                self.assertEqual(release.verify_bundle(root / "out", COMMIT, SPLIT, "v" + SPLIT, "456", "1"), record)
                notes = (root / "out/release-notes.md").read_text(encoding="utf-8")
                self.assertIn("`airkctl`", notes)
                self.assertIn("airk-1.8.24.vsix", notes)
                for target in SPLIT_TARGETS:
                    manifest = json.loads((root / "out" / f"airk-{SPLIT}-{target}.native-manifest.json").read_text())
                    self.assertEqual(manifest["admin_assets"], {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64})
                    self.assertIs(manifest["signed_distribution"], target in SPLIT_SIGNED)
                vsix = json.loads((root / "out/airk-1.8.24.vsix-manifest.json").read_text())
                self.assertEqual((vsix["target"], vsix["pre_release"]), ("universal", channel == "preview"))
                # Marketplace staging copies the one universal VSIX and reads no native bytes.
                with patch.object(release.producers, "verify_record") as remote, \
                        patch.object(release, "verify_native", side_effect=AssertionError("native bytes read")):
                    release.stage_marketplace_bundle(root / "out", root / "market", COMMIT, SPLIT, channel, "456", "1")
                    remote.assert_called_once_with(producer, COMMIT, version=SPLIT)
                self.assertEqual(sorted(path.name for path in (root / "market").iterdir()), ["universal"])
                self.assertEqual((root / "market/universal/airk-1.8.24.vsix").read_bytes(),
                                 (root / "out/airk-1.8.24.vsix").read_bytes())
                self.assertEqual(len(release.extension.verify(root / "market", COMMIT, SPLIT, channel)), 1)
                # The bundle check rejects a pre-split platform VSIX added to the split asset set.
                extra = root / "out" / f"airk-{SPLIT}-win32-x64.vsix"
                extra.write_bytes(b"fictional platform package")
                record["assets"][extra.name] = release.digest(extra)
                (root / "out/release.json").write_text(json.dumps(record), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "asset set differs"):
                    release.verify_bundle(root / "out", COMMIT, SPLIT, "v" + SPLIT, "456", "1")

    def test_split_assembly_fails_closed_before_emitting_output(self):
        windows = "x86_64-pc-windows-msvc"
        for mutation, reason in (("missing_release_vsix", "Missing or linked candidate"),
                                 ("vsix_drift", "producer's tested universal VSIX"),
                                 ("producer_without_universal", "identity"),
                                 # The admin bundle is sealed in the pair's install manifest.
                                 ("signed_admin_drift", "did not originate in the tested candidate"),
                                 ("pre_split_native", "provenance or signing state"),
                                 ("linux_drift", "differs from the tested producer"),
                                 ("no_source_records", "authenticated producer and source receipts")):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                inputs, producer = self.split_inputs(root, "preview")
                released = inputs / f"airk-vsix-universal-{COMMIT}-456"
                source_records = mutation != "no_source_records"
                if mutation == "missing_release_vsix":
                    shutil.rmtree(released)
                elif mutation == "vsix_drift":
                    receipt = json.loads((released / "manifest.json").read_text())
                    receipt["i2_contracts"] = [1, 2]
                    write_receipt(released, receipt)
                elif mutation == "producer_without_universal":
                    producer = copy.deepcopy(producer)
                    del producer["artifacts"][f"airk-vsix-universal-{COMMIT}-123-1"]
                elif mutation == "signed_admin_drift":
                    directory = inputs / f"airk-signed-native-{windows}-{COMMIT}-456"
                    shutil.rmtree(directory)
                    split_fixture(directory, windows, True, admin={**SPLIT_ADMIN, "bundle_sha256": "5" * 64})
                elif mutation == "pre_split_native":
                    directory = inputs / f"airk-signed-native-{windows}-{COMMIT}-456"
                    shutil.rmtree(directory)
                    pair_fixture(directory, windows, True, SPLIT)
                elif mutation == "linux_drift":
                    directory = inputs / f"airk-native-x86_64-unknown-linux-gnu-{COMMIT}-456"
                    receipt = json.loads((directory / "manifest.json").read_text())
                    receipt["bundled_model"] = {"manifest": "another-model.json"}
                    write_receipt(directory, receipt)
                with patch.object(release.producers, "verify_record"), self.assertRaisesRegex(ValueError, reason):
                    release.assemble(inputs, root / "out", COMMIT, SPLIT, "v" + SPLIT, "preview", "456", "1", producer,
                                     pinned=True, source_records=source_records)
                self.assertFalse((root / "out").exists())

    def test_split_installers_bind_the_service_layout_inventory_and_wrappers(self):
        images = {"x86_64-pc-windows-msvc": "payload/service/airk.exe", "x86_64-unknown-linux-gnu": "payload/bin/airkctl",
                  "aarch64-apple-darwin": "payload/AIRK.app/Contents/MacOS/airkctl",
                  "x86_64-apple-darwin": "payload/AIRK.app/Contents/MacOS/airk"}
        for target in SPLIT_TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                output, native, receipt = self.standalone_fixture(Path(temporary), target, SPLIT, split_fixture)
                self.assertEqual(release.verify_installer(output, target, COMMIT, SPLIT, native)[0], receipt)
                self.assertNotIn("runtime/airk-install.json", receipt["payload"])
                mutations = [("image", "different native runtime"), ("schema", "pinned service payload inventory"),
                             ("inventory", "pinned service payload inventory")]
                mutations.append(("presplit_script", "provenance differs") if "windows" in target
                                 else ("roster", "roster differs"))
                for mutation, reason in mutations:
                    wrong = copy.deepcopy(receipt)
                    if mutation == "image":
                        wrong["payload"][images[target]] = "0" * 64
                    elif mutation == "schema":
                        del wrong["payload_schema"]
                    elif mutation == "inventory":
                        wrong["payload_inventory_sha256"] = "0" * 64
                    elif mutation == "presplit_script":
                        wrong["installer_source_sha256"] = release.digest(release.ROOT / "installers/windows/airk.iss")
                    else:
                        wrong["payload"]["payload.sha256"] = "0" * 64
                    self.write_installer(output, wrong)
                    with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, reason):
                        release.verify_installer(output, target, COMMIT, SPLIT, native)

    def test_split_linux_inventory_must_describe_the_tested_pair(self):
        target = "x86_64-unknown-linux-gnu"
        for options, reason in (({"admin": {"inventory_sha256": "3" * 64, "bundle_sha256": "5" * 64}}, "inventory differs"),
                                ({"layout_change": {"notices/NOTICE": b"changed after the inventory"}}, "different native runtime")):
            with self.subTest(options=options), tempfile.TemporaryDirectory() as temporary:
                output, native, _ = self.standalone_fixture(Path(temporary), target, SPLIT, split_fixture, options)
                with self.assertRaisesRegex(ValueError, reason):
                    release.verify_installer(output, target, COMMIT, SPLIT, native)

    def test_split_source_record_binds_the_tested_pair_alone(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            split_fixture(root / "native", "x86_64-pc-windows-msvc")
            producer = split_producer()
            release.write_split_source_record(root / "native", root / "source", producer, "x86_64-pc-windows-msvc",
                                              COMMIT, SPLIT)
            original = release.source_receipt(root / "source", producer, "x86_64-pc-windows-msvc", COMMIT, SPLIT)
            self.assertEqual(original["schema"], "airk-native-candidate/3")
            with self.assertRaisesRegex(ValueError, "Pre-split source records"):
                release.write_split_source_record(root / "native", root / "other", producer, "x86_64-pc-windows-msvc",
                                                  COMMIT, "1.8.23")
            # The pre-split eight-artifact producer catalog cannot authorize a split candidate.
            legacy = {**producer, "artifacts": {f"airk-{kind}-{target}-{COMMIT}-123-1": dict(id=index * 2 + offset + 1, digest="sha256:" + "1" * 64)
                                                for index, target in enumerate(SPLIT_TARGETS)
                                                for offset, kind in enumerate(("native", "vsix"))}}
            with self.assertRaises(ValueError):
                release.write_split_source_record(root / "native", root / "legacy", producer=legacy,
                                                  target="x86_64-pc-windows-msvc", commit=COMMIT, version=SPLIT)
            self.assertFalse((root / "legacy").exists())


class GithubReleaseContracts(MainReleaseFixture):
    def test_current_mac_candidate_requires_signed_executable_launcher_image(self):
        target = "aarch64-apple-darwin"
        builder = release.native.runtime_builder()
        library = "AIRK.app/Contents/MacOS/libonnxruntime.dylib"
        # Fixture inference bytes isolate this inventory/signing contract; the
        # separate int8 distribution suite verifies the actual pinned library.
        assets = {library: {"sha256": release.hashlib.sha256(b"signed " + library.encode()).hexdigest()}}
        with patch.object(release.native, "runtime_builder", return_value=builder), \
                patch.object(builder, "inference_assets", return_value=assets):
            for version, count in (("1.8.7", 3), ("1.8.8", 4)):
                with self.subTest(version=version), tempfile.TemporaryDirectory() as temporary:
                    directory = Path(temporary) / "candidate"
                    files, receipt, executables = pair_fixture(directory, target, True, version)
                    self.assertEqual(len(receipt["signing"]["components"]), count)
                    release.verify_native(directory, target, COMMIT, version, True)
                    if count == 4:
                        image = builder.launcher_name("macos", version)
                        copied = receipt["signing"].pop("copied_launcher")
                        write_receipt(directory, receipt)
                        with self.assertRaisesRegex(ValueError, "copied-launcher evidence"):
                            release.verify_native(directory, target, COMMIT, version, True)
                        for field, bad in (("schema", 2), ("schema", True), ("binary_sha256", "0" * 64),
                                           ("version", "1.8.7"), ("strict_signature", False),
                                           ("standalone_version_probe", False), ("strict_signature", 1)):
                            receipt["signing"]["copied_launcher"] = {**copied, field: bad}
                            write_receipt(directory, receipt)
                            with self.subTest(copied_field=field, bad=bad), self.assertRaisesRegex(ValueError, "copied-launcher evidence"):
                                release.verify_native(directory, target, COMMIT, version, True)
                        receipt["signing"]["copied_launcher"] = copied
                        with zipfile.ZipFile(directory / receipt["archive"]) as zipped:
                            self.assertTrue(zipped.getinfo(image).external_attr >> 16 & 0o111)
                        component = receipt["signing"]["components"].pop(image)
                        write_receipt(directory, receipt)
                        with self.assertRaisesRegex(ValueError, "signing receipt"):
                            release.verify_native(directory, target, COMMIT, version, True)
                        receipt["signing"]["components"][image] = component
                        archive = directory / receipt["archive"]
                        with zipfile.ZipFile(archive, "w") as zipped:
                            for name, data in files.items():
                                info = zipfile.ZipInfo(name)
                                info.external_attr = (0o100755 if name in executables else 0o100644) << 16
                                zipped.writestr(info, data)
                        receipt["archive_sha256"] = release.digest(archive)
                        write_receipt(directory, receipt)
                        with self.assertRaisesRegex(ValueError, "lost executable permission"):
                            release.verify_native(directory, target, COMMIT, version, True)
                        files.pop(image)
                        seal_pair_files(files)
                        save_pair_candidate(directory, files, receipt, executables)
                        with self.assertRaisesRegex(ValueError, "executable missing"):
                            release.verify_native(directory, target, COMMIT, version, True)

    def test_product_screenshot_links_are_bound_to_release_source_and_preserve_old_release_notes(self):
        notes = release.interface_previews("1.7.1", COMMIT)
        self.assertIn(f"/{COMMIT}/docs/workbench/images/workbench-1.7.1-results.png", notes)
        self.assertIn(f"/{COMMIT}/docs/explorer/images/explorer-1.7.1.png", notes)
        self.assertNotIn("/main/", notes)
        self.assertEqual(release.interface_previews("1.7.0", COMMIT), "")

    def test_service_native_release_requires_paired_inventory_and_rejects_legacy_schema(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "native"
            target = "x86_64-unknown-linux-gnu"
            _, receipt = native_fixture(directory, target)
            (directory / receipt["archive"]).unlink()
            version = "1.4.0"
            files = {"airk": b"main fixture", "airk-embed": b"helper fixture", "LICENSE": b"license", "NOTICE": b"notice", "THIRD_PARTY_NOTICES.md": b"attribution",
                     "notices/inventory.json": b"{}"}
            sha = lambda data: release.hashlib.sha256(data).hexdigest()
            pair = {"schema": 1, "version": version, "os": "linux", "arch": "x86_64",
                    "airk_sha256": sha(files["airk"]), "embed_sha256": sha(files["airk-embed"])}
            source = {"sha256": "d" * 64, "files": 4}
            files["airk-runtime.json"] = json.dumps(pair).encode()
            files["airk-build.json"] = json.dumps({"schema": 1, "runtime": pair, "source": source}).encode()
            install = {key: pair[key] for key in ("schema", "version", "os", "arch")}
            install["files"] = {name: sha(data) for name, data in files.items()}
            files["airk-install.json"] = json.dumps(install).encode()
            archive = directory / f"airk-{version}-{target}.zip"
            with zipfile.ZipFile(archive, "w") as zipped:
                for name, data in files.items():
                    info = zipfile.ZipInfo(name)
                    info.external_attr = (0o100755 if name in ("airk", "airk-embed") else 0o100644) << 16
                    zipped.writestr(info, data)
            receipt.update(schema="airk-native-candidate/2", version=version, source=source,
                           archive=archive.name, archive_sha256=release.digest(archive),
                           payload={name: sha(data) for name, data in files.items()},
                           install_manifest_sha256=sha(files["airk-install.json"]))
            write_receipt(directory, receipt)
            observed, _ = release.verify_native(directory, target, COMMIT, version, False)
            self.assertEqual(observed["source"], source)
            receipt["schema"] = "airk-native-candidate/1"
            write_receipt(directory, receipt)
            with self.assertRaisesRegex(ValueError, "provenance"):
                release.verify_native(directory, target, COMMIT, version, False)

    def assemble(self, directory, output, channel="preview"):
        return release.assemble(directory, output, COMMIT, VERSION, TAG, channel, "123", "1")

    def test_promotion_keeps_producer_and_signing_run_distinct(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures(root / "in")
            for path in list((root / "in").iterdir()):
                if path.name.startswith("airk-signed-") or "x86_64-unknown-linux-gnu" in path.name:
                    shutil.copytree(path, path.with_name(path.name.replace(SUFFIX, COMMIT + "-456")))
            producer = dict(schema="airk-ci-producer/1", commit=COMMIT, run_id="123", attempt="1")
            with patch.object(release.producers, "verify_record") as verify:
                record = release.assemble(root / "in", root / "out", COMMIT, VERSION, TAG, "preview", "456", "2", producer)
                verify.assert_called_once_with(producer, COMMIT)
            self.assertEqual(record["producer"], producer)
            self.assertEqual(record["run_id"], "456")
            self.assertEqual(record["attempt"], "2")
            self.assertIn(f"/blob/{COMMIT}/docs/releases/release-{VERSION}.md", (root / "out" / "release-notes.md").read_text())
            release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "456", "2")

    def test_resume_uses_original_assembly_and_never_overwrites_conflicts(self):
        for state in ("draft", "missing", "published", "conflicting", "extra", "future"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                self.assemble(root / "in", root / "out")
                assets = [{"name": p.name, "digest": "sha256:" + release.digest(p)} for p in (root / "out").iterdir()]
                complete = dict(id=7, tag_name=TAG, draft=True, target_commitish=COMMIT, prerelease=True, assets=assets)
                remote = {**complete, "assets": list(assets)}
                if state == "missing":
                    remote["assets"].pop()
                elif state == "published":
                    remote["draft"] = False
                elif state == "conflicting":
                    remote["assets"][0] = {**assets[0], "digest": "sha256:" + "0" * 64}
                elif state == "extra":
                    remote["assets"].append(dict(name="unrecognized", digest=None))
                responses = iter([remote, complete])
                def api(path, **kwargs):
                    if path == "releases/7":
                        return next(responses)
                    if path.startswith("git/ref/"):
                        return {"object": {"sha": COMMIT}} if state == "published" else None
                    self.fail(path)
                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "0" if state == "future" else "2"}), \
                        patch.object(release, "release_id_for_tag", return_value=7), \
                        patch.object(release, "github", side_effect=api), \
                        patch.object(release.subprocess, "run") as run:
                    if state in ("conflicting", "extra", "future"):
                        with self.assertRaises(ValueError):
                            release.publish(root / "out", resume=True)
                        run.assert_not_called()
                    else:
                        release.publish(root / "out", resume=True)
                        self.assertEqual(run.call_count, {"draft": 1, "missing": 2, "published": 0}[state])
                        for call in run.call_args_list:
                            self.assertNotIn("--clobber", call.args[0])
                            self.assertNotIn("create", call.args[0])

    def test_complete_platforms_form_verifiable_public_bundle(self):
        for channel in ("preview", "stable"):
            with self.subTest(channel=channel), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in", channel)
                record = self.assemble(root / "in", root / "out", channel)
                self.assertEqual(len(record["assets"]), 16)
                self.assertEqual(len(list((root / "out").iterdir())), 19)
                self.assertEqual(release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "1"), record)
                with self.assertRaisesRegex(ValueError, "another run"):
                    release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "2")
                (root / "out" / next(iter(record["assets"]))).write_bytes(b"tampered")
                with self.assertRaisesRegex(ValueError, "digest mismatch"):
                    release.verify_bundle(root / "out", COMMIT, VERSION, TAG, "123", "1")

    def test_native_identity_signing_notarization_and_path_fail_closed(self):
        cases = (
            ("dirty_checkout", True), ("commit", "b" * 40), ("signed_distribution", False),
            ("signing.notarization.status", "Rejected"), ("signing.team_id", "WRONGTEAM1"),
            ("signing.binary_sha256", "0" * 64), ("signing.verification.timestamp_present", False),
            ("archive", "../outside.zip"),
        )
        for field, value in cases:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                target = "aarch64-apple-darwin"
                _, receipt = native_fixture(root / "in", target, True)
                selected = receipt
                keys = field.split(".")
                for key in keys[:-1]:
                    selected = selected[key]
                selected[keys[-1]] = value
                if field == "archive":
                    (root / "in/manifest.json").write_text(json.dumps(receipt), encoding="utf-8")
                else:
                    write_receipt(root / "in", receipt)
                with self.assertRaises(ValueError):
                    release.verify_native(root / "in", target, COMMIT, VERSION, True)

    def test_missing_platform_and_wrong_vsix_pair_never_stage_output(self):
        for mutation in ("missing", "pair", "source", "unexpected"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                target = "x86_64-pc-windows-msvc"
                directory = root / "in" / f"airk-signed-vsix-{target}-{SUFFIX}"
                if mutation == "missing":
                    (directory / "manifest.json").unlink()
                elif mutation == "unexpected":
                    (directory / "private.env").write_text("fictional excluded data")
                elif mutation == "pair":
                    receipt = json.loads((directory / "manifest.json").read_text())
                    receipt["native_manifest_sha256"] = "0" * 64
                    write_receipt(directory, receipt)
                else:
                    directory = root / "in" / f"airk-native-{target}-{SUFFIX}"
                    receipt = json.loads((directory / "manifest.json").read_text())
                    archive = directory / receipt["archive"]
                    with zipfile.ZipFile(archive) as zipped:
                        entries = [(entry, zipped.read(entry)) for entry in zipped.infolist()]
                    changed = b"different original fixture"
                    with zipfile.ZipFile(archive, "w") as zipped:
                        for entry, data in entries:
                            zipped.writestr(entry, changed if entry.filename == "airk.exe" else data)
                    receipt["payload"]["airk.exe"] = release.hashlib.sha256(changed).hexdigest()
                    receipt["archive_sha256"] = release.digest(archive)
                    write_receipt(directory, receipt)
                with self.assertRaises((ValueError, FileNotFoundError)):
                    self.assemble(root / "in", root / "out")
                self.assertFalse((root / "out").exists())

    def test_version_identity_and_existing_tag(self):
        self.assertEqual(release.identity(VERSION), TAG)
        for version in ("../version", "0.1.0-beta", "01.1.0", "1.1.0\nbad"):
            with self.assertRaises(ValueError):
                release.identity(version)
        with patch.object(release, "github", return_value={"id": 1}), patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "already exists"):
                release.require_unused_tag(TAG)
            run.assert_not_called()

    def test_only_manual_airk_main_context_is_admitted(self):
        environment = {"GITHUB_REPOSITORY": release.REPOSITORY, "GITHUB_REF": "refs/heads/main",
                       "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_WORKFLOW_REF": release.WORKFLOW,
                       "GITHUB_SHA": COMMIT, "GITHUB_RUN_NUMBER": "42", "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8")
            manifest = root / "packages/vscode/package.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('{"version":"0.1.0"}', encoding="utf-8")
            with patch.dict(os.environ, environment, clear=True), patch.object(release, "ROOT", root):
                self.assertEqual(release.context(), (COMMIT, VERSION, TAG))
                for key, value in (("GITHUB_REF", "refs/tags/v0.1.0"), ("GITHUB_EVENT_NAME", "pull_request"),
                                   ("GITHUB_WORKFLOW_REF", release.WORKFLOW.replace("github-release", "ci"))):
                    with patch.dict(os.environ, {key: value}), self.assertRaises(ValueError):
                        release.context()
                manifest.write_text('{"version":"0.1.1"}', encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "versions differ"):
                    release.context()

    def test_publish_checks_uploaded_assets_before_making_draft_public(self):
        cases = ((channel, corruption) for channel in ("preview", "stable")
                 for corruption in (None, "missing_asset", "digest", "tag", "id", "commit", "channel", "published", "tag_ref"))
        for channel, corruption in cases:
            with self.subTest(channel=channel, corruption=corruption), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in", channel)
                self.assemble(root / "in", root / "out", channel)
                assets = [{"name": p.name, "digest": "sha256:" + release.digest(p)} for p in (root / "out").iterdir()]
                if corruption == "missing_asset":
                    assets.pop()
                elif corruption == "digest":
                    assets[0]["digest"] = "sha256:" + "0" * 64
                remote = {"id": 7, "tag_name": TAG, "draft": True, "target_commitish": COMMIT,
                          "prerelease": channel == "preview", "assets": assets}
                changes = {"tag": ("tag_name", "other-tag"), "id": ("id", 8),
                           "commit": ("target_commitish", "b" * 40), "channel": ("prerelease", channel != "preview"),
                           "published": ("draft", False)}
                if corruption in changes:
                    key, value = changes[corruption]
                    remote[key] = value

                def api(path, **kwargs):
                    if path == "releases?per_page=100":
                        self.assertTrue(kwargs.get("paginate"))
                        return [[{"id": 6, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG}]]
                    if path == "releases/7":
                        return remote
                    if path == f"git/ref/tags/{TAG}":
                        return {"object": {"sha": "b" * 40}} if corruption == "tag_ref" else None
                    self.fail("Unexpected API endpoint (drafts cannot be fetched by tag): " + path)

                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}), \
                        patch.object(release, "require_unused_tag"), \
                        patch.object(release, "github", side_effect=api), \
                        patch.object(release.subprocess, "run") as run:
                    if corruption:
                        with self.assertRaisesRegex(ValueError, "draft retained"):
                            release.publish(root / "out")
                        self.assertEqual(run.call_count, 1)
                    else:
                        release.publish(root / "out")
                        self.assertEqual(run.call_count, 2)
                        self.assertEqual(run.call_args.args[0], [
                            "gh", "api", f"repos/{release.REPOSITORY}/releases/7", "--method", "PATCH",
                            "-F", "draft=false", "-f", "make_latest=" + ("true" if channel == "stable" else "false")])
                    creation = run.call_args_list[0].args[0]
                    self.assertIn("--draft", creation)
                    self.assertEqual(creation[creation.index("--target") + 1], COMMIT)

    def test_existing_draft_without_git_tag_blocks_creation(self):
        def api(path, **kwargs):
            if path == "releases?per_page=100":
                return [[{"id": 1, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG, "draft": True}]]
            return None

        with patch.object(release, "github", side_effect=api), patch.object(release.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "already exists"):
                release.require_unused_tag(TAG)
            run.assert_not_called()

    def test_release_lookup_paginates_and_rejects_ambiguous_or_invalid_ids(self):
        response = subprocess.CompletedProcess([], 0, json.dumps([
            [{"id": 1, "tag_name": "unrelated"}], [{"id": 7, "tag_name": TAG, "draft": True}]]), "")
        with patch.object(release.subprocess, "run", return_value=response) as run:
            self.assertEqual(release.release_id_for_tag(TAG), 7)
            self.assertEqual(run.call_args.args[0], ["gh", "api", f"repos/{release.REPOSITORY}/releases?per_page=100",
                                                   "--method", "GET", "--paginate", "--slurp"])
        for entries in ([{"tag_name": TAG, "id": 7}] * 2,
                        [{"tag_name": TAG, "id": "7"}], [{"tag_name": TAG, "id": True}],
                        [{"tag_name": TAG, "id": 0}], [{"tag_name": TAG}]):
            with self.subTest(entries=entries), patch.object(release, "github", return_value=[entries]):
                with self.assertRaisesRegex(ValueError, "release identity"):
                    release.release_id_for_tag(TAG)
        with patch.object(release, "github", return_value=[[]]):
            self.assertIsNone(release.release_id_for_tag(TAG))

    def test_failed_draft_discovery_never_publishes(self):
        for failure in (None, RuntimeError("network unavailable")):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures(root / "in")
                self.assemble(root / "in", root / "out")
                with patch.object(release, "context", return_value=(COMMIT, VERSION, TAG)), \
                        patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1"}), \
                        patch.object(release, "require_unused_tag"), \
                        patch.object(release, "release_id_for_tag", return_value=None,
                                     side_effect=failure), \
                        patch.object(release.subprocess, "run") as run:
                    with self.assertRaises((ValueError, RuntimeError)):
                        release.publish(root / "out")
                    self.assertEqual(run.call_count, 1)
                    self.assertIn("create", run.call_args.args[0])

    def test_signing_stage_restores_only_tested_executable_and_refuses_reuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "aarch64-apple-darwin"
            native_fixture(root / "in", target)
            for name in ("Cargo.lock", "rust-toolchain.toml"):
                (root / name).write_bytes((release.ROOT / name).read_bytes())
            with patch.object(release, "ROOT", root):
                release.stage_signing(root / "in", target, COMMIT, VERSION)
                output = root / "target" / target / "release"
                self.assertEqual([p.name for p in output.iterdir()], ["airk"])
                self.assertEqual((output / "airk").read_bytes(), b"original fixture")
                with self.assertRaises(FileExistsError):
                    release.stage_signing(root / "in", target, COMMIT, VERSION)

    def test_network_failure_is_not_mistaken_for_absent_release(self):
        failed = subprocess.CompletedProcess([], 1, "", "network unavailable")
        with patch.object(release.subprocess, "run", return_value=failed), self.assertRaises(RuntimeError):
            release.github("releases/tags/" + TAG, missing=True)


if __name__ == "__main__":
    unittest.main()
