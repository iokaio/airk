# SPDX-License-Identifier: Apache-2.0
"""Build a fresh, unsealed cfg(test) native-owner pair; never run qualification.

Explicit frozen input manifest required. No Git discovery, provisioning,
installation, signing, release sealing, source edits, or automatic output removal.
Partial output remains owned and identifiable after failure for review.
"""
import argparse
import base64
import ctypes
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import signal
import stat
import struct
import subprocess
import sys
import time
import tomllib

sys.dont_write_bytecode = True
SELECTORS = [
    "project_supervisor::guarded_qualification_tests::ordinary_exit_keeps_real_provider_work_and_sibling_ownership_until_engine_release",
    "project_supervisor::guarded_qualification_tests::retire_requires_actual_engine_release_and_complete_native_worker_obligation",
    "project_supervisor::guarded_qualification_tests::restart::restored_native_worker_obligation",
]
FIXED_INPUTS = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "LICENSE", "NOTICE",
    "THIRD_PARTY_NOTICES.md", "scripts/build-service-runtime.py",
    "scripts/provision-onnx-runtime.py", "scripts/provision-local-model.py",
    "scripts/int8-policy.py", "scripts/macho-identity.py", "scripts/license-notices.py",
}
REQUIRED = FIXED_INPUTS | {
    ".cargo/config.toml", "crates/airk-cli/Cargo.toml", "crates/airk-cli/build.rs",
    "crates/airk-cli/src/embed_main.rs", "crates/airk-cli/src/embed_request.rs",
    "crates/airk-cli/src/embed_pool.rs", "crates/airk-cli/src/embed_pool_core.rs",
    "crates/airk-cli/src/embed_pool_retirement.rs",
    "crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs",
    "crates/airk-cli/src/project_supervisor_guarded_gate_tests.rs",
    "crates/airk-cli/src/project_supervisor_guarded_restart_tests.rs",
    "crates/airk-cli/tests/fixtures/guarded_provider_fixture.rs",
    "crates/airk-cli/tests/fixtures/guarded_provider_gate.rs",
    "crates/airk-cli/tests/fixtures/guarded_provider_gate_protocol.rs",
}
EXCLUDED_TOP = {".git", ".codex", ".agents", ".aws", ".airk", ".mcp.json", "temp", "target", "artifacts", "node_modules"}


def command_worker(encoded, deadline):
    """No native build can start until the owning parent installs containment."""
    command = json.loads(base64.b64decode(encoded, validate=True))
    require(isinstance(command, list) and 0 < len(command) <= 256
            and all(isinstance(value, str) and "\0" not in value for value in command), "Invalid owned command")
    require(math.isfinite(deadline) and time.monotonic() < deadline, "Owned command deadline expired")
    if os.read(0, 1) != b"\x01":
        return 125
    if os.name != "nt":
        require(os.getpgrp() == os.getpid(), "Build child is not its private session leader")
        # Best-effort parent lifeline, confined to this ignored build fixture.
        # Abrupt parent-death cleanup is not qualified or reported as complete.
        import threading
        def parent_lifetime():
            os.read(0, 1)
            if os.getpgrp() == os.getpid():
                os.killpg(os.getpid(), signal.SIGKILL)
            os._exit(125)
        threading.Thread(target=parent_lifetime, daemon=True).start()
    try:
        result = subprocess.run(command, stdin=subprocess.DEVNULL,
                                creationflags=0x08000000 if os.name == "nt" else 0,
                                timeout=max(0.001, deadline - time.monotonic()))
        return result.returncode
    except subprocess.TimeoutExpired:
        return 124  # The actual parent still drains the entire native job/group.


