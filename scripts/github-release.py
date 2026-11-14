# SPDX-License-Identifier: Apache-2.0
"""Verify, assemble and publish versioned AIRK GitHub Releases."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


extension = module("extension_release", "verify-extension-release.py")
native = module("native_package", "package-native.py")
signer = module("native_signer", "sign-native.py")
producers = module("ci_candidates", "ci-candidates.py")
reads = module("github_read", "github-read.py")
authority = module("release_authority", "release-authority.py")
standalone = module("standalone_contract", "standalone-installer-contract.py")
publication = module("publication_contract", "publication-contract.py")
optional = module("optional_publication", "optional-publication.py")
# The C7 owner of the split threshold, receipt validators and release catalog names.
receipts = extension.package.receipts
SIGNED_TARGETS = {"x86_64-pc-windows-msvc", "aarch64-apple-darwin", "x86_64-apple-darwin"}
REPOSITORY = "iokaio/airk"
WORKFLOW = REPOSITORY + "/.github/workflows/github-release.yml@refs/heads/main"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(version):
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("Expected a numeric checked-in package version")
    return f"v{version}"


def context():
    campaign = os.environ.get("GITHUB_REF") == "refs/heads/" + authority.CAMPAIGN_BRANCH
    if campaign:
        authority.manual_context(os.environ, "github-release.yml")
    elif (os.environ.get("GITHUB_REPOSITORY") != REPOSITORY
          or os.environ.get("GITHUB_REF") != "refs/heads/main"
          or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"
          or os.environ.get("GITHUB_WORKFLOW_REF") != WORKFLOW):
        raise ValueError("Release requires the manual AIRK main-branch or qualified campaign workflow")
    commit = os.environ["GITHUB_SHA"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected full source commit")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if campaign and not receipts.split_version(version):
        raise ValueError("Campaign release requires the split product catalog")
    if json.loads((ROOT / "packages/vscode/package.json").read_text(encoding="utf-8"))["version"] != version:
        raise ValueError("Native and extension source versions differ")
    tag = identity(version)
    for field in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"):
        if not re.fullmatch(r"[1-9]\d*", os.environ[field]):
            raise ValueError("Invalid workflow run identity")
    return commit, version, tag


def require_campaign_producer(producer):
    if os.environ.get("GITHUB_REF") == "refs/heads/" + authority.CAMPAIGN_BRANCH:
        authority.manual_context(os.environ, "github-release.yml")
        if not isinstance(producer, dict) or authority.record_branch(producer) != authority.CAMPAIGN_BRANCH:
            raise ValueError("Campaign requires its explicit full campaign producer")


def candidate(directory):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("Missing or linked candidate directory")
    receipt = extension.read_receipt(directory / "manifest.json")
    name = receipt["archive"]
    extension.package.safe_name(name)
    if "/" in name:
        raise ValueError("Candidate archive must be a filename")
    if set(p.name for p in directory.iterdir()) != {name, "manifest.json", "SHA256SUMS"}:
        raise ValueError("Unexpected candidate files")
    if any(p.is_symlink() or not p.is_file() for p in directory.iterdir()):
        raise ValueError("Linked or non-file candidate payload")
    archive = directory / name
    if digest(archive) != receipt["archive_sha256"]:
        raise ValueError("Candidate archive digest mismatch")
    expected = f'{digest(archive)}  {name}\n{digest(directory / "manifest.json")}  manifest.json\n'
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != expected:
        raise ValueError("Candidate checksums differ")
    return receipt, archive


def verify_native(directory, target, commit, version, signed, *, tree=None):
    receipt, archive = candidate(directory)
    # Split versions package the airk + airkctl pair with the admin identity (C7, no vsix_target).
    split = receipts.split_version(version)
    paired = split or tuple(int(part) for part in version.split(".")[:2]) >= (1, 4)
    schema = ("airk-native-candidate/3" if split else "airk-native-candidate/2" if paired
              else "airk-native-candidate/1")
    if (receipt.get("schema") != schema or receipt.get("target") != target
            or ("vsix_target" in receipt if split else receipt.get("vsix_target") != native.TARGETS[target])
            or not extension.package.same_source(receipt, commit, tree) or receipt.get("version") != version
            or receipt.get("dirty_checkout") is not False
            or receipt.get("cargo_lock_sha256") != digest(ROOT / "Cargo.lock")
            or receipt.get("toolchain_file_sha256") != digest(ROOT / "rust-toolchain.toml")
            or receipt.get("signed_distribution") is not signed
            or archive.name != f"airk-{version}-{target}.zip"):
        raise ValueError("Native candidate provenance or signing state differs")
    if split:
        try:
            receipts.validate_native_candidate(receipt)
        except ValueError as error:
            raise ValueError(f"Native candidate differs from the split receipt contract: {error}") from error
    os_name = "windows" if "windows" in target else "macos" if "apple" in target else "linux"
    builder = native.runtime_builder()
    executables = builder.runtime_executables(os_name, version) if paired else ("airk.exe" if "windows" in target else "airk",)
    executable = executables[0]
    required = {*executables, "airk-runtime.json", "airk-build.json", "airk-install.json", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"} if paired else {executable, "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
    bundle_files = builder.macos_bundle_files(version) if paired and os_name == "macos" else {}
    required.update(bundle_files)
    if paired:
        required.add(builder.launcher_name(os_name, version))
    inference_assets = builder.inference_assets(os_name, "aarch64" if target.startswith("aarch64-") else "x86_64", version) if builder.int8_version(version) else {}
    required.update(inference_assets)
    if builder.int8_version(version):
        builder.load_script("int8-policy").verify_embedded(receipt.get("embedded_models", {}))
    bundle_signatures = {"AIRK.app/Contents/_CodeSignature/CodeResources", "AIRK.app/Contents/CodeResources"} if bundle_files else set()
    if signed:
        required.update(bundle_signatures)
    payload = receipt["payload"]
    if not required.issubset(payload):
        raise ValueError("Native legal files or executable missing")
    for name in payload:
        extension.package.safe_name(name)
        if name not in required | bundle_signatures and not name.startswith("notices/"):
            raise ValueError("Unexpected native payload path")
    with zipfile.ZipFile(archive) as zipped:
        if len(zipped.infolist()) > 15_000 or sum(p.file_size for p in zipped.infolist()) > 4_294_967_296:
            raise ValueError("Native archive exceeds verification budget")
        if any(stat.S_ISLNK(p.external_attr >> 16) or p.is_dir() for p in zipped.infolist()):
            raise ValueError("Native archive links/directories rejected")
        if builder.single_image_version(version):
            notice_name = "notices/static-onnx-runtime/inventory.json"
            if zipped.getinfo(notice_name).file_size > 524_288:
                raise ValueError("Static ORT notice inventory exceeds bound")
            builder.static_notice_inventory(json.loads(zipped.read(notice_name)), target, payload)
        for name, expected in bundle_files.items():
            if zipped.read(name) != expected:
                raise ValueError("Mac service bundle resources differ")
        for name, row in inference_assets.items():
            if row.get("macho_payload_sha256"):
                if zipped.getinfo(name).file_size > 64 << 20 or builder.load_script("macho-identity").payload_digest(zipped.read(name)) != row["macho_payload_sha256"]:
                    raise ValueError("Signed inference library content differs")
            elif payload.get(name) != row["sha256"]:
                raise ValueError("Inference library/notice digest differs")
        if paired:
            install = json.loads(zipped.read("airk-install.json"))
            pair = json.loads(zipped.read("airk-runtime.json"))
            build = json.loads(zipped.read("airk-build.json"))
            architecture = "aarch64" if target.startswith("aarch64-") else "x86_64"
            if (install.get("schema") != 1 or install.get("version") != version or install.get("os") != os_name or install.get("arch") != architecture
                    or not builder.runtime_metadata_matches(pair, version, os_name, architecture, payload)):
                raise ValueError("Native pair installer platform/version differs")
            if (install.get("files") != {name: value for name, value in payload.items() if name != "airk-install.json"}
                    or receipt.get("install_manifest_sha256") != payload["airk-install.json"]
                    or build.get("schema") != (2 if split else 1) or build.get("runtime") != pair
                    or build.get("source") != receipt.get("source")):
                raise ValueError("Native pair build/installer identities differ")
            if split:
                try:
                    receipts.validate_build_receipt(build)
                except ValueError as error:
                    raise ValueError(f"Native pair build receipt differs from the split contract: {error}") from error
                # The embedded admin shell and interface versions are one identity across receipts.
                admin = build["admin_assets"]
                if (receipt["admin_assets"] != {"inventory_sha256": admin["inventory_sha256"],
                                                "bundle_sha256": admin["bundle_sha256"]}
                        or pair["admin_inventory_sha256"] != admin["inventory_sha256"]
                        or (receipt["i1_protocol"], receipt["i2_contract"]) != (pair["i1_protocol"], pair["i2_contract"])
                        or (receipt["airk_sha256"], receipt["airkctl_sha256"]) != (pair["airk_sha256"], pair["airkctl_sha256"])):
                    raise ValueError("Native pair admin, interface or image identities differ")
    executable_names = tuple(dict.fromkeys((*executables, builder.launcher_name(os_name, version)))) if paired else executable
    native.verify_archive(archive, payload, executable_names)
    signing = receipt.get("signing")
    if signed:
        if paired:
            if (not isinstance(signing, dict) or signing.get("schema") != "airk-service-pair-signing/1"
                    or signing.get("target") != target or signing.get("commit") != commit
                    or signing.get("cargo_lock_sha256") != receipt["cargo_lock_sha256"]
                    or signing.get("install_manifest_sha256") != payload["airk-install.json"]
                    or not re.fullmatch(r"[0-9a-f]{64}", signing.get("source_install_sha256", ""))
                    or set(signing.get("components", {})) != set(builder.signing_names(os_name, version))):
                raise ValueError("Pair signing receipt does not bind this build")
            components = signing["components"]
            if bundle_files and not signer.stapled_app_ticket(components):
                raise ValueError("Mac service pair lacks matching stapled app notarization")
            if os_name == "macos" and builder.standalone_launcher_version(version):
                launcher = builder.launcher_name(os_name, version)
                expected_copy = {"schema": 1, "binary_sha256": payload[launcher], "version": version,
                                 "strict_signature": True, "standalone_version_probe": True}
                copied = signing.get("copied_launcher")
                # This is the signer's exact post-seal observation, not a new OS
                # signature assertion on the Linux assembly/Marketplace host.
                if (not isinstance(copied, dict) or copied != expected_copy
                        or type(copied.get("schema")) is not int
                        or copied.get("strict_signature") is not True
                        or copied.get("standalone_version_probe") is not True):
                    raise ValueError("Mac service pair lacks matching standalone copied-launcher evidence")
        else:
            components = {executable: signing}
        for component, signing in components.items():
            if (not isinstance(signing, dict) or signing.get("schema") != "airk-native-signing/1"
                    or signing.get("target") != target or signing.get("commit") != commit
                    or signing.get("cargo_lock_sha256") != receipt["cargo_lock_sha256"]
                    or signing.get("binary_sha256") != payload[component]
                    or not re.fullmatch(r"[0-9a-f]{64}", signing.get("source_sha256", ""))):
                raise ValueError("Signing receipt does not bind this build")
            verification = signing.get("verification", {})
            if verification.get("timestamp_present") is not True:
                raise ValueError("Trusted signing timestamp missing")
            if "apple" in target:
                if (signing.get("platform") != "Darwin" or signing.get("team_id") != "5PX7D99PXA"
                        or verification.get("trust") != "apple_developer_id"
                        or verification.get("team_id") != "5PX7D99PXA"
                        or verification.get("hardened_runtime") is not True
                        or (signing.get("notarization") or {}).get("status") != "Accepted"
                        or not (signing.get("notarization") or {}).get("id")):
                    raise ValueError("Apple signature or accepted notarization missing")
            elif (signing.get("platform") != "Windows" or signing.get("azure") != signer.AZURE_SIGNING
                  or verification.get("trust") != "authenticode_valid"
                  or not re.fullmatch(r"[0-9a-fA-F]{40}", signing.get("identity", ""))
                  or verification.get("signer_thumbprint") != signing["identity"]):
                raise ValueError("Azure Authenticode identity missing")
    elif signing is not None:
        raise ValueError("Unsigned native candidate contains signing metadata")
    return receipt, archive


def stage_signing(directory, target, commit, version, output_root=None, *, tree=None):
    """Restore only the tested executable, never arbitrary artifact paths."""
    if target not in SIGNED_TARGETS:
        raise ValueError("Unsupported signing target")
    receipt, archive = verify_native(directory, target, commit, version, False, tree=tree)
    paired = receipt["schema"] in ("airk-native-candidate/2", "airk-native-candidate/3")
    executable = "airk.exe" if "windows" in target else "airk"
    executables = native.runtime_builder().runtime_executables("windows" if "windows" in target else "macos", version) if paired else (executable,)
    if paired:
        builder = native.runtime_builder()
        executables = tuple(dict.fromkeys((*executables, builder.launcher_name("windows" if "windows" in target else "macos", version))))
    root = ROOT if output_root is None else output_root
    output = root / "artifacts" / "runtime" / target if paired else root / "target" / target / "release"
    output.mkdir(parents=True, exist_ok=False)
    with zipfile.ZipFile(archive) as zipped:
        for name in receipt["payload"] if paired else (executable,):
            destination = output / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(zipped.read(name))
    for name in executables:
        (output / name).chmod(0o755)


def interface_previews(version, commit):
    if tuple(map(int, version.split("."))) < (1, 7, 1):
        return ""
    base = f"https://raw.githubusercontent.com/{REPOSITORY}/{commit}/docs"
    return ("\n## Workbench and Explorer\n\n"
            "Collect cited evidence directly in Workbench, or inspect sources and manage indexing in Explorer. "
            "Workbench uses no generative AI; supported intent parsing and semantic retrieval use local models.\n\n"
            f"![AIRK Workbench]({base}/workbench/images/workbench-1.7.1-results.png)\n\n"
            f"![AIRK Explorer]({base}/explorer/images/explorer-1.7.1.png)\n\n"
            "Compiled UI previews with fictional project data; installed-editor qualification is separate.\n")


def producer_fingerprint(producer):
    return hashlib.sha256(json.dumps(producer, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def write_source_record(output, directory, producer, target, commit):
    raw = (directory / "manifest.json").read_bytes().decode("utf-8")
    # The verified candidate's own version selects the producer catalog it must belong to.
    producers.validate_record(producer, commit, **producers.catalog_args(json.loads(raw).get("version")))
    name = producers.native_artifact(producer, target)
    record = {"schema": "airk-release-source/1", "commit": commit, "target": target,
              "producer_sha256": producer_fingerprint(producer), "artifact": producer["artifacts"][name],
              "manifest_sha256": hashlib.sha256(raw.encode()).hexdigest(), "manifest_json": raw}
    output.mkdir(parents=True, exist_ok=False)
    (output / "source.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")


def write_split_source_record(directory, output, producer, target, commit, version, *, tree=None):
    """Split versions: the source record binds the tested native pair alone (no VSIX is per target)."""
    require_campaign_producer(producer)
    if not receipts.split_version(version):
        raise ValueError("Pre-split source records also bind the platform VSIX (promote-vsix.py source)")
    verify_native(directory, target, commit, version, False, tree=tree)
    write_source_record(output, directory, producer, target, commit)


def source_receipt(directory, producer, target, commit, version, tree=None):
    # A protected platform job emits this after verifying the complete pinned
    # native/VSIX artifacts. Assembly downloads it from the same release run.
    record = extension.read_receipt(directory / "source.json")
    name = producers.native_artifact(producer, target)
    raw = record.get("manifest_json", "")
    if (record.get("schema") != "airk-release-source/1" or record.get("commit") != commit
            or record.get("target") != target or record.get("producer_sha256") != producer_fingerprint(producer)
            or record.get("artifact") != producer["artifacts"][name]
            or not isinstance(raw, str) or hashlib.sha256(raw.encode()).hexdigest() != record.get("manifest_sha256")):
        raise ValueError("Source receipt differs from this release's pinned producer")
    original = json.loads(raw)
    split = receipts.split_version(version)
    if split:
        try:
            receipts.validate_native_candidate(original)
        except ValueError as error:
            raise ValueError(f"Original native receipt differs from the split contract: {error}") from error
    if (original.get("schema") != ("airk-native-candidate/3" if split else "airk-native-candidate/2")
            or original.get("target") != target
            or ("vsix_target" in original if split else original.get("vsix_target") != native.TARGETS[target])
            or original.get("version") != version
            or not extension.package.same_source(original, commit, tree) or original.get("dirty_checkout") is not False
            or original.get("signed_distribution") is not False or original.get("signing") is not None
            or original.get("cargo_lock_sha256") != digest(ROOT / "Cargo.lock")
            or original.get("toolchain_file_sha256") != digest(ROOT / "rust-toolchain.toml")):
        raise ValueError("Original native receipt has different provenance")
    return original


def verify_installer(directory, target, commit, version, runtime, *, published=False):
    """Recheck sealed bytes and native/signing provenance from the platform job.

    Windows/Mac OS signature checks happen on their native signing runners. The
    assembly host validates their exact retained receipts and final artifact hash.
    """
    name = standalone.artifact(version, target)
    manifest_name = standalone.manifest(version, target) if published else "installer.json"
    if (directory.is_symlink() or not directory.is_dir()
            or not published and {path.name for path in directory.iterdir()} != {name, manifest_name, "SHA256SUMS"}
            or any((directory / entry).is_symlink() or not (directory / entry).is_file()
                   for entry in (name, manifest_name, "SHA256SUMS"))):
        raise ValueError("Unexpected standalone installer files")
    receipt = extension.read_receipt(directory / manifest_name)
    archive = directory / name
    metadata = archive.stat()
    if not 0 < metadata.st_size <= 4_294_967_296 or metadata.st_nlink != 1:
        raise ValueError("Linked, empty or oversized standalone installer")
    if (receipt.get("schema") != "airk-standalone-installer/1"
            or receipt.get("version") != version or receipt.get("target") != target
            or receipt.get("commit") != commit
            or receipt.get("cargo_lock_sha256") != digest(ROOT / "Cargo.lock")
            or receipt.get("runtime_install_sha256") != runtime.get("install_manifest_sha256")
            or receipt.get("runtime_source") != runtime.get("source")
            or receipt.get("artifact") != name or receipt.get("artifact_sha256") != digest(archive)
            or receipt.get("installer_kind") != standalone.TARGETS[target][2]):
        raise ValueError("Standalone installer source, runtime or artifact identity differs")
    expected = f'{digest(archive)}  {name}\n{digest(directory / manifest_name)}  {manifest_name}\n'
    if not published and (directory / "SHA256SUMS").read_text(encoding="utf-8") != expected:
        raise ValueError("Standalone installer checksums differ")
    payload = receipt.get("payload")
    if not isinstance(payload, dict) or not 0 < len(payload) <= 15_000:
        raise ValueError("Invalid standalone installer payload roster")
    for path, value in payload.items():
        extension.package.safe_name(path)
        if not re.fullmatch(r"[0-9a-f]{64}", value):
            raise ValueError("Invalid standalone installer payload digest")
    split = receipts.split_version(version)
    if split:
        service = split_service_payload(receipt, target, runtime, payload)
        wrappers = set(payload) - set(service)
    else:
        if {path.removeprefix("runtime/"): value for path, value in payload.items()
                if path.startswith("runtime/")} != runtime["payload"]:
            raise ValueError("Standalone installer contains a different native runtime")
        wrappers = set(payload) - {"runtime/" + path for path in runtime["payload"]}
    if split and "windows" not in target:
        # The pinned roster of every payload/ file; install.sh / Install AIRK.command check it first.
        sums = "".join(f"{payload[path]}  {path}\n" for path in sorted(service)).encode()
        if payload.get("payload.sha256") != hashlib.sha256(sums).hexdigest():
            raise ValueError("Standalone installer payload roster differs from its payload")
    if "windows" in target:
        # Split: the Inno license sits inside the placed notices/ (split_service_payload).
        if wrappers != (set() if split else {"notices/inno-setup/LICENSE.txt"}):
            raise ValueError("Unexpected Windows installer payload files")
        signing, uninstaller = receipt.get("signing"), receipt.get("uninstaller")
        if not isinstance(signing, dict) or not isinstance(uninstaller, dict):
            raise ValueError("Standalone Windows installer lacks signature evidence")
        for item in (signing, uninstaller):
            identity = item.get("identity", "")
            verification = item.get("verification", {})
            if (item.get("azure") != signer.AZURE_SIGNING
                    or not re.fullmatch(r"[0-9a-fA-F]{40}", identity)
                    or verification.get("trust") != "authenticode_valid"
                    or verification.get("timestamp_present") is not True
                    or verification.get("signer_thumbprint") != identity):
                raise ValueError("Standalone Windows Authenticode evidence differs")
        if (signing.get("platform") != "Windows" or signing.get("binary_sha256") != digest(archive)
                or not re.fullmatch(r"[0-9a-f]{64}", uninstaller.get("unsigned_sha256", ""))
                or not re.fullmatch(r"[0-9a-f]{64}", uninstaller.get("binary_sha256", ""))
                or receipt.get("installer_source_sha256") != digest(windows_installer_source(version))
                or not re.fullmatch(r"6\.7\.1(?:\.\d+)?", receipt.get("compiler", {}).get("version", ""))
                or not re.fullmatch(r"[0-9a-f]{64}", receipt.get("compiler", {}).get("sha256", ""))):
            raise ValueError("Standalone Windows installer/uninstaller provenance differs")
    elif "apple" in target:
        signing = receipt.get("signing", {})
        notarization = signing.get("notarization", {})
        mac_wrappers = {"Install AIRK.command", "README.txt", *(("payload.sha256",) if split else ())}
        if (wrappers != mac_wrappers
                or receipt.get("runtime_signing") != runtime.get("signing")
                or not re.fullmatch(r"[0-9a-f]{64}", receipt.get("runtime_signing_sha256", ""))
                or signing.get("platform") != "Darwin" or signing.get("trust") != "apple_developer_id"
                or not re.fullmatch(r"[0-9a-f]{40}", signing.get("identity", ""))
                or signing.get("team_id") != "5PX7D99PXA"
                or signing.get("certificate_sha256") != "4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1"
                or signing.get("timestamp_present") is not True
                or notarization.get("status") != "Accepted" or not notarization.get("id")
                or notarization.get("stapled") is not True or notarization.get("scope") != name):
            raise ValueError("Standalone Mac signature, payload or notarization evidence differs")
        mac = module("standalone_macos_package", "package-standalone-macos.py")
        if split:
            bootstrap = mac.split_installer_files(version, target, receipt["payload_inventory_sha256"],
                                                  payload["payload.sha256"],
                                                  payload["payload/AIRK.app/Contents/MacOS/airk"], len(service))
        else:
            bootstrap = mac.installer_files(version, target, receipt["runtime_install_sha256"],
                                            runtime["payload"]["AIRK.app/Contents/MacOS/airk"])
        for path, encoded in bootstrap.items():
            if payload[path] != hashlib.sha256(encoded).hexdigest():
                raise ValueError("Standalone Mac installer bootstrap differs from reviewed source")
    else:
        if (wrappers != {"install.sh", "README.txt", "payload.sha256"}
                or receipt.get("signing") != {"platform": "Linux", "trust": "unsigned"}):
            raise ValueError("Standalone Linux installer must retain explicit unsigned provenance")
        linux = module("standalone_linux_package", "package-standalone-linux.py")
        values = {"VERSION": version, "ARCHIVE_NAME": name, "DIRECTORY_NAME": name.removesuffix(".tar.gz")}
        if split:
            values.update(PAYLOAD_FILE_COUNT=len(service), PAYLOAD_SUMS_SHA256=payload["payload.sha256"],
                          INVENTORY_SHA256=receipt["payload_inventory_sha256"],
                          AIRK_SHA256=payload["payload/service/airk"])
            expected_wrappers = {}
        else:
            sums = "".join(f"{value}  runtime/{path}\n" for path, value in sorted(runtime["payload"].items()))
            values.update(PAYLOAD_FILE_COUNT=len(runtime["payload"]),
                          INSTALL_MANIFEST_SHA256=receipt["runtime_install_sha256"],
                          PAYLOAD_SUMS_SHA256=hashlib.sha256(sums.encode()).hexdigest())
            expected_wrappers = {"payload.sha256": sums.encode()}
        expected_wrappers.update({path: linux.render_template(path + ".in", values).encode()
                                  for path in ("install.sh", "README.txt")})
        if any(payload[path] != hashlib.sha256(encoded).hexdigest() for path, encoded in expected_wrappers.items()):
            raise ValueError("Standalone Linux installer bootstrap differs from reviewed source")
        if receipt.get("native_archive_sha256") != runtime["archive_sha256"]:
            raise ValueError("Standalone Linux native archive identity differs")
        linux.verify_installer(archive, payload, name.removesuffix(".tar.gz"))
        if split:
            verify_linux_inventory(archive, name.removesuffix(".tar.gz"), version, runtime, service, linux)
    return receipt, archive


def windows_installer_source(version):
    """The reviewed Inno script per version: the split AIRK Service installer or the pre-split one."""
    return ROOT / "installers/windows" / ("airk-service.iss" if receipts.split_version(version) else "airk.iss")


SERVICE_PAYLOAD = "airk-service-payload/1"


def split_service_payload(receipt, target, runtime, payload):
    """The installer's payload/ roster: the plan §9.5 layout of the tested (signed) pair, its
    pinned `airk-service-payload/1` inventory and, on Windows, the Inno Setup license notice."""
    linux = module("standalone_linux_layout", "package-standalone-linux.py")
    os_name = receipts.platform_of(target)[0]
    expected = {"payload/" + placed: runtime["payload"][name]
                for placed, name in linux.layout_names(runtime["payload"], os_name).items()}
    inventory = receipt.get("payload_inventory_sha256")
    if (receipt.get("payload_schema") != SERVICE_PAYLOAD or not isinstance(inventory, str)
            or not re.fullmatch(r"[0-9a-f]{64}", inventory) or payload.get("payload/payload-inventory.json") != inventory):
        raise ValueError("Split installer lacks its pinned service payload inventory")
    service = {path: value for path, value in payload.items() if path.startswith("payload/")}
    extra = {"payload/payload-inventory.json", *(("payload/notices/inno-setup/LICENSE.txt",) if os_name == "windows" else ())}
    if not extra <= set(service) or {path: value for path, value in service.items() if path not in extra} != expected:
        raise ValueError("Standalone installer contains a different native runtime")
    return service


def verify_linux_inventory(archive, directory_name, version, runtime, service, linux):
    """The tar's payload-inventory.json describes exactly the placed files, images and admin shell."""
    with tarfile.open(archive, "r|gz") as packaged:
        for member in packaged:
            if member.name == directory_name + "/payload/payload-inventory.json":
                if not member.isfile() or member.size > 1 << 20:
                    raise ValueError("Linux service payload inventory exceeds its bound")
                inventory = json.loads(packaged.extractfile(member).read())
                break
        else:
            raise ValueError("Linux installer lacks its service payload inventory")
    files = inventory.get("files") if isinstance(inventory, dict) else None
    expected = {path.removeprefix("payload/"): value for path, value in service.items()
                if path != "payload/payload-inventory.json"}
    if (not isinstance(files, dict) or set(files) != set(expected)
            or any(not isinstance(row, dict) or set(row) != {"sha256", "bytes"} or row["sha256"] != expected[path]
                   or type(row["bytes"]) is not int or row["bytes"] < 0 for path, row in files.items())
            or {key: value for key, value in inventory.items() if key != "files"} != {
                "schema": SERVICE_PAYLOAD, "version": version, "channel": "stable", "target": linux.TARGET,
                "executables": linux.SERVICE_IMAGES, "admin_assets": runtime["admin_assets"],
                "signing": linux.LINUX_SIGNING}):
        raise ValueError("Linux service payload inventory differs from the tested pair")


