# SPDX-License-Identifier: Apache-2.0
"""Owned Neo Aqua qualification, launched only after the frozen unsigned batch.

This artifact is not a release workflow. It uses the existing identity/profile,
never changes keychain settings or OS trust, and never publishes anything.
"""
import argparse
import ctypes
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import plistlib
import signal
import socket
import stat
import subprocess
import sys
import time
import uuid
import zipfile

TARGET = "aarch64-apple-darwin"
LEAF = "4d8496dcab73794378194d040d9177fb28a0f982"
TEAM = "5PX7D99PXA"
KEYCHAIN = Path("/Users/tylerjensen/Library/Keychains/airk-signing.keychain-db")
CODE = Path("/Applications/Visual Studio Code.app/Contents/MacOS/Code")
CLI = CODE.parent.parent / "Resources/app/out/cli.js"
OLD_VSIX_SHA = "71518c9d89fd0fb4f54e0d046b2f85948a361c7303ad7c49a60df568dfb4219d"
HOT_BASELINE = Path("/Users/tylerjensen/airk-builds/explorer-notary-1812-ee5ad77-20261107/source3c6e81c/artifacts/neo/vscode-notarized-3c/publish/airk-1.8.12-darwin-arm64.vsix")
HOT_BASELINE_SHA = "022f3869379f163c1f0d28371eb134b78cbc4e5d92125fe5e57a1f12c1eee621"
HOT_BASELINE_BYTES = 108794760
PINNED_PYTHON = Path("/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13")
OLD_RUNTIME = Path("/Users/tylerjensen/airk-builds/explorer-notary-1812-ee5ad77-20261107/source3c6e81c/artifacts/neo/signed-notarized-3c/runtime")
OLD_INSTALL_SHA = "47f0b790645a707077c00015e5e38cc6256b57e0510f6da97c6c2db25002e038"
OLD_NATIVE_SOURCE = {"sha256":"51536a705cf9cbabe4947cacddf43a8cb29b0af8bad4e23ca7f293aa6dd347fa", "files":978}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load(source, name):
    spec = importlib.util.spec_from_file_location("signed_neo_" + name.replace("-", "_"), source / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_private(path, value):
    temporary = path.with_name(path.name + ".pending")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def native(command):
    result = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, timeout=20)
    if len(result.stdout) > 65536 or len(result.stderr) > 4096:
        raise RuntimeError("Owned launchd observation exceeded its allowance")
    return result


def pid_path(pid):
    lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    function = lib.proc_pidpath
    function.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    function.restype = ctypes.c_int
    data = ctypes.create_string_buffer(4096)
    ctypes.set_errno(0)
    count = function(pid, data, len(data))
    if count <= 0:
        error = ctypes.get_errno()
        if error == errno.ESRCH:
            return None
        raise OSError(error, "Owned GUI process identity unavailable")
    return data.value.decode("utf-8")


def receipt_archive(publish, receipt):
    name = receipt.get("archive")
    if not isinstance(name, str) or not name or Path(name).name != name or "/" in name or "\\" in name:
        raise RuntimeError("Candidate archive receipt has an unsafe filename")
    archive = publish / name
    if archive.is_symlink() or not archive.is_file() or digest(archive) != receipt.get("archive_sha256"):
        raise RuntimeError("Candidate archive differs from its exact publish receipt")
    return archive


def launch_pid(output):
    depth = 0
    pid = None
    for line in output.decode("utf-8").splitlines():
        value = line.strip()
        if depth == 1 and value.startswith("pid = "):
            if pid is not None:
                raise RuntimeError("Duplicate owned GUI job PID")
            pid = int(value.removeprefix("pid = "))
            if not 0 < pid <= 2**31 - 1:
                raise RuntimeError("Invalid owned GUI job PID")
        depth += int(value.endswith("{")) - int(value == "}")
        if not 0 <= depth <= 16:
            raise RuntimeError("Unknown owned GUI job observation")
    if depth:
        raise RuntimeError("Incomplete owned GUI job observation")
    return pid