class WindowsBuildJob:
    """Private anonymous native ownership; never adopts a foreign job/PID."""
    def __init__(self):
        from ctypes import wintypes as w
        class Limits(ctypes.Structure):
            _fields_ = [("process_time", ctypes.c_int64), ("job_time", ctypes.c_int64),
                        ("flags", w.DWORD), ("minimum", ctypes.c_size_t), ("maximum", ctypes.c_size_t),
                        ("active", w.DWORD), ("affinity", ctypes.c_size_t),
                        ("priority", w.DWORD), ("scheduling", w.DWORD)]
        class Extended(ctypes.Structure):
            _fields_ = [("basic", Limits), ("io", ctypes.c_uint64 * 6),
                        ("process_memory", ctypes.c_size_t), ("job_memory", ctypes.c_size_t),
                        ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]
        class Members(ctypes.Structure):
            _fields_ = [("assigned", w.DWORD), ("listed", w.DWORD), ("pids", ctypes.c_size_t * 4096)]
        class Accounting(ctypes.Structure):
            _fields_ = [("user", ctypes.c_int64), ("kernel", ctypes.c_int64),
                        ("period_user", ctypes.c_int64), ("period_kernel", ctypes.c_int64),
                        ("page_faults", w.DWORD), ("total", w.DWORD),
                        ("active", w.DWORD), ("terminated", w.DWORD)]
        self.Extended, self.Members, self.Accounting, self.Bool = Extended, Members, Accounting, w.BOOL
        self.k = ctypes.WinDLL("kernel32", use_last_error=True)
        declarations = {
            "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
            "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
            "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
            "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
            "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
            "OpenProcess": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
            "IsProcessInJob": ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
            "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
            "CloseHandle": ([w.HANDLE], w.BOOL),
        }
        for name, (arguments, result) in declarations.items():
            function = getattr(self.k, name)
            function.argtypes, function.restype = arguments, result
        self.handle = self.k.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        self.handles = []
        try:
            self.set_limits(False)
        except BaseException:
            self.k.CloseHandle(self.handle)
            raise

    def set_limits(self, close_creation):
        limits = self.Extended()
        limits.basic.flags = 0x2000 | (0x8 if close_creation else 0)  # KILL_ON_CLOSE / ACTIVE_PROCESS
        limits.basic.active = 1 if close_creation else 0
        if not self.k.SetInformationJobObject(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)):
            raise ctypes.WinError(ctypes.get_last_error())

    def assign(self, process):
        if not self.k.AssignProcessToJobObject(self.handle, int(process._handle)):
            raise ctypes.WinError(ctypes.get_last_error())

    def drain(self, deadline):
        errors = []
        try:
            self.set_limits(True)  # No child can create another descendant.
            members = self.Members()
            if not self.k.QueryInformationJobObject(self.handle, 3, ctypes.byref(members), ctypes.sizeof(members), None):
                raise ctypes.WinError(ctypes.get_last_error())
            require(members.assigned == members.listed and members.listed <= 4096, "Build member census incomplete")
            for pid in members.pids[:members.listed]:
                process = self.k.OpenProcess(0x100000 | 0x1000, False, pid)
                if not process:
                    if ctypes.get_last_error() == 87:  # Original process already gone.
                        continue
                    raise ctypes.WinError(ctypes.get_last_error())
                associated = self.Bool()
                if not self.k.IsProcessInJob(process, self.handle, ctypes.byref(associated)):
                    self.k.CloseHandle(process)
                    raise ctypes.WinError(ctypes.get_last_error())
                if associated.value:
                    self.handles.append(process)
                else:
                    self.k.CloseHandle(process)  # Reused PID never authorizes a signal.
        except Exception as error:
            errors.append(str(error))
        if not self.k.TerminateJobObject(self.handle, 125):
            errors.append(str(ctypes.WinError(ctypes.get_last_error())))
        while time.monotonic() < deadline:
            accounting = self.Accounting()
            if not self.k.QueryInformationJobObject(self.handle, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None):
                errors.append(str(ctypes.WinError(ctypes.get_last_error())))
                break
            active = accounting.active
            waits = [self.k.WaitForSingleObject(handle, 0) for handle in self.handles]
            if active == 0 and all(wait == 0 for wait in waits):
                return {"complete": not errors, "errors": errors, "active": 0, "captured_processes": len(waits)}
            time.sleep(0.02)
        return {"complete": False, "errors": errors, "reason": "Owned Windows build descendants unresolved"}

    def close(self):
        for handle in self.handles:
            self.k.CloseHandle(handle)
        self.k.CloseHandle(self.handle)  # Final defensive KILL_ON_CLOSE, not a completion proof.


