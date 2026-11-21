# SPDX-License-Identifier: Apache-2.0
"""Verify a stopped AWS publication with the native Kernel observer and two full store inventories."""
import argparse
import ctypes
from ctypes import wintypes
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import queue
import runpy
import stat
import subprocess
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
normal = runpy.run_path(str(HERE / "aws_normal_service.py"))
n = normal["n"]
require = n["require"]
E = n["CAMPAIGN"]
ORIGIN = E / "aws-normal-full-r1"
REVISION = "airk-r1:94fa9968172443534a235186861f7e13:18:4e07b7b5e59c2e6c62c68fd02d1e783066b848f16c8e64e964c8557cc50a10e1"
REGISTER = "154b27e8f8c4162608b50f9bcd4406e9f9adf2dea29594679403eab8cd474a83"
AUDIT = "eb894d16b54559fde8654fd808b475db456e72a832e9e727a0687b547b5be385"
STOP = "0d4346c4108863de416e3f26b95b98304aab125cb329924320198924477dad50"
WALL = 3600


def inventory(root, deadline):
    """Hash relative names, identity, timestamps, lengths and exact regular-file bytes."""
    digest = hashlib.sha256()
    stack = [root]
    count = total = 0
    while stack:
        path = stack.pop()
        require(time.monotonic() < deadline, "store inventory deadline")
        status = path.lstat()
        require(not status.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT, "unexpected reparse point")
        is_directory = stat.S_ISDIR(status.st_mode)
        require(is_directory or stat.S_ISREG(status.st_mode), "unexpected store entry")
        identity = [path.relative_to(root).as_posix(), is_directory, status.st_dev, status.st_ino,
                    status.st_size, status.st_mtime_ns, status.st_file_attributes]
        if is_directory:
            stack.extend(sorted(path.iterdir(), reverse=True))
        else:
            total += status.st_size
            require(total <= 12 << 30, "store exceeds reservation")
            with path.open("rb") as stream:
                identity.append(hashlib.file_digest(stream, "sha256").hexdigest())
        digest.update(json.dumps(identity, ensure_ascii=True, separators=(",", ":")).encode() + b"\n")
        count += 1
        require(count <= 500_000, "store inventory exceeded bound")
    return {"entries": count, "bytes": total, "sha256": digest.hexdigest()}


@contextmanager
def deny_database_writes(directory):
    # SQLite may try a write-capable SHM open even for a read-only connection.
    # Deny writes/deletion to the original DB and its actual journal companions.
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                                wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    api.CreateFileW.restype = wintypes.HANDLE
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    handles = []
    try:
        for name in ("current.sqlite3", "current.sqlite3-shm", "current.sqlite3-wal"):
            handle = api.CreateFileW(str(directory / name), 0x80000000, 1, None, 3, 0, None)
            if handle == ctypes.c_void_p(-1).value:
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle)
        yield
    finally:
        for handle in handles:
            api.CloseHandle(handle)


def observe_pages(w, command, output, env, deadline, member):
    """Give each real page a full 120s budget within the single owned process."""
    path = output / "native-list.stdout"
    events = queue.Queue()
    started = time.perf_counter_ns()
    with path.open("xb") as stream, (output / "native-list.stderr").open("xb") as err, subprocess.Popen(
        command, cwd=n["ROOT"], env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=err,
        creationflags=subprocess.CREATE_NO_WINDOW,
    ) as child:
        def consume():
            try:
                for i in range(501):
                    raw = child.stdout.readline((128 << 10) + 1)
                    if not raw:
                        events.put(None)
                        return
                    require(raw.endswith(b"\n") and len(raw) <= 128 << 10, "oversized native page")
                    require(i < 500, "native page count exceeds publication bound")
                    stream.write(raw)
                    events.put(i)
            except Exception as error:
                events.put(error)

        reader = threading.Thread(target=consume)
        reader.start()
        forced = False
        try:
            member(child.pid)
            while True:
                remaining = min(120, deadline - time.monotonic())
                require(remaining > 0, "native census deadline")
                event = events.get(timeout=remaining)
                if event is None:
                    break
                if isinstance(event, Exception):
                    raise event
            code = child.wait(timeout=max(0, min(120, deadline - time.monotonic())))
            require(code == 0, "native Kernel selector refused the read")
        finally:
            if child.poll() is None:
                forced = True
                child.kill()
            code = child.wait()
            reader.join()
            w["write_json"](output / "native-list.command.json", {"argv": list(map(str, command)),
                "pid": child.pid, "exit_code": code, "forced": forced,
                "elapsed_ns": time.perf_counter_ns() - started})
    return path