def assemble(directory, output, commit, version, tag, channel, run_id, attempt, producer=None, repair=None, *, tree=None, pinned=False, source_records=False):
    if tag != identity(version) or channel not in ("preview", "stable"):
        raise ValueError("Invalid release identity/channel")
    if source_records and producer is None:
        raise ValueError("Source receipts require a pinned producer")
    if receipts.split_version(version):
        if repair is not None:
            raise ValueError("Release-tooling repairs are pinned to their historical catalog")
        return assemble_split(directory, output, commit, version, tag, channel, run_id, attempt, producer,
                              tree=tree, pinned=pinned, source_records=source_records)
    suffix = f"{commit}-{run_id}-{attempt}" if producer is None else f"{commit}-{run_id}"
    if producer is not None:
        (producers.validate_record if pinned else producers.verify_record)(producer, commit)

    def original_dir(target):
        if producer is None:
            return directory / f"airk-native-{target}-{suffix}"
        if producer.get("schema") == "airk-ci-producer/2":
            # Promoted PR artifacts keep their producer's merge-ref name.
            return directory / producers.native_artifact(producer, target)
        return directory / f"airk-native-{target}-{commit}-{producer['run_id']}-{producer['attempt']}"
    def original_receipt(target):
        if source_records:
            return source_receipt(directory / f"airk-source-{target}-{commit}-{run_id}",
                                  producer, target, commit, version, tree)
        return verify_native(original_dir(target), target, commit, version, False, tree=tree)[0]

    selected = []
    extensions = []
    for target in native.TARGETS:
        prefix = "airk-signed" if target in SIGNED_TARGETS else "airk"
        native_dir = directory / f"{prefix}-native-{target}-{suffix}"
        vsix_dir = directory / f"{prefix}-vsix-{target}-{suffix}"
        receipt, archive = verify_native(native_dir, target, commit, version, target in SIGNED_TARGETS, tree=tree)
        vsix_receipt, vsix = candidate(vsix_dir)
        if (vsix_receipt["native_manifest_sha256"] != digest(native_dir / "manifest.json")
                or vsix_receipt["native_archive_sha256"] != receipt["archive_sha256"]):
            raise ValueError("VSIX and native download are not the same build")
        if target in SIGNED_TARGETS:
            original = original_receipt(target)
            executable = "airk.exe" if "windows" in target else "airk"
            if receipt["schema"] == "airk-native-candidate/2":
                if receipt["signing"]["source_install_sha256"] != original["payload"]["airk-install.json"]:
                    raise ValueError("Signed pair did not originate in the tested candidate")
                for name, component in receipt["signing"]["components"].items():
                    if component["source_sha256"] != original["payload"][name]:
                        raise ValueError("Signed component did not originate in the tested candidate")
            elif receipt["signing"]["source_sha256"] != original["payload"][executable]:
                raise ValueError("Signed binary did not originate in the tested candidate")
        elif producer is not None:
            original = original_receipt(target)
            if receipt != original:
                raise ValueError("Linux native package differs from the tested producer")
        extensions.append(vsix_dir / "manifest.json")
        selected.extend(((archive, archive.name), (vsix, vsix.name),
                         (native_dir / "manifest.json", f"airk-{version}-{target}.native-manifest.json"),
                         (vsix_dir / "manifest.json", f"airk-{version}-{native.TARGETS[target]}.vsix-manifest.json")))
        if standalone.required(version, channel):
            installer_dir = directory / f"airk-installer-{target}-{commit}-{run_id}"
            _, installer = verify_installer(installer_dir, target, commit, version, receipt)
            selected.extend(((installer, installer.name),
                             (installer_dir / "installer.json", standalone.manifest(version, target))))
    extension.verify(directory, commit, version, channel, receipts=extensions, tree=tree)
    output.mkdir(parents=True, exist_ok=False)
    for source, name in selected:
        shutil.copyfile(source, output / name)
    record = {"schema": "airk-github-release/1", "tag": tag, "package_version": version,
              "commit": commit, "tree": tree, "channel": channel, "run_id": str(run_id), "attempt": str(attempt),
              "assets": {name: digest(output / name) for _, name in selected}}
    if producer is not None:
        record["producer"] = producer
    if repair is not None:
        record["release_tooling_repair"] = repair
    (output / "release.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    build_run = producer["run_id"] if producer else run_id
    build_attempt = producer["attempt"] if producer else attempt
    # The pinned 1.4.4 repair references its immutable original source layout.
    notes_path = f"docs/{'' if repair else 'releases/'}release-{version}.md"
    notes = (f"AIRK {tag} ({channel})\n\n"
             "**Give Claude Code, Codex, Gemini CLI/Antigravity, GitHub Copilot and other MCP-capable agents "
             "a shared, local index of your workspace.** AIRK returns small, cited passages and facts, each "
             "with its file path, content hash and original location, instead of whole files. Your agent "
             "does the reasoning; you can check exactly what it used.\n\n"
             f"Package version: {version}. Source commit: {commit}.\n"
             f"Changes and upgrade guidance: https://github.com/{REPOSITORY}/blob/{commit}/{notes_path}\n"
             f"Build and checks: https://github.com/{REPOSITORY}/actions/runs/{build_run}/attempts/{build_attempt}\n"
             f"Signing and assembly: https://github.com/{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}\n\n"
             + ("Standalone CLI/MCP installers are included without a VSIX: signed Windows EXE, signed/notarized macOS DMG and unsigned Linux tar.gz. "
              if standalone.required(version, channel) else "") +
             "Downloads include native ZIPs and matching platform VSIX files. Windows uses Ioka LLC "
             "Azure Artifact Signing. macOS runtime executables use Developer ID signing and accepted Apple "
             + ("notarization and a stapled app ticket. Linux is unsigned.\n\n" if tuple(map(int, version.split(".")[:2])) >= (1, 4)
                else "notarization; the standalone CLI has no stapled offline ticket. Linux is unsigned.\n\n") +
             "Use the ZIP matching your OS/architecture, or install the matching VSIX from VS Code's "
             "Install from VSIX command. SHA256SUMS covers packages and build records. "
             "The release tag matches the installed app and extension version.\n\n"
             "Installed-editor and oldest-OS qualification remain separate. Review the release checklist "
             "and dependency notices. This release does not publish to Visual Studio Marketplace.\n")
    notes += interface_previews(version, commit)
    if repair is not None:
        notes += f"\nRelease-tooling repair commit: {repair['tooling_commit']}. Native binaries retain the source commit and tested producer recorded above.\n"
    (output / "release-notes.md").write_text(notes, encoding="utf-8")
    checksums = "".join(f"{digest(path)}  {path.name}\n" for path in sorted(output.iterdir()))
    (output / "SHA256SUMS").write_text(checksums, encoding="utf-8")
    return record


def universal_vsix(directory, producer, commit, version, channel, run_id, *, tree=None):
    """The release run's channel-promoted universal VSIX, bound to the producer's tested VSIX.

    Only the pre-release property may differ (promote-vsix.py channel); every extension byte,
    the UI inventory and the supported I2 contracts are the tested ones. No native identity
    participates: a universal VSIX is never native platform evidence.
    """
    tested_dir = directory / producers.universal_artifact(producer)
    tested, _ = candidate(tested_dir)
    receipts.validate_vsix(tested)
    if not extension.package.same_source(tested, commit, tree) or tested["version"] != version:
        raise ValueError("Producer universal VSIX has different provenance")
    released_dir = directory / f"{producers.UNIVERSAL}{commit}-{run_id}"
    released, archive = candidate(released_dir)
    extension.verify(released_dir, commit, version, channel, receipts=[released_dir / "manifest.json"], tree=tree)
    keys = ("version", "tree", "extension_id", "display_name", "target", "i2_contracts", "required_capabilities",
            "ui_inventory_sha256", "locks", "node", "vsce", "archive", "payload")
    if (any(released[key] != tested[key] for key in keys)
            or released["pre_release"] == tested["pre_release"] and released["archive_sha256"] != tested["archive_sha256"]):
        raise ValueError("Release VSIX differs from the producer's tested universal VSIX")
    return released_dir / "manifest.json", archive


def assemble_split(directory, output, commit, version, tag, channel, run_id, attempt, producer, *, tree=None,
                   pinned=False, source_records=False):
    """Split catalog: per target the signed (Linux: promoted) pair and installer, one universal VSIX."""
    require_campaign_producer(producer)
    if producer is None or not source_records:
        raise ValueError("Split releases require an authenticated producer and source receipts")
    (producers.validate_record if pinned else producers.verify_record)(producer, commit, version=version)
    selected = []
    for target in native.TARGETS:
        signed = target in SIGNED_TARGETS
        prefix = "airk-signed" if signed else "airk"
        native_dir = directory / f"{prefix}-native-{target}-{commit}-{run_id}"
        receipt, archive = verify_native(native_dir, target, commit, version, signed, tree=tree)
        original = source_receipt(directory / f"airk-source-{target}-{commit}-{run_id}",
                                  producer, target, commit, version, tree)
        if signed:
            if receipt["signing"]["source_install_sha256"] != original["payload"]["airk-install.json"]:
                raise ValueError("Signed pair did not originate in the tested candidate")
            for name, component in receipt["signing"]["components"].items():
                if component["source_sha256"] != original["payload"][name]:
                    raise ValueError("Signed component did not originate in the tested candidate")
            # Signing transforms executables only: the admin shell and interfaces stay the tested ones.
            if any(receipt[key] != original[key] for key in ("tree", "source", "admin_assets", "i1_protocol",
                                                             "i2_contract", "cargo_lock_sha256")):
                raise ValueError("Signed pair changed the tested source, admin or interface identity")
        elif receipt != original:
            raise ValueError("Linux native package differs from the tested producer")
        selected.extend(((archive, archive.name),
                         (native_dir / "manifest.json", f"airk-{version}-{target}.native-manifest.json")))
        if publication.current(version, channel):
            portable_dir = directory / f"airk-portable-{target}-{commit}-{run_id}"
            portable_owner = module("portable_package", "package-portable.py")
            _, portable_zip = portable_owner.verify(portable_dir, receipt, digest(native_dir / "manifest.json"), channel=channel)
            selected.extend(((portable_zip, portable_zip.name),
                             (portable_dir / standalone.portable_manifest(version, target), standalone.portable_manifest(version, target))))
        if standalone.required(version, channel):
            installer_dir = directory / f"airk-installer-{target}-{commit}-{run_id}"
            _, installer = verify_installer(installer_dir, target, commit, version, receipt)
            selected.extend(((installer, installer.name),
                             (installer_dir / "installer.json", standalone.manifest(version, target))))
    manifest, vsix = universal_vsix(directory, producer, commit, version, channel, run_id, tree=tree)
    selected.extend(((vsix, vsix.name), (manifest, f"airk-{version}.vsix-manifest.json")))
    if ({name for _, name in selected}
            != publication.catalog_names(version, channel, commit, producer, dict(run_id=str(run_id)))[2]):
        raise ValueError("Assembled assets differ from the split release catalog")
    output.mkdir(parents=True, exist_ok=False)
    for source, name in selected:
        shutil.copyfile(source, output / name)
    record = {"schema": "airk-github-release/1", "tag": tag, "package_version": version,
              "commit": commit, "tree": tree, "channel": channel, "run_id": str(run_id), "attempt": str(attempt),
              "assets": {name: digest(output / name) for _, name in selected}, "producer": producer}
    if publication.current(version, channel):
        record["schema"] = publication.RELEASE
        record["installers"] = publication.installer_rows(version, channel)
        publication.validate_release(record)
    if optional.current(version):
        owner = optional.projection(tag)
        for name, asset in owner["assets"].items():
            part_dir = directory / optional.part_input(name, commit, run_id)
            if part_dir.is_symlink() or not part_dir.is_dir() or {path.name for path in part_dir.iterdir()} != {name}:
                raise ValueError("Optional same-run part input roster differs")
            optional.verify_part(part_dir / name, asset)
        record.update(schema=optional.RELEASE, optional_models=owner)
        record["assets"].update({name: asset["sha256"] for name, asset in owner["assets"].items()})
        optional.validate_release(record, owner)
    (output / "release.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    notes = (f"AIRK {tag} ({channel})\n\n"
             "**Give Claude Code, Codex, Gemini CLI/Antigravity, GitHub Copilot and other MCP-capable agents "
             "a shared, local index of your workspace.** AIRK returns small, cited passages and facts, each "
             "with its file path, content hash and original location, instead of whole files. Your agent "
             "does the reasoning; you can check exactly what it used.\n\n"
             f"Package version: {version}. Source commit: {commit}.\n"
             f"Changes and upgrade guidance: https://github.com/{REPOSITORY}/blob/{commit}/docs/releases/release-{version}.md\n"
             f"Build and checks: https://github.com/{REPOSITORY}/actions/runs/{producer['run_id']}/attempts/{producer['attempt']}\n"
             f"Signing and assembly: https://github.com/{REPOSITORY}/actions/runs/{run_id}/attempts/{attempt}\n\n"
             + ("Installers: signed Windows EXE, signed/notarized macOS DMG and unsigned Linux tar.gz. "
                if standalone.required(version, channel) else "") +
             "Each native ZIP holds the AIRK service (`airk`) and its command-line client (`airkctl`) for one "
             "OS/architecture. The VS Code extension is one universal VSIX "
             f"(airk-{version}.vsix) with no native code; it connects to a separately installed AIRK service. "
             "Windows uses Ioka LLC Azure Artifact Signing. macOS executables use Developer ID signing, accepted "
             "Apple notarization and a stapled app ticket. Linux is unsigned.\n\n"
             "SHA256SUMS covers packages and build records. The release tag matches the service and "
             "extension version.\n\n"
             "Installed-editor and oldest-OS qualification remain separate. Review the release checklist "
             "and dependency notices. This release does not publish to Visual Studio Marketplace.\n")
    notes += interface_previews(version, commit)
    (output / "release-notes.md").write_text(notes, encoding="utf-8")
    sums = {path.name: digest(path) for path in output.iterdir()}
    if record["schema"] == optional.RELEASE:
        sums.update({name: asset["sha256"] for name, asset in owner["assets"].items()})
    checksums = "".join(f"{value}  {name}\n" for name, value in sorted(sums.items()))
    (output / "SHA256SUMS").write_text(checksums, encoding="utf-8")
    return record


def github(path, *, missing=False, paginate=False):
    return reads.get(f"repos/{REPOSITORY}/{path}", missing=missing, paginate=paginate)


def release_id_for_tag(tag):
    # The by-tag endpoint returns published releases only. Include drafts and
    # every page, so an earlier partial publication cannot be overwritten.
    pages = github("releases?per_page=100", paginate=True)
    matches = [entry for page in pages for entry in page if entry.get("tag_name") == tag]
    if not matches:
        return None
    if len(matches) != 1 or type(matches[0].get("id")) is not int or matches[0]["id"] <= 0:
        raise ValueError("Ambiguous or invalid release identity; inspect retained drafts")
    return matches[0]["id"]


def require_unused_tag(tag):
    if (github(f"releases/tags/{tag}", missing=True) is not None
            or release_id_for_tag(tag) is not None
            or github(f"git/ref/tags/{tag}", missing=True) is not None):
        raise ValueError("Release/tag already exists; finish the existing release or select an unused package version")


def verify_bundle(directory, commit, version, tag, run_id, attempt):
    record = extension.read_receipt(directory / "release.json")
    optional_owner = optional.projection(tag) if optional.current(version) else None
    expected_schema = optional.RELEASE if optional_owner is not None else publication.RELEASE if publication.current(version, record.get("channel")) else "airk-github-release/1"
    if (record.get("schema") != expected_schema or record.get("commit") != commit
            or record.get("tag") != tag or record.get("package_version") != version
            or record.get("run_id") != str(run_id) or record.get("attempt") != str(attempt)
            or record.get("channel") not in ("preview", "stable")):
        raise ValueError("Release bundle belongs to another run or commit")
    if optional_owner is not None:
        optional.validate_release(record, optional_owner)
    elif expected_schema == publication.RELEASE:
        publication.validate_release(record)
    if receipts.split_version(version):
        # Pair packages (and installers) per target and one universal VSIX (C7 release catalog).
        if not isinstance(record.get("producer"), dict) or record.get("release_tooling_repair") is not None:
            raise ValueError("Split release bundle lacks its authenticated producer")
        expected = set(publication.catalog_names(version, record["channel"], commit, record["producer"],
                                              dict(run_id=str(run_id)))[2])
    else:
        expected = set()
        for target, platform in native.TARGETS.items():
            expected.update((f"airk-{version}-{target}.zip", f"airk-{version}-{platform}.vsix",
                             f"airk-{version}-{target}.native-manifest.json",
                             f"airk-{version}-{platform}.vsix-manifest.json"))
            if standalone.required(version, record["channel"]):
                expected.update((standalone.artifact(version, target), standalone.manifest(version, target)))
    if optional_owner is not None:
        expected.update(optional_owner["assets"])
    if set(record["assets"]) != expected:
        raise ValueError("Release bundle asset set differs")
    expected.update(("release.json", "release-notes.md", "SHA256SUMS"))
    actual_names = set(p.name for p in directory.iterdir())
    part_names = set(optional_owner["assets"]) if optional_owner is not None else set()
    small_names = expected - part_names
    if directory.is_symlink() or actual_names not in (expected, small_names):
        raise ValueError("Unexpected release bundle files")
    parts_present = actual_names == expected
    if any(p.is_symlink() or not p.is_file() for p in directory.iterdir()):
        raise ValueError("Release bundle links rejected")
    for name, value in record["assets"].items():
        if name in part_names:
            if parts_present:
                optional.verify_part(directory / name, optional_owner["assets"][name])
        elif digest(directory / name) != value:
            raise ValueError("Release bundle digest mismatch")
    sums = {p.name: digest(p) for p in directory.iterdir() if p.name != "SHA256SUMS"}
    if optional_owner is not None:
        sums.update({name: asset["sha256"] for name, asset in optional_owner["assets"].items()})
    if (directory / "SHA256SUMS").read_text(encoding="utf-8") != "".join(f"{value}  {name}\n" for name, value in sorted(sums.items())):
        raise ValueError("Release bundle checksums differ")
    if standalone.required(version, record["channel"]):
        for target in native.TARGETS:
            runtime = extension.read_receipt(directory / f"airk-{version}-{target}.native-manifest.json")
            verify_installer(directory, target, commit, version, runtime, published=True)
    if publication.current(version, record["channel"]):
        portable_owner = module("portable_package", "package-portable.py")
        for target in native.TARGETS:
            manifest_path = directory / f"airk-{version}-{target}.native-manifest.json"
            runtime = extension.read_receipt(manifest_path)
            portable_owner.verify(directory, runtime, digest(manifest_path), published=True, channel=record["channel"])
    return record


def stage_marketplace_bundle(directory, output, commit, version, channel, run_id, attempt, *, tree=None,
                             published_only=False):
    """Revalidate the signed producer's complete set and copy its exact VSIX bytes.

    No compilation, signing, channel rewriting or package regeneration is allowed.
    GitHub and Marketplace therefore share native and compiled extension identities.
    """
    record = verify_bundle(directory, commit, version, identity(version), run_id, attempt)
    if record["channel"] != channel or record.get("release_tooling_repair") is not None:
        raise ValueError("Signed bundle has a different channel or historical tooling repair")
    producer = record.get("producer")
    if producer is None:
        raise ValueError("Signed release has no authenticated CI producer")
    split = receipts.split_version(version)
    producers.verify_record(producer, commit, **producers.catalog_args(version))
    if published_only:
        published = github(f"releases/tags/{identity(version)}", missing=True)
        tag = github(f"git/ref/tags/{identity(version)}", missing=True)
        if (not published or published.get("draft") is not False
                or published.get("tag_name") != identity(version)
                or published.get("target_commitish") != commit
                or published.get("prerelease") is not (channel == "preview")
                or not tag or tag.get("object", {}).get("sha") != commit):
            raise ValueError("Marketplace publication requires the matching published GitHub release first")
        assets = published.get("assets", [])
        actual = {asset.get("name"): asset.get("digest") for asset in assets}
        expected = {path.name: "sha256:" + digest(path) for path in directory.iterdir()}
        if record["schema"] == optional.RELEASE:
            owner = optional.projection(identity(version))
            expected.update({name: "sha256:" + asset["sha256"] for name, asset in owner["assets"].items()})
            if any(asset.get("size") != owner["assets"][asset["name"]]["bytes"]
                   for asset in assets if asset.get("name") in owner["assets"]):
                raise ValueError("Published optional part size differs from native pins")
        if len(actual) != len(assets) or actual != expected:
            raise ValueError("Published GitHub assets differ from the selected signed producer")
    if output.exists():
        raise ValueError("Marketplace staging destination must be fresh")
    if split:
        return stage_universal(directory, output, record, commit, version, channel, run_id, attempt, tree=tree)
    with tempfile.TemporaryDirectory(prefix="airk-marketplace-verify-") as temporary:
        staging = Path(temporary)
        for target, platform in native.TARGETS.items():
            native_dir = staging / "native" / target
            vsix_dir = staging / "vsix" / target
            for destination, archive_name, manifest_name in (
                (native_dir, f"airk-{version}-{target}.zip", f"airk-{version}-{target}.native-manifest.json"),
                (vsix_dir, f"airk-{version}-{platform}.vsix", f"airk-{version}-{platform}.vsix-manifest.json"),
            ):
                destination.mkdir(parents=True)
                shutil.copyfile(directory / archive_name, destination / archive_name)
                shutil.copyfile(directory / manifest_name, destination / "manifest.json")
                (destination / "SHA256SUMS").write_text(
                    f"{digest(destination / archive_name)}  {archive_name}\n"
                    f"{digest(destination / 'manifest.json')}  manifest.json\n", encoding="utf-8")
            receipt, _ = verify_native(native_dir, target, commit, version, target in SIGNED_TARGETS, tree=tree)
            vsix_receipt, _ = candidate(vsix_dir)
            if (vsix_receipt.get("native_manifest_sha256") != digest(native_dir / "manifest.json")
                    or vsix_receipt.get("native_archive_sha256") != receipt["archive_sha256"]):
                raise ValueError("Marketplace VSIX differs from the canonical signed native download")
        extension.verify(staging / "vsix", commit, version, channel, tree=tree)
        # Recheck complete source bytes after validation, before emitting anything.
        verify_bundle(directory, commit, version, identity(version), run_id, attempt)
        shutil.copytree(staging / "vsix", output)
        for target, platform in native.TARGETS.items():
            archive_name = f"airk-{version}-{platform}.vsix"
            manifest_name = f"airk-{version}-{platform}.vsix-manifest.json"
            if (digest(output / target / archive_name) != record["assets"][archive_name]
                    or digest(output / target / "manifest.json") != record["assets"][manifest_name]):
                raise ValueError("Marketplace staging changed the signed producer's package bytes")
    return record


def stage_universal(directory, output, record, commit, version, channel, run_id, attempt, *, tree=None):
    """Copy the signed release's one universal VSIX for Marketplace; no native byte participates."""
    archive_name, manifest_name = f"airk-{version}.vsix", f"airk-{version}.vsix-manifest.json"
    with tempfile.TemporaryDirectory(prefix="airk-marketplace-verify-") as temporary:
        staging = Path(temporary) / "vsix"
        destination = staging / "universal"
        destination.mkdir(parents=True)
        shutil.copyfile(directory / archive_name, destination / archive_name)
        shutil.copyfile(directory / manifest_name, destination / "manifest.json")
        (destination / "SHA256SUMS").write_text(
            f"{digest(destination / archive_name)}  {archive_name}\n"
            f"{digest(destination / 'manifest.json')}  manifest.json\n", encoding="utf-8")
        extension.verify(staging, commit, version, channel, tree=tree)
        # Recheck complete source bytes after validation, before emitting anything.
        verify_bundle(directory, commit, version, identity(version), run_id, attempt)
        shutil.copytree(staging, output)
        if (digest(output / "universal" / archive_name) != record["assets"][archive_name]
                or digest(output / "universal" / "manifest.json") != record["assets"][manifest_name]):
            raise ValueError("Marketplace staging changed the signed producer's package bytes")
    return record


def qualified_signed_preflight(run_id):
    commit, version, tag = context()
    if os.environ.get("GITHUB_REF") != "refs/heads/" + authority.CAMPAIGN_BRANCH or not publication.current(version, "stable"):
        raise ValueError("Retained signed publication is only the current campaign path")
    authority.verify_campaign_publisher(producers.api, os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], commit)
    if producers.positive(run_id) == producers.positive(os.environ["GITHUB_RUN_ID"]):
        raise ValueError("Retained signed publication needs an earlier successful run")
    origin = {"branch": authority.CAMPAIGN_BRANCH}
    if optional.current(version):
        origin["optional_projection"] = optional.projection(tag)
    record = producers.attest_signed_release(run_id, commit, **origin)
    return commit, version, tag, record