def unix_drain(process, deadline):
    require(process.pid > 1 and process.pid != os.getpgrp(), "Refusing nonprivate build group")
    # Never reap/reuse the leader before all group members are accounted for.
    os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    unresolved = []
    while time.monotonic() < deadline:
        observed = subprocess.run(["/bin/ps", "-axo", "pid=,pgid=,stat="],
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  timeout=max(0.001, deadline - time.monotonic()), check=True)
        require(len(observed.stdout) <= 16 << 20, "Build group census exceeds bound")
        rows = [line.split() for line in observed.stdout.decode().splitlines()]
        unresolved = [(int(pid), state) for pid, group, state in rows if int(group) == process.pid and int(pid) != process.pid]
        for pid, state in unresolved:
            if state.startswith("Z"):
                try:
                    os.waitpid(pid, os.WNOHANG)  # Only actual adopted children; never signal them.
                except ChildProcessError:
                    pass
        if not unresolved:
            process.wait(timeout=max(0.001, deadline - time.monotonic()))
            return {"complete": True, "remaining_group_members": []}
        time.sleep(0.02)
    return {"complete": False, "remaining_group_members": unresolved}


class OwnedCommands:
    def __init__(self, root, environment, logs, deadline, cleanup_seconds):
        self.root, self.environment, self.logs = root, environment, logs
        self.deadline, self.cleanup_seconds, self.records = deadline, cleanup_seconds, []
        if os.name != "nt":
            require(hasattr(os, "waitid") and signal.getsignal(signal.SIGCHLD) == signal.SIG_DFL,
                    "Actual unreaped native child ownership is unavailable")
            if sys.platform.startswith("linux"):
                libc = ctypes.CDLL(None, use_errno=True)
                require(libc.prctl(36, 1, 0, 0, 0) == 0, "Owned build subreaper unavailable")

    def remaining(self):
        require(time.monotonic() < self.deadline, "Total native build deadline exhausted")
        return self.deadline - time.monotonic()

    def run(self, command, log=None):
        budget = self.remaining()
        sequence = len(self.records) + 1
        output_path = log or self.logs / f"command-{sequence:03}.stdout"
        error_path = self.logs / f"command-{sequence:03}.stderr"
        encoded = base64.b64encode(json.dumps(command).encode()).decode()
        require(len(encoded) <= 16384, "Owned command bootstrap exceeds bound")
        record = {"command": command, "budget_seconds": budget, "cleanup": {"complete": False}}
        self.records.append(record)
        job = WindowsBuildJob() if os.name == "nt" else None
        process = None
        assigned = False
        try:
            with output_path.open("x", encoding="utf-8") as output, error_path.open("x", encoding="utf-8") as error:
                process = subprocess.Popen([sys.executable, "-I", "-S", "-B", str(Path(__file__).resolve()),
                                            "--_owned-command", encoded, str(self.deadline)],
                                           cwd=self.root, env=self.environment, stdin=subprocess.PIPE,
                                           stdout=output, stderr=error, close_fds=True,
                                           start_new_session=os.name != "nt",
                                           creationflags=0x08000000 if os.name == "nt" else 0)
                if job:
                    job.assign(process)
                    assigned = True
                process.stdin.write(b"\x01")
                process.stdin.flush()  # One byte in a fresh empty private pipe.
                if os.name == "nt":
                    process.wait(timeout=self.remaining())
                else:
                    while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
                        self.remaining()
                        time.sleep(0.02)
        except BaseException as error:
            record["error"] = f"{type(error).__name__}: {error}"
        finally:
            if process is not None:
                cleanup_deadline = time.monotonic() + self.cleanup_seconds
                try:
                    if job and not assigned:
                        # This exact child never received a permit and cannot
                        # have started Cargo. Do not leave it outside the job.
                        process.kill()
                        process.wait(timeout=max(0.001, cleanup_deadline - time.monotonic()))
                    record["cleanup"] = job.drain(cleanup_deadline) if job else unix_drain(process, cleanup_deadline)
                    if record["cleanup"]["complete"]:
                        process.wait(timeout=max(0.001, cleanup_deadline - time.monotonic()))
                        record["returncode"] = process.returncode
                except BaseException as error:
                    record["cleanup"] = {"complete": False, "error": f"{type(error).__name__}: {error}"}
                if process.stdin:
                    try:
                        process.stdin.close()
                    except BrokenPipeError:
                        pass
            if job:
                job.close()
            (self.logs / f"command-{sequence:03}.json").write_text(json.dumps(record, indent=2) + "\n")
        require(record["cleanup"]["complete"] and record.get("returncode") == 0 and "error" not in record,
                f"Native command failed/unresolved; preserve outputs and inspect {self.logs / f'command-{sequence:03}.json'}")
        self.remaining()
        return output_path

    def check_output(self, command, **keywords):
        require(set(keywords) <= {"cwd", "text", "encoding"}, "Unexpected canonical subprocess option")
        require("cwd" not in keywords or Path(keywords["cwd"]).resolve() == self.root, "Canonical command changed source root")
        path = self.run(command)
        require(path.stat().st_size <= 16 << 20, "Captured native command output exceeds bound")
        result = path.read_bytes()
        return result.decode(keywords.get("encoding", "utf-8")) if keywords.get("text") or keywords.get("encoding") else result


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def regular(path):
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and not path.is_symlink()
            and not getattr(metadata, "st_file_attributes", 0) & 0x400,
            f"Refusing redirected/nonregular input: {path}")
    return path


