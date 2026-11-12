# SPDX-License-Identifier: Apache-2.0
"""Archive/installer contracts; fictional bytes do not qualify Linux execution."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("package_standalone_linux", Path(__file__).parents[1] / "package-standalone-linux.py")
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)


# The pre-split CLI installer is kept for versions before split_receipts.SPLIT_VERSION.
PRESPLIT_VERSION = "1.8.23"


def fixture(base, version=PRESPLIT_VERSION):
    """Use the real native legal/inventory validator with tiny fictional data."""
    runtime, publish = base / "runtime", base / "native/publish"
    runtime.mkdir(); publish.mkdir(parents=True)
    builder = package.native.runtime_builder()
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
                patch.object(package.native, "capture", side_effect=lambda *args: "c" * 40 if args[-1] == "HEAD" else "d" * 40),
                patch.object(package, "workspace_version", return_value=receipt["version"]))

    def test_offline_archive_preserves_exact_native_payload_legal_and_unsigned_identity(self):
        native, original, builder = fixture(self.base)
        first = self.base / "first"
        a, b, c, d = self.environment(builder, original)
        with a, b, c, d:
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
                a, b, c, d = self.environment(builder, receipt)
                with a, b, c, d, self.assertRaises(ValueError):
                    package.package_candidate(native, self.base / ("bad-" + field))
                self.assertFalse((self.base / ("bad-" + field)).exists())
        seal_receipt(native, receipt)

    def test_candidate_unknown_file_and_hard_link_are_refused(self):
        native, receipt, builder = fixture(self.base)
        extra = native / "extra.vsix"
        extra.write_bytes(b"not part of a standalone native package")
        with patch.object(package, "workspace_version", return_value=receipt["version"]),                 self.assertRaisesRegex(ValueError, "Unexpected native candidate"):
            package.candidate(native)
        extra.unlink()
        alias = self.base / "external-link"
        os.link(native / receipt["archive"], alias)
        try:
            with patch.object(package, "workspace_version", return_value=receipt["version"]),                     self.assertRaises(ValueError):
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
        for version in (PRESPLIT_VERSION, SPLIT_VERSION):
            with self.subTest(version=version), self.assertRaises(ValueError):
                package.render_template("install.sh.in", {"VERSION": version})


SPLIT_VERSION = "1.8.24"
NATIVE_TESTS_SPEC = importlib.util.spec_from_file_location("package_native_tests_for_linux",
                                                           Path(__file__).with_name("test_package_native.py"))
native_tests = importlib.util.module_from_spec(NATIVE_TESTS_SPEC)
NATIVE_TESTS_SPEC.loader.exec_module(native_tests)
SOURCE = {"sha256": "1" * 64, "files": 4}
# The section 9.5 Linux program layout: the two images by reviewed path, the root legal files under
# notices/, and the runtime's notices/ tree as it is; pair metadata never enters the program folder.
SPLIT_FIXED = {"bin/airkctl", "service/airk", "notices/LICENSE", "notices/NOTICE", "notices/THIRD_PARTY_NOTICES.md"}


def split_layout(payload):
    return SPLIT_FIXED | {name for name in payload if name.startswith("notices/")}
SH = shutil.which("sh")
# Field order of airk_core::install_record::SetupOperation (canonical bytes are declaration order).
OPERATION_FIELDS = ["schema", "operation_id", "kind", "created_unix_ms", "expires_unix_ms", "requested_by",
                    "acknowledgements", "locations", "payload_inventory_sha256", "options"]


def split_fixture(base):
    runtime, publish = base / "split-runtime", base / "split-native/publish"
    runtime.mkdir(); publish.mkdir(parents=True)
    native_tests.split_runtime(runtime, package.TARGET, SOURCE)
    files, executables = package.native.validate_runtime(runtime, SPLIT_VERSION, package.TARGET, SOURCE)
    archive = publish / f"airk-{SPLIT_VERSION}-{package.TARGET}.zip"
    package.native.write_archive(runtime, archive, files, executables)
    common = {"version": SPLIT_VERSION, "target": package.TARGET, "commit": "c" * 40, "tree": "d" * 40,
              "dirty_checkout": False, "source": SOURCE,
              "cargo_lock_sha256": package.native.sha256(package.ROOT / "Cargo.lock"),
              "toolchain_file_sha256": package.native.sha256(package.ROOT / "rust-toolchain.toml"),
              "rustc": "rustc fixture\nhost: " + package.TARGET, "cargo": "cargo fixture",
              "archive": archive.name, "archive_sha256": package.native.sha256(archive),
              "install_manifest_sha256": files["airk-install.json"], "payload": files,
              "bundled_model": {"manifest": "fixture.json"}, "embedded_models": {}, "signed_distribution": False,
              "signing": None}
    receipt = package.native.split_candidate(common, files, executables, runtime, {"schema": 1, "profile": "fixture"})
    seal_receipt(publish, receipt)
    return publish, receipt


FAKE_AIRK = b"""#!/bin/sh
# Fictional setup role: records each call and answers --plan from the environment.
operation=$4
mode=${5:-complete}
printf '%s\\n' "$mode" >> "$AIRK_FAKE_LOG"
cp "$operation" "$AIRK_FAKE_LOG.${mode#--}.json"
if [ "$mode" = --plan ]; then
    printf '{"predecessors":[{"removed":%s}],"location_problems":[%s]}\\n' "$AIRK_FAKE_REMOVED" "$AIRK_FAKE_PROBLEM" > "$(dirname "$operation")/plan.json"
    printf 'fictional plan\\r\\n' > "$(dirname "$operation")/plan.txt"