def publish_qualified(signed_producer, expected_producer, channel, resume=False):
    """Publish only an API-authenticated owned snapshot of the qualified assembly."""
    if channel not in ("stable", "preview"):
        raise ValueError("Retained publication requires its reviewed channel")
    if not isinstance(signed_producer, dict) or not isinstance(expected_producer, dict):
        raise ValueError("Retained publication requires both protected producer records")
    commit, version, tag, observed = qualified_signed_preflight(signed_producer.get("run_id"))
    if authority.canonical(observed) != authority.canonical(signed_producer):
        raise ValueError("Qualified signed assembly identity changed")
    producers.validate_record(expected_producer, commit, version=version)
    require_campaign_producer(expected_producer)
    tree = extension.package.source_tree(commit)
    with tempfile.TemporaryDirectory(prefix="airk-qualified-publish-") as temporary:
        snapshot = Path(temporary) / "bundle"
        # The bytes come from the artifact whose digest the actual GitHub API
        # authenticated. A caller's local directory/JSON never substitutes.
        producers.download_signed_release(signed_producer, commit, snapshot, version, channel,
                                         tree=tree, full_bundle=True)
        record = verify_bundle(snapshot, commit, version, tag, signed_producer["run_id"], signed_producer["attempt"])
        if authority.canonical(record.get("producer")) != authority.canonical(expected_producer):
            raise ValueError("Qualified bundle does not contain the pinned complete CI producer")
        require_campaign_producer(record["producer"])
        producers.verify_record(record["producer"], commit, **producers.catalog_args(version))
        authority.verify_campaign_publisher(producers.api, os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], commit)
        origin = {"branch": authority.CAMPAIGN_BRANCH}
        if optional.current(version):
            origin["optional_projection"] = optional.projection(tag)
        final_signed = producers.attest_signed_release(signed_producer["run_id"], commit, **origin)
        if authority.canonical(final_signed) != authority.canonical(signed_producer):
            raise ValueError("Qualified signed assembly changed before publication")
        return _publish_assets(snapshot, record, tag, commit, resume)