def absolute(path, exists=True):
    require(path.is_absolute(), f"Explicit absolute path required: {path}")
    for candidate in (path, *path.parents):
        if candidate.exists() or candidate.is_symlink():
            metadata = candidate.lstat()
            require(not candidate.is_symlink() and not getattr(metadata, "st_file_attributes", 0) & 0x400,
                    f"Use a canonical, unredirected absolute path: {candidate}")
    resolved = path.resolve(strict=exists)
    for parent in (resolved, *resolved.parents):
        if parent.exists():
            metadata = parent.lstat()
            require(not parent.is_symlink() and not getattr(metadata, "st_file_attributes", 0) & 0x400,
                    f"Redirected path is unsupported: {parent}")
    return resolved


def contained(path, owner):
    require(path != owner and path.is_relative_to(owner), f"Output escapes explicit artifact owner: {path}")


def load_module(root, name):
    source = regular(root / "scripts" / (name + ".py"))
    specification = importlib.util.spec_from_file_location(name.replace("-", "_"), source)
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def manifest_files(root, manifest, expected_hash):
    regular(manifest)
    require(manifest.stat().st_size <= 16 << 20, "Frozen source manifest exceeds bound")
    require(sha(manifest) == expected_hash, "Frozen source manifest digest differs")
    document = json.loads(manifest.read_bytes())
    files = document.get("files")
    require(isinstance(files, dict) and 0 < len(files) <= 100_000, "Expected bounded explicit files-to-SHA256 manifest")
    checked = {}
    physical = set()
    for name, digest in files.items():
        require(isinstance(name, str) and len(name.encode()) <= 4096, "Invalid frozen source path")
        relative = PurePosixPath(name)
        require(bool(relative.parts) and not relative.is_absolute() and relative.as_posix() == name
                and all(part not in ("", ".", "..") for part in relative.parts)
                and not any(part in (".git", "node_modules") for part in relative.parts)
                and relative.parts[0] not in EXCLUDED_TOP and "\\" not in name
                and ":" not in name, "Unsafe/private frozen source path")
        require(isinstance(digest, str) and re.fullmatch(r"[0-9a-f]{64}", digest), "Invalid source SHA256")
        path = root.joinpath(*relative.parts)
        for part in (path, *path.parents):
            if part == root:
                break
            regular(part) if part == path else require(not part.is_symlink()
                and not getattr(part.lstat(), "st_file_attributes", 0) & 0x400, "Source ancestor is redirected")
        resolved = path.resolve(strict=True)
        require(resolved.is_relative_to(root) and resolved not in physical, "Aliased/escaping source input")
        physical.add(resolved)
        checked[name] = digest
    require(REQUIRED <= checked.keys(), "Frozen manifest omits required qualification/build source")
    # Refuse omitted native inputs. This enumerates only the selected source
    # roots, never a parent checkout or private/index state directory.
    discovered = set(FIXED_INPUTS)
    for selected in (".cargo", "crates", "third_party"):
        base = root / selected
        require(base.is_dir() and not base.is_symlink(), f"Native source root missing: {selected}")
        for directory, directories, names in os.walk(base, followlinks=False):
            require(not any((Path(directory) / name).is_symlink()
                or getattr((Path(directory) / name).lstat(), "st_file_attributes", 0) & 0x400
                for name in directories), "Native source directory is redirected")
            for name in names:
                path = Path(directory) / name
                regular(path)
                discovered.add(path.relative_to(root).as_posix())
    require(discovered <= checked.keys(), "Frozen manifest omits native files present in the selected source tree")
    return document, checked