def verified_context(args):
    if platform.system() != "Darwin" or platform.machine() != "arm64" or os.getuid() != 501 or socket.gethostname() != "Tylers-MacBook-Neo.local":
        raise RuntimeError("Requires the confirmed Neo ARM standard-user501")
    campaign = args.campaign.resolve(strict=True)
    if campaign.parent != Path("/Users/tylerjensen/airk-builds") or not campaign.name.startswith("one-airk-1814-final-"):
        raise RuntimeError("Unexpected owned campaign path")
    if digest(Path(__file__).resolve(strict=True)) != args.runner_sha256:
        raise RuntimeError("Signed runner differs from its reviewed bytes")
    source = (campaign / "source").resolve(strict=True)
    receipt = campaign / "evidence/batch-receipt.json"
    if receipt.is_symlink() or digest(receipt) != args.unsigned_receipt_sha256:
        raise RuntimeError("Unsigned batch receipt differs")
    unsigned = json.loads(receipt.read_text())
    manifest = json.loads((campaign / "source-manifest.json").read_text())
    builder = load(source, "build-service-runtime")
    if (unsigned.get("result") != "passed" or unsigned.get("cargo_cleanup") != "completed"
            or unsigned.get("compiler_tree_absent_or_empty") is not True
            or unsigned.get("all_owned_command_descendants_settled") is not True
            or manifest.get("status") != "FROZEN_PUBLIC_SOURCE" or manifest.get("final_freeze_required") is not False
            or builder.source_identity() != unsigned["source_identity"]
            or manifest["native_source"] != unsigned["source_identity"]
            or digest(campaign / "source-manifest.json") != unsigned["source_manifest_sha256"]):
        raise RuntimeError("Unsigned qualification/source/cleanup boundary is not authenticated")
    for row in manifest["files"]:
        relative = Path(row["path"])
        path = source / relative
        if relative.is_absolute() or ".." in relative.parts or path.is_symlink() or not path.is_file() or path.stat().st_size != row["bytes"] or digest(path) != row["sha256"]:
            raise RuntimeError("Frozen public source changed: " + row["path"])
    observed = native(["/usr/bin/stat", "-f", "%u", "/dev/console"])
    if observed.returncode or observed.stdout.strip() != b"501" or native(["/bin/launchctl", "print", "gui/501"]).returncode:
        raise RuntimeError("Same-UID active Aqua session is not available")
    runtime = (source / "artifacts/neo-final/runtime").resolve(strict=True)
    if str(runtime) != unsigned["runtime"]["path"] or digest(runtime / "airk-install.json") != unsigned["runtime"]["install_manifest_sha256"] or digest(runtime / "airk-build.json") != unsigned["runtime"]["build_receipt_sha256"]:
        raise RuntimeError("Original compiled runtime differs from its tested producer")
    package = load(source, "package-native")
    inventory, _ = package.validate_runtime(runtime, "1.8.14", TARGET, unsigned["source_identity"])
    old_vsix = args.previous_vsix.resolve(strict=True)
    if args.previous_vsix.is_symlink() or not old_vsix.is_file() or digest(old_vsix) != OLD_VSIX_SHA:
        raise RuntimeError("Historical signed1.8.7 ARM VSIX differs from the authenticated public archive")
    if (HOT_BASELINE.is_symlink() or not HOT_BASELINE.is_file() or HOT_BASELINE.stat().st_size != HOT_BASELINE_BYTES
            or digest(HOT_BASELINE) != HOT_BASELINE_SHA):
        raise RuntimeError("Genuine loader-capable1.8.12 VSIX differs from the reviewed historical archive")
    if Path(sys.executable).resolve(strict=True) != PINNED_PYTHON.resolve(strict=True):
        raise RuntimeError("Signed qualification must use the pinned CPython3.13 executable")
    old_build = json.loads((OLD_RUNTIME / "airk-build.json").read_text())
    if digest(OLD_RUNTIME / "airk-install.json") != OLD_INSTALL_SHA or old_build.get("source") != OLD_NATIVE_SOURCE:
        raise RuntimeError("Historical1.8.12 compiled producer/install inventory differs")
    package.validate_runtime(OLD_RUNTIME, "1.8.12", TARGET, OLD_NATIVE_SOURCE)
    return campaign, source, runtime, unsigned, inventory