def publish(directory, resume=False, source_commit=None, signed_producer=None, expected_producer=None, qualified_channel=None):
    if signed_producer is not None:
        if directory is not None or source_commit is not None:
            raise ValueError("Qualified publication takes its bytes only from the authenticated signed artifact")
        return publish_qualified(signed_producer, expected_producer, qualified_channel, resume)
    commit, version, tag = context()
    # Only the hard-pinned repair adapter supplies a distinct source identity.
    if source_commit is not None:
        commit = source_commit
    if os.environ.get("GITHUB_REF") == "refs/heads/" + authority.CAMPAIGN_BRANCH:
        raise ValueError("Campaign publication requires its previously qualified signed assembly")
    attempt = os.environ["GITHUB_RUN_ATTEMPT"]
    if resume:
        attempt = extension.read_receipt(directory / "release.json")["attempt"]
        if not 0 < producers.positive(attempt) <= producers.positive(os.environ["GITHUB_RUN_ATTEMPT"]):
            raise ValueError("Release bundle comes from a future attempt")
    record = verify_bundle(directory, commit, version, tag, os.environ["GITHUB_RUN_ID"], attempt)
    require_campaign_producer(record.get("producer"))
    if source_commit is None and record.get("producer") is not None:
        producers.verify_record(record["producer"], commit, **producers.catalog_args(version))
    return _publish_assets(directory, record, tag, commit, resume)