def verify_source(root, files):
    aggregate = hashlib.sha256()
    for name, expected in sorted(files.items()):
        path = regular(root / name)
        require(sha(path) == expected, f"Frozen source bytes changed: {name}")
        encoded = name.encode()
        aggregate.update(len(encoded).to_bytes(8, "big"))
        aggregate.update(encoded)
        aggregate.update(bytes.fromhex(expected))
    return {"sha256": aggregate.hexdigest(), "files": len(files)}


def verify_models(root, directory):
    policy = load_module(root, "int8-policy")
    provisioner = load_module(root, "provision-local-model")
    catalog = policy.catalog()
    result = {}
    for profile in catalog["profiles"]:
        selected = absolute(directory / profile["id"])
        require(selected.is_relative_to(directory), "Model profile escapes selected local directory")
        provisioner.validate_asset_bindings(profile, {a["name"]: (a["url"], a["bytes"], a["sha256"])
                                                     for a in profile["assets"]})
        manifest = regular(selected / "model.json")
        require(manifest.stat().st_size <= 16384 and json.loads(manifest.read_bytes()) == profile["manifest"],
                "Existing offline model manifest differs from canonical catalog")
        values = {"model.json": sha(manifest)}
        for asset in profile["assets"]:
            path = regular(selected / asset["name"])
            provisioner.verify(path, asset["bytes"], asset["sha256"])
            values[asset["name"]] = sha(path)
        result[profile["id"]] = values
    return result


def verify_runtime(root, directory, operating_system, architecture):
    # Read-only verification, never provision() or archive extraction/download.
    provisioner = load_module(root, "provision-onnx-runtime")
    rows = provisioner.entries(operating_system + "-" + architecture)
    for row in rows:
        path = regular(directory / row["name"])
        provisioner.verify(path, row["bytes"], row["sha256"])
    return rows