def worker(args):
    campaign, source, runtime, unsigned, inventory = verified_context(args)
    evidence = campaign / "signed-evidence"
    if evidence.is_symlink() or evidence.stat().st_uid != 501 or stat.S_IMODE(evidence.stat().st_mode) != 0o700:
        raise RuntimeError("Owned signed evidence directory changed")
    owner = load(source, "provision-static-onnx-runtime")
    package = load(source, "package-native")
    builder = load(source, "build-service-runtime")
    tools = Path("/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools")
    fixtures = campaign / "signed-fixtures"
    fixtures.mkdir(mode=0o700, exist_ok=False)
    if fixtures.is_symlink() or fixtures.stat().st_uid != 501 or stat.S_IMODE(fixtures.stat().st_mode) != 0o700:
        raise RuntimeError("Signed fixture parent is not an owned private directory")
    environment = dict(os.environ)
    environment.update(PATH=f"{Path.home() / '.cargo/bin'}:{tools / 'node-v24.15.0-darwin-arm64/bin'}:{tools}:{Path(sys.executable).parent}:" + os.environ.get("PATH", ""),
                       PYTHONDONTWRITEBYTECODE="1", PYTHONNOUSERSITE="1", MACOSX_DEPLOYMENT_TARGET="13.3",
                       AIRK_PRIVATE_CENSUS_BASE=str(fixtures), TMPDIR=str(fixtures), TEMP=str(fixtures), TMP=str(fixtures))
    for name in ("CARGO_INCREMENTAL", "AIRK_TEST_BINARY", "AIRK_STATIC_ORT_ENVIRONMENT", "ORT_DYLIB_PATH",
                 "MACOS_CERTIFICATE_P12", "MACOS_CERTIFICATE_PASSWORD", "APPLE_ID", "APPLE_APP_SPECIFIC_PASSWORD"):
        environment.pop(name, None)
    environment.update(load(source, "static-runtime-environment").load_environment_file(
        Path("/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059/static-environment-recovery.json"), target=TARGET))
    report = {"schema":1, "result":"running", "host":"192.168.68.61", "worker_pid":os.getpid(),
              "worker_executable":pid_path(os.getpid()), "uid":501, "commands":[], "source_identity":unsigned["source_identity"],
              "original_compiled_producer":json.loads((runtime / "airk-build.json").read_text()),
              "unsigned_receipt_sha256":args.unsigned_receipt_sha256, "runner_sha256":args.runner_sha256,
              "no_cargo_or_build":True, "os_trust_mutated":False,
              "fixture_parent_preserved":str(fixtures),
              "historical_vsix":{"path":str(args.previous_vsix.resolve(strict=True)), "sha256":OLD_VSIX_SHA, "version":"1.8.7"},
              "historical_hot_update_vsix":{"path":str(HOT_BASELINE), "sha256":HOT_BASELINE_SHA,
                                             "bytes":HOT_BASELINE_BYTES, "version":"1.8.12",
                                             "scope":"genuine original3c notarized ARM loader-capable package; no native execution in hot UI gate"},
              "global_deadline_seconds":7200,
              "deadline_qualifier":"Individual stage ceilings may sum above the existing global bound; any global timeout is failure/unknown, never partial acceptance",
              "historical_semantic_runtime":{"path":str(OLD_RUNTIME), "version":"1.8.12", "install_manifest_sha256":OLD_INSTALL_SHA,
                                             "source":OLD_NATIVE_SOURCE, "original_build_receipt_sha256":digest(OLD_RUNTIME / "airk-build.json")},
              "qualification_limits":["ARM Neo only; no Intel or oldest-macOS claim", "private CA fixtures do not prove OS root enrollment/removal", "Selected native HTTPS SAME, native-disabled hot refresh and genuine old-to-current editor upgrade only; other editor scenarios remain separate", "old launcher rejection/SIGKILL proves failed-install recovery, not invented retained old indexed data", "no public release or Marketplace acceptance"]}
    started = time.monotonic()
    def save():
        report["elapsed_seconds"] = round(time.monotonic() - started, 3)
        write_private(evidence / "signed-batch-receipt.json", report)
    def run(name, arguments, timeout=1800, extra=None):
        row = {"stage":name, "argv":list(map(str, arguments)), "result":"running"}
        report["commands"].append(row)
        save()
        result = owner.run_owned(row["argv"], source, evidence / (name + ".log"), timeout, {**environment, **(extra or {})})
        row.update(result)
        row["result"] = "passed" if owner.successful(result) else "failed"
        save()
        if row["result"] != "passed":
            raise RuntimeError("Collect the complete failed stage before correction: " + name)
    def interrupted(_number, _frame):
        raise InterruptedError("Owned Aqua campaign interrupted; retain all evidence")
    signal.signal(signal.SIGTERM, interrupted)
    signed = source / "artifacts/neo-final/signed-notarized"
    native_output = source / "artifacts/neo-final/native-signed"
    vsix_output = source / "artifacts/neo-final/vscode-signed"
    try:
        public = load(source, "ci-macos-sign")
        if public.imported_identity(KEYCHAIN) != LEAF:
            raise RuntimeError("Existing public certificate differs from the pinned identity")
        run("Developer-ID-and-notary", [sys.executable, source / "scripts/sign-native.py", "--target", TARGET,
            "--runtime-dir", runtime, "--identity", LEAF, "--team-id", TEAM, "--keychain", KEYCHAIN,
            "--notarize", "--keychain-profile", "airk-notary", "--output", signed])
        report["public_certificate"] = public.verify_signed_output(signed, True, LEAF)
        signer = load(source, "sign-native")
        signed_runtime, signing = signer.read_verified_pair(signed, runtime, TARGET, unsigned["git_producer"], digest(source / "Cargo.lock"))
        report["signing_receipt_sha256"] = digest(signed / "signing.json")
        report["signed_install_sha256"] = digest(signed_runtime / "airk-install.json")
        report["copied_launcher"] = "not applicable: schema2 single sealed app main; no standalone launcher"
        for component in signing["components"].values():
            ticket = component.get("notarization", {})
            if ticket.get("status") != "Accepted" or ticket.get("stapled") is not True:
                raise RuntimeError("Signed runtime lacks Accepted stapled notarization")
        save()
        requirement = f'=anchor apple generic and certificate leaf[subject.OU] = "{TEAM}" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf = H"{LEAF}"'
        run("strict-signed-app", ["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R", requirement, signed_runtime / "AIRK.app"], 120)
        run("stapled-app", ["/usr/bin/xcrun", "stapler", "validate", signed_runtime / "AIRK.app"], 120)
        run("Gatekeeper-app", ["/usr/sbin/spctl", "--assess", "--type", "execute", "--verbose=4", signed_runtime / "AIRK.app"], 120)
        run("signed-1.8.12-semantic-upgrade", [sys.executable, source / "scripts/check-runtime-upgrade.py", "--runtime", signed_runtime,
            "--previous-runtime", OLD_RUNTIME, "--source-state", "indexed", "--fixture-setup", "reviewed", "--semantic-old",
            "--output", evidence / "signed-1.8.12-semantic-upgrade.json"], 1800)
        run("signed-actual-model-HTTPS", [sys.executable, source / "scripts/check-service-clients.py", "--runtime-dir", signed_runtime, "--files", "16", "--model", "--report", evidence / "signed-model-clients.json"])
        # Execute the identical post-sign model/install gate used by hosted CI.
        # This child never imports a key, changes a keychain or calls ci.sign().
        run("shared-CI-post-sign-execution", [sys.executable, Path(__file__).resolve(), "post-sign-ci-gate",
            "--campaign", campaign, "--unsigned-receipt-sha256", args.unsigned_receipt_sha256,
            "--runner-sha256", args.runner_sha256, "--previous-vsix", args.previous_vsix], 1800)
        shared_receipt = signed / "post-sign-validation.json"
        shared = json.loads(shared_receipt.read_text())
        if (shared.get("schema") != "airk-macos-post-sign-validation/1" or shared.get("status") != "passed"
                or shared.get("target") != TARGET or shared.get("version") != "1.8.14"
                or shared.get("install_manifest_sha256") != report["signed_install_sha256"]
                or shared.get("executable_sha256") != digest(signed_runtime / "AIRK.app/Contents/MacOS/airk")
                or shared.get("stages") != ["signed_embedded_model_passed", "signed_private_installation_passed"]):
            raise RuntimeError("Shared CI post-sign acceptance receipt differs from the exact signed runtime")
        report["shared_CI_post_sign"] = {"receipt": shared, "receipt_sha256": digest(shared_receipt),
                                        "helper_sha256": digest(source / "scripts/ci-macos-sign.py")}
        save()
        run("signed-native-package", [sys.executable, source / "scripts/package-native.py", "--target", TARGET, "--runtime-dir", runtime, "--signed-dir", signed, "--output", native_output], 900)
        run("signed-ARM-VSIX", [sys.executable, source / "scripts/package-extension.py", "--target", TARGET, "--native-dir", native_output / "publish", "--output", vsix_output], 900)
        native_manifest = json.loads((native_output / "publish/manifest.json").read_text())
        if (native_manifest.get("schema") != "airk-native-candidate/2" or native_manifest.get("version") != "1.8.14"
                or native_manifest.get("target") != TARGET or native_manifest.get("vsix_target") != "darwin-arm64"
                or native_manifest.get("commit") != unsigned["git_producer"]
                or native_manifest.get("source") != unsigned["source_identity"] or native_manifest.get("signed_distribution") is not True
                or native_manifest.get("install_manifest_sha256") != report["signed_install_sha256"]):
            raise RuntimeError("Final native receipt platform/version/source/signing identity differs")
        archive = receipt_archive(native_output / "publish", native_manifest)
        # Canonical extraction verifies exact entry topology, executable modes,
        # and bytes. Keep the transferred app inside its complete sealed bundle.
        transferred = source / "artifacts/neo-final/transferred-signed-runtime"
        executables = builder.runtime_executables("macos", "1.8.14")
        package.verify_archive(archive, native_manifest["payload"], executables)
        transferred.mkdir(mode=0o700, exist_ok=False)
        with zipfile.ZipFile(archive) as zipped:
            for name in native_manifest["payload"]:
                relative = Path(name)
                if relative.is_absolute() or ".." in relative.parts:
                    raise RuntimeError("Transfer archive path escapes its owned directory")
                mode = zipped.getinfo(name).external_attr >> 16
                if not stat.S_ISREG(mode) or stat.S_IMODE(mode) != (0o755 if name in executables else 0o644):
                    raise RuntimeError("Transfer archive entry type/executable mode differs from canonical packaging")
                output = transferred / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                with output.open("xb") as stream, zipped.open(name) as original:
                    import shutil
                    shutil.copyfileobj(original, stream, length=1048576)
                output.chmod(0o755 if name in executables else 0o644)
        package.validate_runtime(transferred, "1.8.14", TARGET, unsigned["source_identity"])
        run("transferred-strict-app", ["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "-R", requirement, transferred / "AIRK.app"], 120)
        run("transferred-stapled-app", ["/usr/bin/xcrun", "stapler", "validate", transferred / "AIRK.app"], 120)
        run("transferred-Gatekeeper-app", ["/usr/sbin/spctl", "--assess", "--type", "execute", "--verbose=4", transferred / "AIRK.app"], 120)
        run("transferred-version", [transferred / "AIRK.app/Contents/MacOS/airk", "--version"], 120)
        run("actual-SMAppService-public-login", [sys.executable, source / "scripts/check-macos-direct-runtime.py", "--runtime", transferred,
            "--approved-sha256", report["signed_install_sha256"], "--evidence-parent", evidence, "--allow-preview-registration"])
        extension = load(source, "package-extension")
        vsix_manifest = json.loads((vsix_output / "publish/manifest.json").read_text())
        if (vsix_manifest.get("schema") != "airk-extension-candidate/1" or vsix_manifest.get("version") != "1.8.14"
                or vsix_manifest.get("platform") != "darwin-arm64" or vsix_manifest.get("extension_id") != "iokaio.airk"
                or vsix_manifest.get("commit") != unsigned["git_producer"]
                or vsix_manifest.get("native_archive_sha256") != native_manifest["archive_sha256"]
                or vsix_manifest.get("native_manifest_sha256") != digest(native_output / "publish/manifest.json")):
            raise RuntimeError("Final extension receipt platform/version/native identity differs")
        vsix = receipt_archive(vsix_output / "publish", vsix_manifest)
        extension.verify_vsix(vsix, vsix_manifest["payload"], "native/runtime/AIRK.app/Contents/MacOS/airk", "darwin-arm64", "1.8.14",
                              vsix_manifest["pre_release"], vsix_manifest["display_name"])
        with zipfile.ZipFile(vsix) as zipped:
            extension.read_update_inventory(zipped, "1.8.14", "darwin-arm64")
        report["final_artifacts"] = {"native_zip":str(archive), "native_sha256":digest(archive), "vsix":str(vsix), "vsix_sha256":digest(vsix)}
        report["editor"] = {"path":str(CODE), "cli":str(CLI), "application":json.loads((CLI.parents[1] / "package.json").read_text()) ["version"]}
        save()
        if digest(HOT_BASELINE) != HOT_BASELINE_SHA or digest(vsix) != report["final_artifacts"]["vsix_sha256"]:
            raise RuntimeError("Baseline/current VSIX changed before same-host Explorer hot replacement")
        report["hot_explorer_UI"] = {"baseline_sha256":HOT_BASELINE_SHA,
            "candidate_sha256":report["final_artifacts"]["vsix_sha256"], "native_execution":False,
            "helper_sha256":digest(source / "scripts/check-explorer-hot-update.cjs"),
            "profile_helper_sha256":digest(source / "scripts/editor-owned-profile.cjs"), "python":str(PINNED_PYTHON),
            "scope":"actual same-Code-host automatic asset refresh, singleton and saved notice dismissal across screens/reopen"}
        save()
        run("actual-hot-Explorer-refresh-singleton-dismissal", ["node", source / "scripts/check-explorer-hot-update.cjs",
            CODE, CLI, HOT_BASELINE, HOT_BASELINE_SHA, vsix, report["final_artifacts"]["vsix_sha256"]],
            600, {"PYTHON":str(PINNED_PYTHON)})
        run("actual-installed-editor-SAME-HTTPS", ["node", source / "scripts/check-native-editor.cjs", CODE, CLI, vsix], 2400, {"AIRK_EDITOR_SCENARIOS":"same"})
        # Only after the complete SAME journey and its canonical cleanup pass:
        # reproduce the authenticated old launcher failure in private profiles,
        # then recover with the exact current signed VSIX through real HTTPS.
        if digest(args.previous_vsix.resolve(strict=True)) != OLD_VSIX_SHA or digest(vsix) != report["final_artifacts"]["vsix_sha256"]:
            raise RuntimeError("Old/current VSIX changed before genuine installed-editor recovery")
        run("actual-signed-1.8.7-to-current-VSIX-recovery", ["node", source / "scripts/check-vsix-upgrade-editor.cjs", CODE, CLI,
            args.previous_vsix.resolve(strict=True), vsix, report["final_artifacts"]["vsix_sha256"]], 3600)
        if builder.source_identity() != unsigned["source_identity"] or package.inventory(runtime) != inventory:
            raise RuntimeError("Original compiled producer/source changed during qualification")
        report["result"] = "passed_selected_ARM_signed_acceptance"
    except BaseException as error:
        report["result"] = "failed_retained"
        report["error"] = str(error)
    finally:
        report["all_owned_command_descendants_settled"] = all(row.get("owned_descendants_settled") is True for row in report["commands"])
        if not report["all_owned_command_descendants_settled"]:
            report["result"] = "failed_retained"
        report["compiler_target_still_absent_or_empty"] = not (campaign / "cargo-target").exists() or not any((campaign / "cargo-target").iterdir())
        save()
    return 0 if report["result"] == "passed_selected_ARM_signed_acceptance" else 1


def launch(args):
    campaign, source, _runtime, _unsigned, _inventory = verified_context(args)
    evidence = campaign / "signed-evidence"
    evidence.mkdir(mode=0o700, exist_ok=False)
    label = "io.iokaio.airk.qualification." + uuid.uuid4().hex
    target = "gui/501/" + label
    plist = evidence / "owned-Aqua-job.plist"
    arguments = [sys.executable, str(Path(__file__).resolve()), "worker", "--campaign", str(campaign),
                 "--unsigned-receipt-sha256", args.unsigned_receipt_sha256, "--runner-sha256", args.runner_sha256,
                 "--previous-vsix", str(args.previous_vsix.resolve(strict=True))]
    record = {"schema":1, "label":label, "target":target, "plist":str(plist), "argv":arguments,
              "result":"running", "job_removed":False, "plist_removed":False, "worker_exit_verified":False}
    observed = native(["/bin/launchctl", "print", target])
    if observed.returncode == 0 or f'Could not find service "{label}"'.encode() not in observed.stderr:
        raise RuntimeError("Owned launch label is not proven unused")
    with plist.open("xb") as stream:
        plistlib.dump({"Label":label, "ProgramArguments":arguments, "WorkingDirectory":str(source), "RunAtLoad":True,
                      "ProcessType":"Interactive", "LimitLoadToSessionType":"Aqua", "KeepAlive":False,
                      "StandardOutPath":str(evidence / "Aqua.stdout.log"), "StandardErrorPath":str(evidence / "Aqua.stderr.log"),
                      "EnvironmentVariables":{"PYTHONNOUSERSITE":"1", "PYTHONDONTWRITEBYTECODE":"1", "PATH":"/usr/bin:/bin:/usr/sbin:/sbin"}}, stream)
    plist.chmod(0o600)
    plist_sha = digest(plist)
    record["plist_sha256"] = plist_sha
    result_path = evidence / "signed-batch-receipt.json"
    pid = image = None
    bootstrapped = False
    try:
        result = native(["/bin/launchctl", "bootstrap", "gui/501", str(plist)])
        if result.returncode:
            raise RuntimeError("Owned Aqua bootstrap failed: " + result.stderr.decode("utf-8", errors="replace"))
        bootstrapped = True
        deadline = time.monotonic() + 7200
        previous_stage = None
        while True:
            job = native(["/bin/launchctl", "print", target])
            if job.returncode:
                raise RuntimeError("Owned Aqua job observation disappeared")
            current_pid = launch_pid(job.stdout)
            if current_pid is not None:
                current_image = pid_path(current_pid)
                if Path(current_image).resolve() != Path(sys.executable).resolve() or (pid is not None and pid != current_pid):
                    raise RuntimeError("Owned Aqua worker identity changed")
                pid, image = current_pid, current_image
            if result_path.exists():
                report = json.loads(result_path.read_text())
                if report["worker_pid"] != pid:
                    raise RuntimeError("Signed worker receipt PID is not the observed Aqua job")
                stage = report["commands"][-1]["stage"] if report["commands"] else "preflight"
                if stage != previous_stage:
                    print("Neo signed Aqua phase: " + stage, flush=True)
                    previous_stage = stage
                if report["result"] != "running":
                    record["worker_result"] = report["result"]
                    record["worker_receipt_sha256"] = digest(result_path)
                    record["result"] = "passed" if report["result"] == "passed_selected_ARM_signed_acceptance" else "failed_retained"
                    break
            elif current_pid is None and b"last exit code = " in job.stdout:
                raise RuntimeError("Owned Aqua worker exited before producing an authenticated receipt; retain stderr")
            if time.monotonic() >= deadline:
                raise TimeoutError("Owned Aqua qualification deadline reached")
            time.sleep(1)
    except BaseException as error:
        record["result"] = "failed_retained"
        record["error"] = str(error)
        if bootstrapped:
            native(["/bin/launchctl", "kill", "SIGTERM", target])
    finally:
        # Never change Login Items/startup settings: remove only this exact
        # ephemeral job and immutable owned plist, then verify worker absence.
        try:
            if pid is not None:
                end = time.monotonic() + 45
                while (path := pid_path(pid)) is not None:
                    if path != image or time.monotonic() >= end:
                        raise RuntimeError("Owned GUI worker exit remains unknown")
                    time.sleep(.1)
                record["worker_exit_verified"] = True
            if bootstrapped:
                result = native(["/bin/launchctl", "bootout", target])
                if result.returncode:
                    raise RuntimeError("Owned Aqua bootout failed")
                absent = native(["/bin/launchctl", "print", target])
                if absent.returncode == 0 or f'Could not find service "{label}"'.encode() not in absent.stderr:
                    raise RuntimeError("Owned Aqua job removal remains unknown")
                record["job_removed"] = True
            if plist.is_symlink() or digest(plist) != plist_sha:
                raise RuntimeError("Owned Aqua plist changed; preserved")
            plist.unlink()
            record["plist_removed"] = True
        except BaseException as error:
            record["result"] = "failed_retained"
            record["cleanup_error"] = str(error)
        write_private(evidence / "Aqua-job-cleanup.json", record)
        print(json.dumps({"result":record["result"], "evidence":str(evidence), "job_removed":record["job_removed"], "worker_exit_verified":record["worker_exit_verified"]}), flush=True)
    return 0 if record["result"] == "passed" else 1



def post_sign_ci_gate(args):
    # A narrow execution entrypoint, not the CI keychain-provisioning entrypoint.
    _campaign, source, _runtime, _unsigned, _inventory = verified_context(args)
    public = load(source, "ci-macos-sign")
    if any(name in os.environ for name in public.SECRET_NAMES):
        raise RuntimeError("Shared post-sign gate must receive no CI signing credential environment")
    result = public.qualify_signed_runtime(source / "artifacts/neo-final/signed-notarized", True, TARGET)
    if not isinstance(result, dict) or result.get("status") != "passed":
        raise RuntimeError("Shared CI post-sign gate did not qualify the modern signed runtime")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("launch", "worker", "post-sign-ci-gate"))
    parser.add_argument("--campaign", type=Path, required=True)
    parser.add_argument("--unsigned-receipt-sha256", required=True)
    parser.add_argument("--runner-sha256", required=True)
    parser.add_argument("--previous-vsix", type=Path, required=True,
                        help="Authenticated public signed1.8.7 darwin-arm64 VSIX; exact SHA pinned independently")
    args = parser.parse_args()
    if args.mode == "post-sign-ci-gate":
        return post_sign_ci_gate(args)
    return launch(args) if args.mode == "launch" else worker(args)


if __name__ == "__main__":
    raise SystemExit(main())