def _publish_assets(directory, record, tag, commit, resume):
    """Upload/read back the caller's already verified owned bundle; no rebuilds."""
    assets = sorted(directory.iterdir())
    release_id = release_id_for_tag(tag) if resume else None
    expected = {p.name: "sha256:" + digest(p) for p in assets}
    if release_id is not None:
        uploaded = github(f"releases/{release_id}")
        actual = {p["name"]: p.get("digest") for p in uploaded["assets"]}
        if (uploaded.get("id") != release_id or uploaded.get("tag_name") != tag
                or uploaded.get("target_commitish") != commit
                or uploaded.get("prerelease") is not (record["channel"] == "preview")
                or len(uploaded["assets"]) != len(actual)
                or any(name not in expected or expected[name] != value for name, value in actual.items())):
            raise ValueError("Existing release identity/assets differ; draft retained for inspection")
        tag_ref = github(f"git/ref/tags/{tag}", missing=True)
        if tag_ref is not None and tag_ref.get("object", {}).get("sha") != commit:
            raise ValueError("Release tag points at another commit; draft retained for inspection")
        if uploaded.get("draft") is False:
            if actual != expected or tag_ref is None:
                raise ValueError("Published release is incomplete or has no matching tag")
            print("Verified release already published")
            return
        if uploaded.get("draft") is not True:
            raise ValueError("Invalid remote draft state")
        missing = [str(p) for p in assets if p.name not in actual]
        if missing:
            subprocess.run(["gh", "release", "upload", tag, "--repo", REPOSITORY, *missing], check=True)
    else:
        require_unused_tag(tag)
        create_draft(tag, commit, record["channel"], directory, assets)
        release_id = release_id_for_tag(tag)
    if release_id is None:
        raise ValueError("Uploaded draft not found; rerun the failed publish job after inspection")
    uploaded = github(f"releases/{release_id}")
    actual = {p["name"]: p.get("digest") for p in uploaded["assets"]}
    if (uploaded.get("id") != release_id or uploaded.get("tag_name") != tag
            or uploaded["draft"] is not True or uploaded["target_commitish"] != commit
            or uploaded["prerelease"] is not (record["channel"] == "preview")
            or len(uploaded["assets"]) != len(expected) or actual != expected):
        raise ValueError("Uploaded draft identity/assets differ; draft retained for inspection")
    tag_ref = github(f"git/ref/tags/{tag}", missing=True)
    if tag_ref is not None and tag_ref.get("object", {}).get("sha") != commit:
        raise ValueError("Release tag points at another commit; draft retained for inspection")
    subprocess.run(["gh", "api", f"repos/{REPOSITORY}/releases/{release_id}", "--method", "PATCH",
                    "-F", "draft=false", "-f", "make_latest=" + ("true" if record["channel"] == "stable" else "false")],
                   check=True)


