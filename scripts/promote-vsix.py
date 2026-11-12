# SPDX-License-Identifier: Apache-2.0
"""Reuse CI extension bytes; change only channel metadata (and, before the split, native payloads).

Split versions (split_receipts.SPLIT_VERSION and later) ship one universal,
UI-only VSIX: `channel` rewrites only the pre-release property of
extension.vsixmanifest and proves every extension/ byte unchanged. The pre-split
`source`/`package` commands, which splice signed native payloads into platform
VSIXs, remain for historical versions only.
"""
import argparse
import copy
import io
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import stat
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("release_promotion", ROOT / "scripts/github-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def channel_manifest(data, channel):
    """extension.vsixmanifest with only its pre-release property set for `channel`."""
    metadata = ET.fromstring(data)
    properties = next((node for node in metadata.iter() if node.tag.rsplit("}", 1)[-1] == "Properties"), None)
    if properties is None:
        raise ValueError("VSIX channel metadata has no Properties element")
    for node in list(properties):
        if node.get("Id") == "Microsoft.VisualStudio.Code.PreRelease":
            properties.remove(node)
    namespace = properties.tag.rsplit("}", 1)[0] + "}" if "}" in properties.tag else ""
    if channel == "preview":
        ET.SubElement(properties, namespace + "Property", Id="Microsoft.VisualStudio.Code.PreRelease", Value="true")
    return ET.tostring(metadata, encoding="utf-8", xml_declaration=True)


def verified_universal(directory, commit, version, tree=None):
    """The CI universal VSIX and its C7 receipt, rechecked against this source and lock."""
    package = release.extension.package
    if not package.receipts.split_version(version):
        raise ValueError("A universal VSIX requires a split version")
    receipt, archive = release.candidate(directory)
    package.receipts.validate_vsix(receipt)
    if (not package.same_source(receipt, commit, tree) or receipt["version"] != version
            or receipt["extension_id"] != "iokaio.airk"
            or receipt["locks"] != {package.LOCK: release.digest(ROOT / package.LOCK)}
            or archive.name != f"airk-{version}.vsix"):
        raise ValueError("CI universal VSIX differs from this source/lock/version")
    package.verify_vsix(archive, receipt["payload"], version=version, pre_release=receipt["pre_release"],
                        display_name=receipt["display_name"])
    return receipt, archive


def promote_universal(vsix_directory, output, commit, version, channel, tree=None):
    """Copy the tested universal VSIX, rewriting only its channel metadata when needed."""
    if channel not in ("stable", "preview"):
        raise ValueError("Unknown release channel")
    receipt, archive = verified_universal(vsix_directory, commit, version, tree)
    output.mkdir(parents=True, exist_ok=False)
    if receipt["pre_release"] == (channel == "preview"):
        for name in (archive.name, "manifest.json", "SHA256SUMS"):
            shutil.copyfile(vsix_directory / name, output / name)
        return receipt
    destination = output / archive.name
    expected = {"readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name: value
                for name, value in receipt["payload"].items()}
    with zipfile.ZipFile(archive) as original_zip, \
            zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as result:
        for info in original_zip.infolist():
            emitted = copy.copy(info)
            emitted.compress_type = zipfile.ZIP_DEFLATED
            data = original_zip.read(info)
            if info.filename == "extension.vsixmanifest":
                data = channel_manifest(data, channel)
            elif info.filename.startswith("extension/") and expected.get(info.filename.removeprefix("extension/")) != hashlib.sha256(data).hexdigest():
                raise ValueError("CI extension bytes changed during promotion")
            result.writestr(emitted, data)
    release.extension.package.verify_vsix(destination, receipt["payload"], version=version,
                                          pre_release=channel == "preview", display_name=receipt["display_name"])
    # same_source admitted the receipt: an equal commit (its tree stands) or an identical tree.
    receipt = {**receipt, "commit": commit, "tree": tree or receipt["tree"], "pre_release": channel == "preview",
               "archive_sha256": release.digest(destination)}
    release.extension.package.receipts.validate_vsix(receipt)
    (output / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (output / "SHA256SUMS").write_text(f"{release.digest(destination)}  {destination.name}\n"
        f"{release.digest(output / 'manifest.json')}  manifest.json\n", encoding="utf-8")
    return receipt


def pre_split(version):
    if release.extension.package.receipts.split_version(version):
        raise ValueError("Split versions ship one universal VSIX; use the channel command")


def verified_vsix(directory, native_directory, target, commit, version, tree=None):
    pre_split(version)
    receipt, archive = release.candidate(directory)
    original = release.extension.read_receipt(native_directory / "manifest.json")
    if (receipt.get("schema") != "airk-extension-candidate/1"
            or not release.extension.package.same_source(receipt, commit, tree)
            or receipt.get("version") != version or receipt.get("platform") != release.native.TARGETS[target]
            or receipt.get("extension_id") != "iokaio.airk"
            or type(receipt.get("pre_release")) is not bool
            or receipt.get("npm_lock_sha256") != release.digest(ROOT / "packages/vscode/package-lock.json")
            or receipt.get("native_manifest_sha256") != release.digest(native_directory / "manifest.json")
            or receipt.get("native_archive_sha256") != original["archive_sha256"]):
        raise ValueError("CI VSIX differs from its native candidate/source/lock")
    release.extension.package.verify_vsix(archive, receipt["payload"], platform=receipt["platform"], version=version,
                                          pre_release=receipt["pre_release"], display_name=receipt["display_name"])
    with zipfile.ZipFile(archive) as zipped:
        raw = zipped.read("extension/native/candidate.json")
        if hashlib.sha256(raw).hexdigest() != receipt["native_manifest_sha256"]:
            raise ValueError("Embedded CI native receipt differs")
        for name, expected in original["payload"].items():
            if hashlib.sha256(zipped.read("extension/native/runtime/" + name)).hexdigest() != expected:
                raise ValueError("Embedded CI native bytes differ")
    return receipt, archive


def promote(vsix_directory, original_directory, native_directory, output, target, commit, version, channel, tree=None):
    original, _ = release.verify_native(original_directory, target, commit, version, False, tree=tree)
    receipt, archive = verified_vsix(vsix_directory, original_directory, target, commit, version, tree)
    signed = target in release.SIGNED_TARGETS
    native, native_archive = release.verify_native(native_directory, target, commit, version, signed, tree=tree)
    if signed:
        if (native["signing"]["source_install_sha256"] != original["payload"]["airk-install.json"]
                or any(row["source_sha256"] != original["payload"][name]
                       for name, row in native["signing"]["components"].items())):
            raise ValueError("Signed VSIX payload did not originate in this CI candidate")
    elif native != original:
        raise ValueError("Linux native payload must remain unchanged")
    if channel not in ("stable", "preview"):
        raise ValueError("Unknown release channel")
    output.mkdir(parents=True, exist_ok=False)
    if not signed and receipt["pre_release"] == (channel == "preview"):
        for name in (archive.name, "manifest.json", "SHA256SUMS"):
            shutil.copyfile(vsix_directory / name, output / name)
        return receipt
    destination = output / archive.name
    payload = {}
    expected = {"readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name: value
                for name, value in receipt["payload"].items()}
    with zipfile.ZipFile(archive) as original_zip, zipfile.ZipFile(native_archive) as native_zip, \
            zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as result:
        update_inventory = release.extension.package.read_update_inventory(original_zip, version, receipt["platform"])
        def write(info, source, name=None):
            emitted = copy.copy(info)
            emitted.filename = name or info.filename
            emitted.compress_type = zipfile.ZIP_DEFLATED
            hashed = hashlib.sha256()
            with result.open(emitted, "w") as target_stream:
                while block := source.read(1_048_576):
                    hashed.update(block)
                    target_stream.write(block)
            value = hashed.hexdigest()
            if emitted.filename.startswith("extension/"):
                payload[emitted.filename.removeprefix("extension/")] = value
            return value

        for info in original_zip.infolist():
            name = info.filename
            if signed and name.startswith("extension/native/"):
                continue
            if signed and update_inventory is not None and name == "extension/airk-update.json":
                if release.extension.package.digest(original_zip.read(info)) != expected["airk-update.json"]:
                    raise ValueError("CI extension update inventory changed during promotion")
                # The signed native wrapper is the only loader-consumed asset
                # replaced below. Preserve every reviewed host/UI/parser digest.
                continue
            if name == "extension.vsixmanifest" and receipt["pre_release"] != (channel == "preview"):
                write(info, io.BytesIO(channel_manifest(original_zip.read(info), channel)))
            else:
                with original_zip.open(info) as source:
                    value = write(info, source)
                if name.startswith("extension/") and expected.get(name.removeprefix("extension/")) != value:
                    raise ValueError("CI extension bytes changed during promotion")
        if signed:
            for info in native_zip.infolist():
                with native_zip.open(info) as source:
                    value = write(info, source, "extension/native/runtime/" + info.filename)
                if native["payload"].get(info.filename) != value:
                    raise ValueError("Signed native bytes changed during promotion")
            operating_system = "windows" if "windows" in target else "macos"
            executables = release.native.runtime_builder().runtime_executables(operating_system, version)
            wrapper = {"schema": 2, "version": version, "vsix_target": native["vsix_target"],
                       "runtime_dir": "runtime", "install_manifest_sha256": native["install_manifest_sha256"],
                       "payload": {name: native["payload"][name] for name in executables}}
            for name, data in (("extension/native/manifest.json", json.dumps(wrapper).encode()),
                               ("extension/native/candidate.json", (native_directory / "manifest.json").read_bytes())):
                info = zipfile.ZipInfo(name)
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                write(info, io.BytesIO(data))
            if update_inventory is not None:
                updated_files = dict(update_inventory["files"])
                updated_files["native/manifest.json"] = payload["native/manifest.json"]
                data = release.extension.package.encode_update_inventory(
                    updated_files, update_inventory["version"], update_inventory["package_manifest"])
                info = zipfile.ZipInfo("extension/airk-update.json")
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                write(info, io.BytesIO(data))
    release.extension.package.verify_vsix(destination, payload, platform=native["vsix_target"], version=version,
                                          pre_release=channel == "preview", display_name=receipt["display_name"])
    receipt = {**receipt, "commit": commit, "tree": tree, "pre_release": channel == "preview",
               "native_archive_sha256": native["archive_sha256"],
               "native_manifest_sha256": release.digest(native_directory / "manifest.json"),
               "archive_sha256": release.digest(destination), "payload": payload}
    (output / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (output / "SHA256SUMS").write_text(f"{release.digest(destination)}  {destination.name}\n"
        f"{release.digest(output / 'manifest.json')}  manifest.json\n", encoding="utf-8")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("channel", "source", "package"),
                        help="channel: split universal VSIX; source/package: pre-split platform VSIXs")
    parser.add_argument("--native", type=Path)
    parser.add_argument("--vsix", type=Path, required=True)
    parser.add_argument("--original", type=Path)
    parser.add_argument("--record", type=Path, default=Path("artifacts/producer.json"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", choices=release.native.TARGETS)
    parser.add_argument("--channel", choices=("stable", "preview"), default="stable")
    args = parser.parse_args()
    commit, version, _ = release.context()
    tree = release.extension.package.source_tree(commit)
    if args.command == "channel":
        promote_universal(args.vsix, args.output, commit, version, args.channel, tree)
        return
    if args.native is None or args.target is None:
        parser.error("source and package require --native and --target")
    pre_split(version)
    if args.command == "source":
        release.verify_native(args.native, args.target, commit, version, False, tree=tree)
        verified_vsix(args.vsix, args.native, args.target, commit, version, tree)
        producer = release.extension.read_receipt(args.record)
        release.write_source_record(args.output, args.native, producer, args.target, commit)
    else:
        promote(args.vsix, args.original or args.native, args.native, args.output,
                args.target, commit, version, args.channel, tree)


if __name__ == "__main__":
    main()