def run(w, owner, output, image, proof, deadline, member, final_baseline):
    n["checked"](E / "aws-normal-full-r1-custody.private.json", AUDIT)
    closed = E / "aws-normal-reinspect-r1/owned.json"
    n["checked"](closed, STOP)
    prior = w["read_json"](closed)
    require(prior["owned_descendants_settled"] and prior["owned_job_final_active_processes"] == 0
            and not prior["owned_job_forced_termination"], "original service not settled")
    journal = ORIGIN / "commands/register.stdout"
    n["checked"](journal, REGISTER)
    with journal.open("rb") as stream:
        _, applied = map(json.loads, stream)
    project = applied["receipt"]["request"]["registration"]
    store = owner.plain(ORIGIN / "index")
    inc = store / "collections-1.4" / project["collection"] / "incarnations" / REVISION.split(":")[1]
    quota_proof = E / "aws-durable-store-quota-r1.private.json"
    n["checked"](quota_proof, "d47c3ace56d068a85609887de801d4ca0ab297c70806a9d17d23aeda82f1d423")
    quota = store / ".quota-1.4"
    require({p.name for p in quota.iterdir()} == {"ledger.lock", "current-grants", "reservations"}
            and not any((quota / "reservations").iterdir()), "native quota observation would recover other state")
    grant, = (quota / "current-grants").iterdir()
    n["checked"](grant, "7b6ddaa8716f57d7a25e5018196d197f169c99e37634da468495bb0df63efa51")
    menu = normal["menu"]
    source = E / menu["INVENTORY"]
    n["checked"](source, menu["INVENTORY_SHA"])
    _, cases, members = menu["inputs"](w["read_json"](source))
    require(len(members) == 35515, "source roster differs")
    command = [image, project["root"], project["data_root"], project["collection"], project["binding"], REVISION, "35400"]
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG", "PYTHON"))}
    if final_baseline:
        final = E / "aws-candidate-retained-r5"
        for name, seal in {"owned.json": "24b7af6875592d9072551ecab5743eb305a6fe30fcc1c18ecf3b2547db781da9",
                           "owner-audit.json": "8b42ba401fc34834c10da71301181d15cabbd8c43f21fb0ff1146d6e114b96f5",
                           "after.json": "7045aa52b6afed8242109adef960934ffc39566e62a93c437d86224464e6d236",
                           "public-native-census.private.json": "89f9f9f535f6f529ee585784b6dd56d124e8318f14efb2af6acda36d7d0697d2"}.items():
            n["checked"](owner.plain(final / name, regular=True), seal)
        normal["stage"]["require_settlement"](w, w["read_json"](final / "owned.json"))
        n["checked"](ORIGIN / "control/service-1.4.sqlite3", "e868fc8e1b65881bdc42141bf586ae704f0dc80b2a9e2592fdd8469e0eef3fab")
        witness = runpy.run_path(str(HERE / "aws_kernel_entry_evidence.py"))

    def capture(name):
        if final_baseline:
            return witness["capture"](store, output / (name + ".rows"), deadline)
        return inventory(store, deadline)

    with deny_database_writes(inc):
        before = capture("before")
        if final_baseline:
            require(before == w["read_json"](final / "after.json"), "R5 stopped baseline changed")
        w["write_json"](output / "store-before.json", before)
        try:
            native = observe_pages(w, command, output, env, deadline, member)
        finally:
            after = capture("after")
            w["write_json"](output / "store-after.json", after)
            if final_baseline:
                comparison = witness["compare"](output / "before.rows", output / "after.rows")
                w["write_json"](output / "comparison.json", comparison)
                require(comparison["different_rows"] == 0, "native kernel read changed an original entry")
            require(before == after, "original native store changed")
    rows = set()
    last = None
    pages = 0
    with native.open("rb") as stream:
        for line in stream:
            require(last is None or last["truncated"], "extra native pages after terminal")
            require(len(line) <= 128 << 10, "oversized page")
            last = json.loads(line)
            paths = last["paths"]
            require(last["publication"]["token"] == REVISION and last["collection"] == project["collection"]
                    and len(paths) == len(set(paths)) and set(paths) <= members.keys() and not rows.intersection(paths),
                    "wrong native paths/revision")
            rows.update(paths)
            pages += 1
    require(last is not None and not last["truncated"] and len(rows) == 35400, "incomplete native pages")
    absent = sorted(members.keys() - rows)
    positives = {row["source_path"].removeprefix("documents/") for row in cases[0]["cases"]}
    w["write_json"](output / "kernel-census.private.json", {"original_custody": AUDIT, "closed_service": STOP,
        "original_registration": REGISTER, "original_status_revision": REVISION, "expected_sources": len(members),
        "evidence": "kernel_existing_service_observer", "visible_native_paths": len(rows), "pages": pages,
        "visible_positives": sorted(positives & rows), "unlisted_paths": absent,
        "native_listing_sha256": owner.digest(native), "artifact_proof_sha256": owner.digest(proof),
        "executable_sha256": owner.digest(image), "source_only_config_applied": False,
        "live_service_scope_observed": False, "ranker_observed": False, "scored_requests": 0, "before": before, "after": after})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--image-proof", type=Path, required=True)
    parser.add_argument("--proof-sha", required=True)
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--from-retained-r5", action="store_true", help="require the sealed R5 store and retain both entry maps")
    args = parser.parse_args()
    n["checked"](n["WRAPPER"], n["WRAPPER_SHA"])
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    proof = owner.plain(args.image_proof, regular=True)
    require(output.parent == E and output.name.startswith("aws-kernel-census-")
            and proof.parent.parent == E and proof.name == "observer-image.private.json", "wrong private input/output")
    n["checked"](proof, args.proof_sha)
    data = w["read_json"](proof)
    source = n["ROOT"] / "crates/airk-kernel/examples/observe_discovery_publication.rs"
    n["checked"](source, data["source_sha256"])
    receipt = owner.plain(proof.parent / "aws-kernel-build-r1.json", regular=True)
    n["checked"](receipt, data["build_sha256"])
    build = w["read_json"](receipt)
    require(build["passed"] and build["inputs_unchanged"] and build["exit_code"] == 0
            and all(arg in build["command"] for arg in ("build", "--locked", "--offline", "airk-kernel",
                    "--example", "observe_discovery_publication", "--no-default-features")), "native Cargo build differs")
    image = owner.plain(proof.parent / "observe_discovery_publication.exe", regular=True)
    n["checked"](image, data["image_sha256"])
    if args.worker:
        require(os.environ["AIRK_AWS_KERNEL_OWNER"] == str(output), "owned worker required")
        with normal["capped"](w, output, 2 << 30) as (member, _sample):
            run(w, owner, output, image, proof, float(os.environ["AIRK_AWS_KERNEL_DEADLINE"]), member, args.from_retained_r5)
        return
    require(not output.exists(), "fresh output required")
    wall = min(WALL, int(1795194600 - time.time()))
    require(wall > 120, "insufficient campaign wall time")
    output.mkdir()
    command = [sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"]
    result = owner.run_evaluation_fixture(command, n["ROOT"], output / "outer.log", wall, os.environ | {
        "AIRK_AWS_KERNEL_OWNER": str(output), "AIRK_AWS_KERNEL_DEADLINE": str(time.monotonic() + wall)})
    w["write_json"](output / "owned.json", result)
    normal["stage"]["require_settlement"](w, result)
    print(owner.digest(output / "kernel-census.private.json"))


if __name__ == "__main__":
    main()