def create_draft(tag, commit, channel, directory, assets):
    command = ["gh", "release", "create", tag, "--repo", REPOSITORY, "--draft",
               "--target", commit, "--title", f"AIRK {tag}", "--notes-file", str(directory / "release-notes.md")]
    if channel == "preview":
        command.append("--prerelease")
    subprocess.run(command + [str(p) for p in assets], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("preflight", "stage-signing", "source-record", "assemble", "publish"))
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target", choices=native.TARGETS)
    parser.add_argument("--producer", type=Path)
    parser.add_argument("--signed-run-id")
    parser.add_argument("--signed-producer", type=Path)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--pinned-producer", action="store_true")
    parser.add_argument("--source-records", action="store_true")
    parser.add_argument("--channel", choices=("preview", "stable"), default="stable")
    args = parser.parse_args()
    commit, version, tag = context()
    if args.command == "preflight":
        if args.signed_run_id:
            _, _, _, signed = qualified_signed_preflight(args.signed_run_id)
            selected = ROOT / "artifacts/signed-producer.json"
            selected.parent.mkdir(parents=True, exist_ok=True)
            selected.write_text(json.dumps(signed, indent=2) + "\n", encoding="utf-8")
        else:
            require_unused_tag(tag)
        split = str(receipts.split_version(version)).lower()
        owner = optional.projection(tag) if optional.current(version) else None
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write("optional_parts=" + json.dumps(sorted(owner["assets"]) if owner is not None else [], separators=(",", ":")) + "\n")
            output.write(f"version={version}\ntag={tag}\nsplit={split}\nportable={str(publication.current(version, 'stable')).lower()}\n")
            if args.signed_run_id:
                output.write("action=publish-qualified\n")
        print(f"Release {tag}, package version {version}, source {commit}, split catalog {split}")
    elif args.command == "stage-signing":
        stage_signing(args.directory, args.target, commit, version, tree=extension.package.source_tree(commit))
    elif args.command == "source-record":
        if not receipts.split_version(version) or not (args.directory and args.target and args.producer and args.output):
            parser.error("source-record is the split-version path and requires --directory, --target, --producer and --output")
        write_split_source_record(args.directory, args.output, json.loads(args.producer.read_text(encoding="utf-8")),
                                  args.target, commit, version, tree=extension.package.source_tree(commit))
    elif args.command == "assemble":
        assemble(args.directory, args.output, commit, version, tag, args.channel,
                 os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"],
                 json.loads(args.producer.read_text(encoding="utf-8")) if args.producer else None,
                 tree=extension.package.source_tree(commit), pinned=args.pinned_producer, source_records=args.source_records)
        print(f"Verified release assets: {args.output}")
    else:
        if args.signed_producer and not args.producer:
            parser.error("Retained signed publication requires its protected CI producer record")
        if args.signed_producer:
            publish(args.directory, resume=args.resume,
                    signed_producer=json.loads(args.signed_producer.read_text(encoding="utf-8")),
                    expected_producer=json.loads(args.producer.read_text(encoding="utf-8")),
                    qualified_channel=args.channel)
        else:
            publish(args.directory, resume=args.resume)


if __name__ == "__main__":
    main()