fi
printf '{"schema":"airk-setup-result/1","status":"fixture"}\\n'
"""


def service_package(base, airk=FAKE_AIRK):
    """A split package built from the packager's own layout, inventory, roster and template code."""
    stage = base / "package"
    layout = stage / "payload"
    for name, data in (("service/airk", airk), ("bin/airkctl", b"fictional airkctl\n"),
                       ("notices/LICENSE", b"fictional license\n")):
        (layout / name).parent.mkdir(parents=True, exist_ok=True)
        (layout / name).write_bytes(data)
        (layout / name).chmod(0o755 if name != "notices/LICENSE" else 0o644)
    inventory = package.payload_inventory(layout, SPLIT_VERSION, {"inventory_sha256": "3" * 64, "bundle_sha256": "4" * 64})
    count = package.write_inventory(layout, inventory, stage)
    values = {"VERSION": SPLIT_VERSION, "ARCHIVE_NAME": "fixture.tar.gz", "DIRECTORY_NAME": "fixture",
              "PAYLOAD_FILE_COUNT": count, "PAYLOAD_SUMS_SHA256": package.native.sha256(stage / "payload.sha256"),
              "INVENTORY_SHA256": package.native.sha256(layout / "payload-inventory.json"),
              "AIRK_SHA256": inventory["files"]["service/airk"]["sha256"]}
    (stage / "install.sh").write_bytes(package.render_template("install.sh.in", values).encode())
    return stage, values


