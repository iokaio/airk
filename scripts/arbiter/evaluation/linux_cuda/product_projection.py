# SPDX-License-Identifier: Apache-2.0
"""Own two native Linux metadata selectors and verify their product-closure evidence."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import runpy
import select
import signal
import stat
import subprocess
import sys
import time

BUILD = Path("/home/tyler/airk-builds/arbiter-final-linux-r1")
UID = 1000
HOST = 8 << 30
LOG = 64 << 20
NS = 1_000_000_000
PREFIX = "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::"
REQUEST = PREFIX + "emit_cuda_component_probe_request"
PROJECT = PREFIX + "emit_cuda_runtime_package_projection_from_held_native_closure"
MANIFEST = "airk-cuda-runtime.json"
GUARD = Path(__file__).resolve().with_name("fp16_process.py")


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def identity():
    require(platform.system() == "Linux" and platform.machine() == "x86_64"
            and os.getresuid() == (UID, UID, UID) and os.getresgid() == (UID, UID, UID),
            "verified Ubuntu human identity required")


def checked_path(path, directory=False):
    path = Path(path)
    require(path.is_absolute() and path == path.resolve(strict=True) and path.is_relative_to(BUILD),
            "canonical private Linux build path required")
    for item in [path, *path.parents]:
        if item == Path("/home"):
            break
        meta = item.lstat()
        is_dir = item != path or directory
        require(meta.st_uid == UID and not meta.st_mode & 0o022
                and (stat.S_ISDIR(meta.st_mode) if is_dir else stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1),
                "incorrect private source owner, mode, or link")
    return path


def stamp(meta):
    return [meta.st_dev, meta.st_ino, meta.st_size, meta.st_ctime_ns, meta.st_mtime_ns]


def held(stack, path):
    path = checked_path(path)
    file = stack.enter_context(os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC), "rb"))
    initial = stamp(os.fstat(file.fileno()))
    require(initial == stamp(path.stat()), "held native path differs")
    return file, initial


def byte_pin(path, file, initial, deadline=None):
    file.seek(0)
    hasher = hashlib.sha256()
    while data := file.read(1 << 20):
        hasher.update(data)
        if deadline is not None:
            require(time.monotonic_ns() < deadline, "original projection deadline expired")
    require(initial == stamp(os.fstat(file.fileno())) == stamp(path.lstat()), "held native bytes changed")
    file.seek(0)
    return {"path": str(path), "bytes": initial[2], "sha256": hasher.hexdigest()}


def pin(path, deadline=None):
    path = checked_path(path)
    with ExitStack() as stack:
        file, initial = held(stack, path)
        return byte_pin(path, file, initial, deadline)


def document(path, expected, limit=1 << 20):
    path = checked_path(path)
    with ExitStack() as stack:
        file, initial = held(stack, path)
        require(0 < initial[2] <= limit, "metadata outside bound")
        data = file.read()
        require(hashlib.sha256(data).hexdigest() == expected and stamp(path.stat()) == initial
                and stamp(os.fstat(file.fileno())) == initial, "metadata identity differs")
        return json.loads(data)


def verify_pin(row, limit=None):
    require(isinstance(row, dict) and set(row) == {"path", "bytes", "sha256"}
            and type(row["bytes"]) is int and (limit is None or row["bytes"] <= limit),
            "expected original bounded byte pin")
    require(pin(row["path"]) == row, "original retained bytes differ")
    return Path(row["path"])


def image_proof(path, expected, image):
    proof = document(path, expected)
    require(proof["success"] is True and proof["target"] == "x86_64-unknown-linux-gnu"
            and proof["command"] and all(re.fullmatch("[0-9a-f]{64}", proof[k])
                                        for k in ("static_core_build", "aggregate_static_sha256", "manifest_sha256")),
            "checked native Linux image build proof required")
    verify_pin(proof["image"])
    require((proof["image"]["bytes"], proof["image"]["sha256"]) == (image["bytes"], image["sha256"]),
            "staged image differs from actual native build")
    return proof


def write(path, row):
    require(checked_path(path.parent, True) == path.parent, "private output parent changed")
    with path.open("x", encoding="utf-8") as stream:
        json.dump(row, stream, sort_keys=True, separators=(",", ":"))
        stream.write("\n")
    checked_path(path)


def resource_pin(row):
    return {"path": row["path"], "sha256": row["sha256"]}


def projection_rows(projected, root, descriptor, image, static_core, deadline=None):
    require(projected["schema"] == "airk.native.cuda-package-projection/1" and projected["activation"] is False
            and projected["qualification"] is False and projected["static_core_build"] == static_core,
            "original native product metadata/static core differs")
    expected_descriptor = {"name": MANIFEST, "bytes": descriptor["bytes"], "sha256": descriptor["sha256"]}
    require(projected["descriptor"] == expected_descriptor, "native projection descriptor differs")
    files = {}
    for row in projected["files"]:
        name = row["name"]
        require(isinstance(name, str) and name and name not in (".", "..", Path(image["path"]).name)
                and "/" not in name and "\\" not in name and name not in files, "invalid product filename")
        actual = pin(root / name, deadline)
        require(row == {"name": name, "bytes": actual["bytes"], "sha256": actual["sha256"]},
                "held package file differs from genuine native projection")
        files[name] = actual["sha256"]
    require(files.get(MANIFEST) == descriptor["sha256"]
            and set(files) | {Path(image["path"]).name} == {p.name for p in root.iterdir()},
            "runtime root is incomplete or contains unobserved files")
    return files


def child():
    parent, imagefd, readfd, readyfd = map(int, sys.argv[2:6])
    guard = runpy.run_path(str(GUARD))
    resource.setrlimit(resource.RLIMIT_FSIZE, (LOG, LOG))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    guard["bind_process"](parent, HOST)
    os.write(readyfd, b"1")
    os.close(readyfd)
    require(os.read(readfd, 1) == b"1", "parent released no native execution grant")
    os.close(readfd)
    os.set_inheritable(imagefd, False)
    os.execve(imagefd, sys.argv[6:], dict(os.environ))


def run_selector(output, image, file, initial, env, selector, label, deadline):
    log = output / (label + ".log")
    argv = [str(image), "--exact", selector, "--ignored", "--nocapture", "--test-threads=1"]
    started = time.monotonic_ns()
    with ExitStack() as resources:
        gate, release = os.pipe2(os.O_CLOEXEC)
        announce, ready = os.pipe2(os.O_CLOEXEC)
        for fd in (gate, release, announce, ready):
            resources.callback(os.close, fd)
        sink = resources.enter_context(log.open("xb"))
        cmd = [sys.executable, "-I", "-B", "-S", str(Path(__file__).resolve()), "--child", str(os.getpid()),
               str(file.fileno()), str(gate), str(ready), *argv]
        proc = subprocess.Popen(cmd, env=env, cwd=output, stdin=subprocess.DEVNULL, stdout=sink,
                                stderr=subprocess.STDOUT, start_new_session=True, pass_fds=(file.fileno(), gate, ready))
        pidfd = os.pidfd_open(proc.pid)
        resources.callback(os.close, pidfd)
        try:
            remaining = lambda: max(0, (deadline - time.monotonic_ns()) / NS)
            readable, _, _ = select.select([announce, pidfd], [], [], remaining())
            require(announce in readable and os.read(announce, 1) == b"1", "native custody initialization failed")
            status = dict(line.split(":", 1) for line in Path(f"/proc/{proc.pid}/status").read_text().splitlines())
            uid = list(map(int, status["Uid"].split()))
            require(uid == [UID] * 4 and int(status["PPid"]) == os.getpid() and int(status["NoNewPrivs"]) == 1
                    and int(status["Seccomp"]) == 2 and os.getpgid(proc.pid) == proc.pid
                    and resource.prlimit(proc.pid, resource.RLIMIT_AS) == (HOST, HOST)
                    and initial == stamp(os.fstat(file.fileno())) == stamp(image.stat()), "pre-exec custody differs")
            os.write(release, b"1")
            readable, _, _ = select.select([pidfd], [], [], remaining())
            require(pidfd in readable, "native selector exceeded original deadline")
            event = os.waitid(os.P_PIDFD, pidfd, os.WEXITED | os.WNOWAIT)
            require(event.si_pid == proc.pid and event.si_uid == UID and event.si_code == os.CLD_EXITED
                    and event.si_status == 0 and proc.wait() == 0, "native selector failed")
            require(not Path(f"/proc/{proc.pid}").exists(), "original native leader not reaped")
        finally:
            if proc.poll() is None:
                signal.pidfd_send_signal(pidfd, signal.SIGKILL)
                proc.wait()
    proof = pin(log, deadline)
    require(proof["bytes"] <= LOG, "native log cap exceeded")
    if proof["bytes"] > 1 << 20:
        raise ValueError("unexpectedly large model-free selector log")
    log_text = log.read_text()
    require(f"test {selector} ... ok" in log_text and "test result: ok. 1 passed; 0 failed" in log_text,
            "exact native selector did not pass")
    return {"pid": proc.pid, "uid": uid, "parent_pid": os.getpid(), "pgid": proc.pid,
            "argv": argv, "exit_code": 0, "reaped_by_pidfd": True, "pre_exec_seccomp": 2,
            "no_new_privs": 1, "address_space_limit_bytes": HOST, "image_stat": initial,
            "start_monotonic_ns": started, "finish_monotonic_ns": time.monotonic_ns(), "log": proof}


def execute(image_path, proof_path, proof_sha, descriptor_sha, output):
    identity()
    os.umask(0o077)
    image = checked_path(image_path)
    root = checked_path(image.parent, True)
    output = checked_path(output, True)
    require(not any(output.iterdir()) and not output.is_relative_to(root), "fresh separate private output required")
    descriptor = pin(root / MANIFEST)
    require(descriptor["sha256"] == descriptor_sha, "expected original Linux product descriptor")
    approved = image_proof(proof_path, proof_sha, pin(image))
    scratch = output / "tmp"
    scratch.mkdir(mode=0o700)
    checked_path(scratch, True)
    request_path = output / "request.json"
    proposed = output / "projection.json"
    env = {"PATH": "/usr/bin:/bin", "LANG": "C", "HOME": "/home/tyler", "TMPDIR": str(scratch)}
    started = time.monotonic_ns()
    deadline = started + 120 * NS
    with ExitStack() as stack:
        file, initial = held(stack, image)
        image_row = byte_pin(image, file, initial, deadline)
        require(image_row["sha256"] == approved["image"]["sha256"], "staged native image changed")
        creator = run_selector(output, image, file, initial, env | {
            "AIRK_CUDA_REQUEST_KIND": "package_projection", "AIRK_CUDA_RUNTIME_ROOT": str(root),
            "AIRK_CUDA_RUNTIME_MANIFEST": descriptor["path"], "AIRK_CUDA_RUNTIME_MANIFEST_SHA256": descriptor_sha,
            "AIRK_CUDA_DEADLINE_MS": "120000", "AIRK_CUDA_REQUEST_OUTPUT": str(request_path)},
            REQUEST, "request", deadline)
        request = pin(request_path, deadline)
        emitter = run_selector(output, image, file, initial, env | {
            "AIRK_CUDA_QUALIFICATION_INPUT": request["path"], "AIRK_CUDA_QUALIFICATION_INPUT_SHA256": request["sha256"],
            "AIRK_CUDA_QUALIFICATION_OUTPUT": str(proposed)}, PROJECT, "projection", deadline)
        require(byte_pin(image, file, initial, deadline) == image_row, "native image changed after execution")
    projected = pin(proposed, deadline)
    projection_rows(document(proposed, projected["sha256"]), root, descriptor, image_row,
                    approved["static_core_build"], deadline)
    result = {"schema": "airk.private.linux-cuda-native-product-projection-execution/1",
              "image": image_row, "image_build_proof": pin(proof_path), "descriptor": descriptor,
              "projection": projected, "emission": pin(proposed.with_suffix(".receipt.json"), deadline),
              "request": request, "request_process": creator, "projection_process": emitter,
              "start_monotonic_ns": started, "finish_monotonic_ns": time.monotonic_ns(), "deadline_ns": deadline,
              "uid": UID, "parent_pid": os.getpid(), "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "process_guard_sha256": hashlib.sha256(GUARD.read_bytes()).hexdigest()}
    require(result["image_build_proof"]["sha256"] == proof_sha
            and result["finish_monotonic_ns"] < deadline, "source proof changed or native deadline expired")
    receipt = output / "execution.json"
    write(receipt, result)
    return verify_execution(receipt, pin(receipt)["sha256"], proposed, result["projection"]["sha256"])


def verify_execution(receipt_path, receipt_sha, projection_path, projection_sha):
    identity()
    receipt = pin(receipt_path)
    require(receipt["sha256"] == receipt_sha, "native process evidence differs")
    value = document(receipt_path, receipt_sha)
    require(value["schema"] == "airk.private.linux-cuda-native-product-projection-execution/1"
            and value["uid"] == UID and value["parent_pid"] > 0
            and value["script_sha256"] == hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
            and value["process_guard_sha256"] == hashlib.sha256(GUARD.read_bytes()).hexdigest(),
            "unrecognized Linux projection producer")
    image = verify_pin(value["image"])
    proof = value["image_build_proof"]
    verify_pin(proof)
    build = image_proof(proof["path"], proof["sha256"], value["image"])
    root = checked_path(image.parent, True)
    descriptor = verify_pin(value["descriptor"], 128 << 10)
    projected = verify_pin(value["projection"], 1 << 20)
    emission = verify_pin(value["emission"], 1 << 20)
    request = verify_pin(value["request"], 1 << 20)
    require(descriptor == root / MANIFEST and projected == Path(projection_path) and
            value["projection"]["sha256"] == projection_sha and emission == projected.with_suffix(".receipt.json")
            and Path(receipt_path).parent == request.parent == projected.parent
            and not projected.parent.is_relative_to(root), "original projection paths differ")
    lo = value["start_monotonic_ns"]
    hi = value["finish_monotonic_ns"]
    require(0 < lo < hi < value["deadline_ns"] == lo + 120 * NS, "native deadline evidence differs")
    last = lo
    for key, selector in (("request_process", REQUEST), ("projection_process", PROJECT)):
        proc = value[key]
        expected_argv = [str(image), "--exact", selector, "--ignored", "--nocapture", "--test-threads=1"]
        require(proc["argv"] == expected_argv and proc["pid"] == proc["pgid"] > 0
                and proc["uid"] == [UID] * 4 and proc["parent_pid"] == value["parent_pid"]
                and proc["exit_code"] == 0 and proc["reaped_by_pidfd"] is True and proc["no_new_privs"] == 1
                and proc["pre_exec_seccomp"] == 2 and proc["address_space_limit_bytes"] == HOST
                and proc["image_stat"] == stamp(image.stat()) and last <= proc["start_monotonic_ns"]
                < proc["finish_monotonic_ns"] <= hi, "native child custody evidence differs")
        log = verify_pin(proc["log"], 1 << 20)
        expected_log = request.parent / ("request.log" if selector == REQUEST else "projection.log")
        require(log == expected_log and f"test {selector} ... ok" in log.read_text(), "native selector log differs")
        last = proc["finish_monotonic_ns"]
    req = document(request, value["request"]["sha256"])
    require(req["kind"] == "package_projection" and req["directory"] == str(root)
            and req["descriptor"] == resource_pin(value["descriptor"]) and req["deadline_ms"] == 120000,
            "original Rust-emitted request differs")
    witness = document(emission, value["emission"]["sha256"])
    require(witness["schema"] == "airk.private.cuda-runtime-package-projection-emission/1"
            and witness["input"] == resource_pin(value["request"]) and witness["descriptor"] == req["descriptor"]
            and witness["projection"] == resource_pin(value["projection"])
            and witness["pid"] == value["projection_process"]["pid"] and witness["execution_user"] == str(UID)
            and 0 < witness["elapsed_us"] < 120_000_000 and witness["forward_count_observed"] is None
            and all(witness[k] is False for k in ("model_requests_dispatched", "runtime_admission_dispatched",
                                                   "activation", "qualification")), "native projection witness differs")
    body = document(projected, projection_sha)
    files = projection_rows(body, root, value["descriptor"], value["image"], build["static_core_build"])
    require(pin(receipt_path) == receipt and pin(projected) == value["projection"], "projection evidence changed")
    return {"runtime_root": root, "descriptor": body["descriptor"], "projection": value["projection"],
            "files": files, "execution": receipt}


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        child()
    else:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("--image", type=Path, required=True)
        parser.add_argument("--image-proof", type=Path, required=True)
        parser.add_argument("--image-proof-sha", required=True)
        parser.add_argument("--descriptor-sha", required=True)
        parser.add_argument("--output", type=Path, required=True)
        opts = parser.parse_args()
        result = execute(opts.image, opts.image_proof, opts.image_proof_sha, opts.descriptor_sha, opts.output)
        print(json.dumps({k: str(v) if isinstance(v, Path) else v for k, v in result.items() if k != "files"}))