def native_image(path, operating_system, architecture):
    # Same bounded PE/ELF/Mach-O conditions as runtime_pair::validate_native_image.
    with regular(path).open("rb") as source:
        header = source.read(65536)
    if operating_system == "windows":
        require(len(header) >= 64 and header[:2] == b"MZ", "Invalid native PE header")
        offset = struct.unpack_from("<I", header, 60)[0]
        require(offset + 26 <= len(header), "PE header exceeds canonical inspection bound")
        require(header[offset:offset + 4] == b"PE\0\0"
                and struct.unpack_from("<H", header, offset + 4)[0] == (0x8664 if architecture == "x86_64" else 0xAA64)
                and struct.unpack_from("<H", header, offset + 24)[0] == 0x20B, "Wrong native PE architecture")
    elif operating_system == "linux":
        require(len(header) >= 20 and header[:6] == b"\x7fELF\x02\x01"
                and struct.unpack_from("<H", header, 18)[0] == (62 if architecture == "x86_64" else 183), "Wrong native ELF architecture")
    else:
        require(len(header) >= 16 and header[:4] == b"\xcf\xfa\xed\xfe"
                and struct.unpack_from("<I", header, 4)[0] == (0x0100000C if architecture == "aarch64" else 0x01000007)
                and struct.unpack_from("<I", header, 12)[0] == 2, "Wrong native Mach-O architecture")