class LinuxServiceInstaller(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=package.ROOT / "artifacts", prefix="linux-service-installer-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)

    def environment(self, builder, receipt):
        return (patch.object(package.native, "runtime_builder", return_value=builder),
                patch.object(builder, "source_identity", return_value=receipt["source"]),
                patch.object(package.native, "capture", side_effect=lambda *args: "c" * 40 if args[-1] == "HEAD" else "d" * 40),
                patch.object(package, "workspace_version", return_value=SPLIT_VERSION))

    def test_split_package_places_the_service_layout_inventory_and_pinned_installer(self):
        native, original = split_fixture(self.base)
        builder = package.native.runtime_builder()
        a, b, c, d = self.environment(builder, original)
        with a, b, c, d:
            receipt = package.package_candidate(native, self.base / "first")
            other = package.package_candidate(native, self.base / "second")
        self.assertEqual(receipt["artifact_sha256"], other["artifact_sha256"], "identical inputs produce stable bytes")
        self.assertEqual(receipt["artifact"], f"airk-{SPLIT_VERSION}-{package.TARGET}-installer.tar.gz")
        self.assertEqual((receipt["installer_kind"], receipt["code_signing"]), ("linux_tar", "unsigned"))
        self.assertEqual(receipt["signing"], {"platform": "Linux", "trust": "unsigned"})
        self.assertEqual(set(receipt["installer_sources"]), {"scripts/package-standalone-linux.py",
                         "installers/linux/install.sh.in", "installers/linux/README.txt.in"})
        directory = receipt["artifact"].removesuffix(".tar.gz")
        artifact = self.base / "first/publish" / receipt["artifact"]
        package.verify_installer(artifact, receipt["payload"], directory)
        with tarfile.open(artifact, "r:gz") as archive:
            members = {member.name.removeprefix(directory + "/"): member for member in archive.getmembers()}
            inventory_bytes = archive.extractfile(members["payload/payload-inventory.json"]).read()
            shell = archive.extractfile(members["install.sh"]).read().decode()
            readme = archive.extractfile(members["README.txt"]).read().decode()
            roster = archive.extractfile(members["payload.sha256"]).read().decode()
        inventory = json.loads(inventory_bytes)
        layout = split_layout(original["payload"])
        self.assertIn("notices/all-MiniLM-L6-v2/LICENSE", layout)
        self.assertEqual(set(inventory["files"]), layout)
        self.assertEqual({name for name in members if name.startswith("payload/")},
                         {"payload/" + name for name in (*layout, "payload-inventory.json")})
        self.assertEqual({name for name in members if not name.startswith("payload/")},
                         {"install.sh", "README.txt", "payload.sha256"})
        self.assertEqual(inventory["schema"], "airk-service-payload/1")
        self.assertEqual(inventory["executables"], {"airk": "service/airk", "airkctl": "bin/airkctl"})
        self.assertEqual(inventory["signing"], {"platform": "linux", "signed": False, "integrity": "record-sha256"})
        self.assertEqual(inventory["admin_assets"], original["admin_assets"])
        self.assertEqual((inventory["version"], inventory["target"], inventory["channel"]),
                         (SPLIT_VERSION, package.TARGET, "stable"))
        self.assertEqual(inventory["files"]["service/airk"]["sha256"], original["airk_sha256"])
        self.assertEqual(inventory["files"]["bin/airkctl"]["sha256"], original["airkctl_sha256"])
        for name in ("install.sh", "payload/service/airk", "payload/bin/airkctl"):
            self.assertEqual(members[name].mode, 0o755, name)
        self.assertEqual(members["payload/notices/LICENSE"].mode, 0o644)
        inventory_sha256 = hashlib.sha256(inventory_bytes).hexdigest()
        self.assertEqual(receipt["payload_inventory_sha256"], inventory_sha256)
        self.assertIn(f"inventory_sha256='{inventory_sha256}'", shell)
        self.assertIn(f"roster_sha256='{hashlib.sha256(roster.encode()).hexdigest()}'", shell)
        self.assertIn(f"airk_sha256='{original['airk_sha256']}'", shell)
        self.assertIn(f"file_count='{len(layout) + 1}'", shell)
        self.assertIn(f"  payload/payload-inventory.json\n", roster)
        self.assertIn('__airk_role setup --operation "$operation"', shell)
        self.assertIn('"requested_by":"install-sh"', shell)
        # Every package check runs before the first native execution.
        first_native = shell.index('run_setup "$image" --plan')
        for check in ("sha256sum --check --strict --status payload.sha256", '"$airk_sha256" payload/service/airk',
                      '"$inventory_sha256" payload/payload-inventory.json', "-links +1"):
            self.assertLess(shell.index(check), first_native, check)
        self.assertLess(shell.index("--prepare"), shell.index("place ||"))
        for absent in ("sudo", "curl ", ".bashrc", ".profile", "@VERSION@", "runtime-source"):
            self.assertNotIn(absent, shell)
        self.assertIn("airkctl service uninstall --yes", readme)
        self.assertIn("unsigned", readme)
        self.assertIn("gh attestation verify " + receipt["artifact"], readme)

    def test_split_layout_refuses_files_outside_the_service_layout(self):
        runtime = self.base / "runtime"
        runtime.mkdir()
        (runtime / "airk-embed").write_bytes(b"not part of the split layout")
        files = {"airk-embed": package.native.sha256(runtime / "airk-embed")}
        with self.assertRaisesRegex(ValueError, "outside the service layout"):
            package.service_layout(runtime, files, self.base / "layout")

    def test_split_candidate_must_be_the_split_schema(self):
        native, receipt = split_fixture(self.base)
        seal_receipt(native, {**receipt, "schema": "airk-native-candidate/2"})
        builder = package.native.runtime_builder()
        a, b, c, d = self.environment(builder, receipt)
        with a, b, c, d, self.assertRaises(ValueError):
            package.package_candidate(native, self.base / "refused")
        self.assertFalse((self.base / "refused").exists())

    @unittest.skipIf(SH is None, "a POSIX sh is not available on this host")
    def test_install_script_is_valid_posix_sh(self):
        stage, _ = service_package(self.base)
        result = subprocess.run([SH, "-n", str(stage / "install.sh")], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def run_installer(self, stage, *arguments, **environment):
        log = self.base / "airk.log"
        home = self.base / "home"
        (home / ".local/bin").mkdir(parents=True, exist_ok=True)
        env = {**os.environ, "PATH": str(Path(SH).parent) + os.pathsep + os.environ.get("PATH", ""),
               "HOME": str(home), "AIRK_FAKE_LOG": str(log), "AIRK_FAKE_REMOVED": "false",
               "AIRK_FAKE_PROBLEM": "", **environment}
        result = subprocess.run([SH, str(stage / "install.sh"), *arguments], capture_output=True, text=True,
                                env=env, stdin=subprocess.DEVNULL, timeout=60)
        calls = log.read_text().split() if log.exists() else []
        return result, calls

    @unittest.skipIf(SH is None, "a POSIX sh is not available on this host")
    def test_install_script_refuses_changed_linked_and_extra_payload_before_native_execution(self):
        cases = ("unchanged", "bytes", "inventory", "extra", "hard_link", "roster")
        for case in cases:
            with self.subTest(case=case):
                base = self.base / case
                base.mkdir()
                stage, _ = service_package(base)
                if case == "bytes":
                    (stage / "payload/bin/airkctl").write_bytes(b"changed airkctl\n")
                elif case == "inventory":
                    (stage / "payload/payload-inventory.json").write_text("{}\n")
                elif case == "extra":
                    (stage / "payload/notices/EXTRA").write_bytes(b"not in the roster\n")
                elif case == "hard_link":
                    os.link(stage / "payload/notices/LICENSE", base / "alias")
                elif case == "roster":
                    with (stage / "payload.sha256").open("a") as roster:
                        roster.write("0" * 64 + "  payload/notices/LICENSE\n")
                result, calls = self.run_installer(stage, "--yes", "--program-dir", str(base / "program"))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [], "no native program runs before the package verifies")
                self.assertFalse((base / "program").exists())
                if case == "unchanged":
                    if sys.platform.startswith("linux"):
                        continue
                    self.assertIn("this package is for Linux x86-64", result.stderr)
                else:
                    self.assertNotIn("this package is for Linux", result.stderr)
                    self.assertIn("AIRK installer:", result.stderr)

    @unittest.skipIf(SH is None or os.name == "nt", "symbolic links need a POSIX host")
    def test_install_script_refuses_a_symbolic_link_in_the_payload(self):
        stage, _ = service_package(self.base)
        os.symlink("/etc/passwd", stage / "payload/notices/link")
        result, calls = self.run_installer(stage, "--yes")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("linked or special file", result.stderr)
        self.assertEqual(calls, [])

    @unittest.skipUnless(SH and sys.platform.startswith("linux"), "runs the package on Linux only")
    def test_unattended_install_needs_the_reset_acknowledgement_when_setup_finds_predecessors(self):
        stage, _ = service_package(self.base)
        program = self.base / "program"
        result, calls = self.run_installer(stage, "--yes", "--program-dir", str(program), AIRK_FAKE_REMOVED="true")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--acknowledge-reset", result.stderr)
        self.assertEqual(calls, ["--plan"])
        self.assertFalse(program.exists())

    @unittest.skipUnless(SH and sys.platform.startswith("linux"), "runs the package on Linux only")
    def test_location_problems_stop_before_any_change(self):
        stage, _ = service_package(self.base)
        result, calls = self.run_installer(stage, "--yes", AIRK_FAKE_PROBLEM='"fixture problem"')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, ["--plan"])

    @unittest.skipUnless(SH and sys.platform.startswith("linux"), "runs the package on Linux only")
    def test_install_hands_one_canonical_operation_to_setup_places_the_layout_and_owns_the_link(self):
        stage, values = service_package(self.base)
        program = self.base / "program"
        locations = {"program": str(program), "data": str(self.base / "data/indexes"),
                     "state": str(self.base / "state/service"), "cache": str(self.base / "cache/service"),
                     "logs": str(self.base / "logs/service")}
        options = [f"--{role}-dir={path}/" for role, path in locations.items()]
        result, calls = self.run_installer(stage, "--yes", "--acknowledge-reset", *options, AIRK_FAKE_REMOVED="true")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, ["--plan", "--prepare", "complete"])
        log = self.base / "airk.log"
        prepared = (Path(str(log) + ".prepare.json")).read_bytes()
        self.assertEqual(prepared, Path(str(log) + ".complete.json").read_bytes(), "placement resumes one operation")
        operation = json.loads(prepared)
        self.assertEqual(list(operation), OPERATION_FIELDS)
        self.assertEqual(prepared, json.dumps(operation, separators=(",", ":")).encode() + b"\n", "canonical bytes")
        self.assertEqual(operation["locations"], locations)
        self.assertEqual(operation["requested_by"], "install-sh")
        self.assertEqual(operation["acknowledgements"], {"predecessor_removal": True, "index_reset": True})
        self.assertEqual(operation["options"], {"login_startup": True, "trust": "ask", "start": True,
                                                "delete_indexes": "keep"})
        self.assertEqual(operation["payload_inventory_sha256"], values["INVENTORY_SHA256"])
        self.assertEqual(operation["expires_unix_ms"] - operation["created_unix_ms"], 600000)
        self.assertFalse(json.loads(Path(str(log) + ".plan.json").read_bytes())["options"]["start"])
        for name, mode in (("service/airk", 0o755), ("bin/airkctl", 0o755), ("notices/LICENSE", 0o644),
                           ("payload-inventory.json", 0o644)):
            self.assertEqual((program / name).read_bytes(), (stage / "payload" / name).read_bytes(), name)
            self.assertEqual((program / name).stat().st_mode & 0o777, mode, name)
        link = self.base / "home/.local/bin/airkctl"
        self.assertEqual(os.readlink(link), str(program / "bin/airkctl"))
        self.assertEqual((program / "airkctl-link-owner.txt").read_text(),
                         f"airk-airkctl-link/1\n{link}\n{program / 'bin/airkctl'}\n")


if __name__ == "__main__":
    unittest.main()