def cargo_artifact(commands, command, log, target_dir, name, kind):
    commands.run(command, log)
    matches = []
    with log.open(encoding="utf-8") as source:
        for line in source:
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("reason") == "compiler-artifact" and event.get("target", {}).get("name") == name and event.get("executable"):
                require(kind in event["target"]["kind"], "Cargo target kind differs")
                matches.append(event)
    require(len(matches) == 1, f"Expected one actual executable artifact for {name}")
    event = matches[0]
    require(event.get("fresh") is False, f"Executable {name} was reused; a fresh build from this capture is required")
    executable = absolute(Path(event["executable"]))
    require(executable.is_relative_to(target_dir), "Cargo executable escaped selected target directory")
    regular(executable)
    expected = {"embedding-worker", "local-inference", "bundled-model"} if kind == "test" else set()
    require(set(event.get("features", [])) == expected, "Actual Cargo feature boundary differs")
    return executable, {"target": event["target"], "profile": event["profile"], "features": event.get("features", []), "fresh": event["fresh"],
                        "executable": str(executable), "executable_sha256": sha(executable)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("source-root", "source-manifest", "artifact-root", "target-dir", "output-root", "model-dir", "runtime-dir"):
        parser.add_argument("--" + flag, type=Path, required=True)
    parser.add_argument("--source-manifest-sha256", required=True)
    parser.add_argument("--profile", choices=("debug", "release"), default="debug")
    parser.add_argument("--build-timeout-seconds", type=float, required=True,
                        help="Finite total deadline shared by all native build/identity commands")
    parser.add_argument("--cleanup-timeout-seconds", type=float, default=30,
                        help="Separate bounded cleanup allowance after each owned command")
    args = parser.parse_args()
    require(math.isfinite(args.build_timeout_seconds) and args.build_timeout_seconds > 0,
            "A positive finite total build timeout is required")
    require(math.isfinite(args.cleanup_timeout_seconds) and 0 < args.cleanup_timeout_seconds <= 120,
            "Cleanup timeout must be positive and at most 120 seconds")
    deadline = time.monotonic() + args.build_timeout_seconds
    require(math.isfinite(deadline), "Total build deadline overflow")
    require(re.fullmatch(r"[0-9a-f]{64}", args.source_manifest_sha256), "Explicit frozen manifest hash required")
    root = absolute(args.source_root)
    manifest_path = absolute(args.source_manifest)
    artifacts = absolute(args.artifact_root)
    target_dir = absolute(args.target_dir, exists=False)
    output = absolute(args.output_root, exists=False)
    models = absolute(args.model_dir)
    runtime = absolute(args.runtime_dir)
    require(root.is_dir() and artifacts.is_dir() and models.is_dir() and runtime.is_dir(),
            "Source/artifact/model/runtime roots must already exist")
    for path in (target_dir, output):
        contained(path, artifacts)
        require(all(not path.is_relative_to(directory) and not directory.is_relative_to(path)
                    for directory in (models, runtime)), "Build outputs and verified asset inputs must not overlap")
        if path.is_relative_to(root):
            require(any(path.is_relative_to(root / excluded) for excluded in ("artifacts", "target")),
                    "Outputs inside checkout must stay in excluded artifact/target roots")
    require(target_dir != output and not target_dir.is_relative_to(output) and not output.is_relative_to(target_dir), "Target and fresh output overlap")
    require(not output.exists(), "Select a NEW qualification output root; existing artifacts are never modified")
    require(not target_dir.is_relative_to(root / "crates") and not output.is_relative_to(root / "crates"), "Build outputs may not enter source inputs")
    source_document, files = manifest_files(root, manifest_path, args.source_manifest_sha256)
    before = verify_source(root, files)
    runner = sha(Path(__file__).resolve())
    model_inputs = verify_models(root, models)
    operating_system = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}.get(platform.system())
    architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
    require(operating_system is not None and architecture is not None, "Unsupported native host")
    runtime_inputs = verify_runtime(root, runtime, operating_system, architecture)
    environment = os.environ.copy()
    cleared = []
    for name in list(environment):
        if name in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP", "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTFLAGS", "CARGO_TARGET_DIR"} or re.fullmatch(r"CARGO_TARGET_.*_RUSTFLAGS", name):
            cleared.append(name)
            environment.pop(name)
    environment["AIRK_BUNDLED_MODEL_DIR"] = str(models)
    environment["CARGO_TARGET_DIR"] = str(target_dir)
    environment["CARGO_NET_OFFLINE"] = "true"
    if operating_system == "macos":
        require(environment.get("MACOSX_DEPLOYMENT_TARGET", "13.3") == "13.3", "Native Mac baseline must remain 13.3")
        environment["MACOSX_DEPLOYMENT_TARGET"] = "13.3"
    builder = load_module(root, "build-service-runtime")
    output.mkdir(mode=0o700)
    payload = output / "payload"
    payload.mkdir(mode=0o700)
    logs = output / "logs"
    logs.mkdir(mode=0o700)
    target_dir.mkdir(parents=True, exist_ok=True)
    commands = OwnedCommands(root, environment, logs, deadline, args.cleanup_timeout_seconds)
    # Instance-local proxy bounds even canonical write_pair_metadata's internal
    # version subprocesses; the shared subprocess module is never monkeypatched.
    builder.subprocess = commands
    rustc = commands.check_output(["rustc", "--version", "--verbose"], text=True)
    host = next(line.removeprefix("host: ") for line in rustc.splitlines() if line.startswith("host: "))
    require(host.startswith(architecture + "-") and {"windows": "windows", "linux": "linux", "macos": "apple-darwin"}[operating_system] in host,
            "Rust host/Python native architecture differ; translation/cross compilation is not qualification")
    builder.check_lightweight_dependencies()
    common = ["--offline", "--locked", "-p", "airk-cli", "--no-default-features",
              "--target-dir", str(target_dir), "--message-format=json-render-diagnostics"]
    profile = ["--release"] if args.profile == "release" else []
    helper_command = ["cargo", "test", *common, "--test", "guarded_provider_fixture", "--features", "embedding-worker", "--no-run", *profile]
    helper, helper_build = cargo_artifact(commands, helper_command, logs / "helper.jsonl", target_dir, "guarded_provider_fixture", "test")
    native_image(helper, operating_system, architecture)
    helper_name = "airk-embed.exe" if operating_system == "windows" else "airk-embed"
    shutil.copy2(helper, payload / helper_name)
    require(sha(helper) == sha(payload / helper_name), "Copied helper bytes differ")
    # Copy helper BEFORE the separate main build can change Cargo's features.
    main_command = ["cargo", "build", *common, "--bin", "airk", *profile]
    main_image, main_build = cargo_artifact(commands, main_command, logs / "main.jsonl", target_dir, "airk", "bin")
    native_image(main_image, operating_system, architecture)
    main_name = "airk.exe" if operating_system == "windows" else "airk"
    shutil.copy2(main_image, payload / main_name)
    require(sha(main_image) == sha(payload / main_name), "Copied main bytes differ")
    version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if operating_system == "macos":
        builder.arrange_macos_bundle(payload, version)
    for row in runtime_inputs:
        destination = payload / builder.inference_asset_path(operating_system, row["name"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        require(not destination.exists(), "Runtime staging would overwrite a payload member")
        shutil.copyfile(runtime / row["name"], destination)
        require(sha(destination) == row["sha256"], "Copied runtime asset differs")
    pair = builder.write_pair_metadata(payload, version, operating_system, architecture)
    main_relative, helper_relative = builder.runtime_names(operating_system)
    for path in (payload / main_relative, payload / helper_relative):
        native_image(path, operating_system, architecture)
    require(pair == {"schema": 1, "version": version, "os": operating_system, "arch": architecture,
                     "airk_sha256": sha(payload / main_relative), "embed_sha256": sha(payload / helper_relative)}, "Canonical RuntimePair differs from staged actual bytes")
    _, after_files = manifest_files(root, manifest_path, args.source_manifest_sha256)
    require(files == after_files and before == verify_source(root, files) and sha(manifest_path) == args.source_manifest_sha256,
            "Source/manifest changed during paired build; no qualification receipt issued")
    require(model_inputs == verify_models(root, models) and runner == sha(Path(__file__).resolve()), "Model inputs/runner changed during paired build")
    require(runtime_inputs == verify_runtime(root, runtime, operating_system, architecture),
            "Verified runtime inputs changed during paired build")
    commands.remaining()
    receipt = {
        "schema": "airk.p6f-instrumented-native-owner-pair/1", "kind": "instrumented-native-owner",
        "cfg_test": True, "target": "guarded_provider_fixture", "harness": False, "default_test": False,
        "release_candidate": False, "signed_qualified": False, "installed_qualified": False,
        "model_work_qualified": False, "qualification_executed": False, "selector_catalog_validated": False,
        "selectors_expected": SELECTORS, "source": before, "source_manifest_sha256": args.source_manifest_sha256,
        "entry_commit_in_supplied_manifest_only": source_document.get("entry_commit"), "runner_sha256": runner,
        "profile": args.profile, "native_target": host, "rustc": rustc.strip(), "cleared_child_build_environment_keys": sorted(cleared),
        "commands": commands.records,
        "execution_control": {"build_timeout_seconds": args.build_timeout_seconds,
                              "cleanup_timeout_seconds": args.cleanup_timeout_seconds,
                              "containment_scope": "anonymous Windows job" if os.name == "nt" else "private Unix build process group",
                              "abrupt_parent_death_cleanup_qualified": False,
                              "escaped_descendant_cleanup_qualified": False,
                              "competing_builds_allowed_for_qualification": False},
        "cargo_artifacts": {"helper": helper_build, "main": main_build}, "runtime_pair": pair,
        "runtime_manifest_sha256": sha(payload / "airk-runtime.json"), "offline_bundled_build_model_inputs": model_inputs,
        "offline_runtime_inputs": runtime_inputs,
        "exercised_model": "none: this script only stages; ignored drivers create/verify original 2D vocabulary inputs",
        "payload_files": {path.relative_to(payload).as_posix(): sha(path) for path in sorted(payload.rglob("*")) if path.is_file()},
        "logs": {path.name: sha(path) for path in sorted(logs.iterdir())},
        "scope": "Unsealed cfg(test) helper plus actual minimal main; no download, signing, installation, release sealing or native qualification execution.",
    }
    require(runner == sha(Path(__file__).resolve()), "Runner changed before receipt publication")
    with (output / "qualification.json").open("x", encoding="utf-8") as target:
        json.dump(receipt, target, indent=2)
        target.write("\n")
    print(json.dumps({"payload": str(payload), "receipt": str(output / "qualification.json"),
                      "receipt_sha256": sha(output / "qualification.json"), "qualification_executed": False}))


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--_owned-command":
        sys.exit(command_worker(sys.argv[2], float(sys.argv[3])))
    main()
