# SPDX-License-Identifier: Apache-2.0
"""Measure bounded indexing/retrieval on original scale fixtures and an optional pinned public CPython tree."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import subprocess
import tarfile
import time
import urllib.request

CPYTHON_REVISION = "ebf955df7a89ed0c7968f79faec1de49f61ed7cb"
CPYTHON_URL = f"https://codeload.github.com/python/cpython/tar.gz/{CPYTHON_REVISION}"
CPYTHON_ARCHIVE_SHA256 = "57601e7fdcf7ce150e76e0af106768ec7491c39b246b45a1599c06d0c6f61bdc"
QUALIFICATION_QUOTA = {"root_bytes": 32 * 1024**3, "default_collection_bytes": 16 * 1024**3, "collections": {}}


def hash_file(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
            size += len(block)
    return digest.hexdigest(), size


def process_metrics(process, expected_executable=None):
    """OS process counters, never a claim about physical SSD traffic."""
    if os.name == "nt":
        from ctypes import wintypes
        class Memory(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                (name, ctypes.c_size_t) for name in ("PeakWorkingSetSize", "WorkingSetSize",
                "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]
        class Io(ctypes.Structure):
            _fields_ = [(name, ctypes.c_uint64) for name in ("ReadOperationCount", "WriteOperationCount",
                "OtherOperationCount", "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        memory, io = Memory(), Io()
        memory.cb = ctypes.sizeof(memory)
        handle = wintypes.HANDLE(int(process._handle))
        result = {"counter_basis": "Windows process working-set and logical I/O transfer counters"}
        if kernel.K32GetProcessMemoryInfo(handle, ctypes.byref(memory), ctypes.sizeof(memory)) and memory.PeakWorkingSetSize:
            result["peak_rss_bytes"] = memory.PeakWorkingSetSize
        if kernel.GetProcessIoCounters(handle, ctypes.byref(io)):
            result.update(read_bytes=io.ReadTransferCount, write_bytes=io.WriteTransferCount)
        timestamps = [ctypes.c_uint64() for _ in range(4)]
        if kernel.GetProcessTimes(handle, *(ctypes.byref(item) for item in timestamps)):
            result["cpu_us"] = (timestamps[2].value + timestamps[3].value) // 10
        return result
    if platform.system() == "Linux":
        result = {"counter_basis": "Linux sampled post-exec procfs high-water RSS and storage I/O counters"}
        try:
            executable = Path(f"/proc/{process.pid}/exe")
            if expected_executable is None or executable.resolve(strict=True) != expected_executable:
                return result
            for line in Path(f"/proc/{process.pid}/status").read_text().splitlines():
                if line.startswith("VmHWM:"):
                    result["peak_rss_bytes"] = int(line.split()[1]) * 1024
            values = dict(line.split(": ", 1) for line in Path(f"/proc/{process.pid}/io").read_text().splitlines())
            result.update(read_bytes=int(values["read_bytes"]), write_bytes=int(values["write_bytes"]))
            stat = Path(f"/proc/{process.pid}/stat").read_text().rsplit(") ", 1)[1].split()
            result["cpu_us"] = (int(stat[11]) + int(stat[12])) * 1_000_000 // os.sysconf("SC_CLK_TCK")
        except (OSError, ValueError, KeyError):
            pass
        return result
    return {"counter_basis": "OS counters unavailable in this harness; inspect native usage resource report"}


def measured(command, output_directory, label, timeout=1800):
    started = time.monotonic()
    path = output_directory / (label + ".json")
    errors = output_directory / (label + ".stderr")
    metrics = {}
    expected_executable = None
    if platform.system() == "Linux":
        executable = shutil.which(str(command[0])) or str(command[0])
        expected_executable = Path(executable).resolve(strict=True)
    with path.open("wb") as output, errors.open("wb") as error:
        process = subprocess.Popen(command, stdout=output, stderr=error, stdin=subprocess.DEVNULL)
        try:
            while True:
                if platform.system() == "Linux":
                    completed, status, usage = os.wait4(process.pid, os.WNOHANG)
                    if completed:
                        process.returncode = os.waitstatus_to_exitcode(status)
                        metrics["cpu_us"] = round((usage.ru_utime + usage.ru_stime) * 1_000_000)
                        # Linux can retain the launcher's pre-exec resident size
                        # in wait4's high-water mark. Keep it separate from the
                        # sampled executable VmHWM, which resets at exec.
                        metrics["child_lifetime_peak_rss_bytes"] = usage.ru_maxrss * 1024
                        metrics["counter_basis"] = "Linux wait4 final CPU; sampled post-exec procfs RSS/I/O; lifetime RSS may include launcher memory"
                        metrics["rss_sampling"] = "20 ms; post-exec high-water samples can miss final growth or a short-lived executable"
                        break
                elif process.poll() is not None:
                    break
                sample = process_metrics(process, expected_executable)
                if platform.system() == "Linux" and "peak_rss_bytes" in sample:
                    metrics["rss_samples"] = metrics.get("rss_samples", 0) + 1
                for key, value in sample.items():
                    metrics[key] = max(metrics.get(key, 0), value) if isinstance(value, int) else value
                if time.monotonic() - started > timeout:
                    raise TimeoutError(f"{label} exceeded {timeout} seconds")
                time.sleep(0.02)
            if os.name == "nt":
                for key, value in process_metrics(process).items():
                    metrics[key] = max(metrics.get(key, 0), value) if isinstance(value, int) else value
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    metrics.update(label=label, wall_ms=round((time.monotonic() - started) * 1000, 3), exit_code=process.returncode)
    if process.returncode:
        raise RuntimeError(f"{label} failed: {errors.read_text(errors='replace')[:4000]}")
    if path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("benchmark command response exceeds 16 MiB")
    return json.loads(path.read_text(encoding="utf-8-sig")), metrics


def machine_environment(directory):
    result = {"logical_cpus": os.cpu_count(), "processor": platform.processor(),
              "disk_total_bytes": shutil.disk_usage(directory).total,
              "disk_free_bytes_before": shutil.disk_usage(directory).free}
    if os.name == "nt":
        from ctypes import wintypes
        class MemoryStatus(ctypes.Structure):
            _fields_ = [("length", wintypes.DWORD), ("load", wintypes.DWORD)] + [
                (name, ctypes.c_uint64) for name in ("total_physical", "available_physical",
                "total_pagefile", "available_pagefile", "total_virtual", "available_virtual", "extended")]
        status = MemoryStatus()
        status.length = ctypes.sizeof(status)
        if ctypes.WinDLL("kernel32", use_last_error=True).GlobalMemoryStatusEx(ctypes.byref(status)):
            result["physical_memory_bytes"] = status.total_physical
    elif platform.system() == "Linux":
        result["physical_memory_bytes"] = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    return result


def logical_bytes(root):
    return sum(path.stat().st_size for path in root.rglob("*") if path.is_file() and not path.is_symlink())


def source_digest(root):
    digest = hashlib.sha256()
    count = size = 0
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            name = path.relative_to(root).as_posix().encode()
            content_hash, content_bytes = hash_file(path)
            digest.update(len(name).to_bytes(8, "little") + name + bytes.fromhex(content_hash))
            count += 1
            size += content_bytes
    return {"files": count, "bytes": size, "inventory_sha256": digest.hexdigest()}


def limits(files, source_bytes):
    return {"file_bytes": 16_777_216, "files": max(100_000, files + 10),
            "corpus_bytes": max(1_073_741_824, source_bytes * 2),
            "extracted_bytes": max(2_147_483_648, source_bytes * 4),
            "discovery_entries": max(1_000_000, files * 4),
            "read_bytes": max(2_147_483_648, source_bytes * 4)}


def fixture_bytes(index, size, revised=False):
    first = f"{'revisedprobe' if revised else 'airkprobe'}{index:08d}\n".encode()
    filler = b"Original synthetic text for bounded storage measurement and retrieval verification.\n"
    return (first + filler * ((size // len(filler)) + 1))[:size - 1] + b"\n"


def synthetic(binary, directory, count, size, measurements, command_timeout=1800):
    root, state = directory / "synthetic", directory / "synthetic-state"
    root.mkdir()
    for index in range(count):
        folder = root / f"group{index // 256:04d}"
        folder.mkdir(exist_ok=True)
        (folder / f"item{index:08d}.txt").write_bytes(fixture_bytes(index, size))
    config = directory / "synthetic-limits.json"
    config.write_text(json.dumps(limits(count, count * size)), encoding="utf-8")
    base = [str(binary), "--root", str(root), "--data-dir", str(state)]
    def run(label, *args):
        result, metrics = measured(base + list(args), directory, "synthetic-" + label, timeout=command_timeout)
        measurements.append(metrics)
        return result
    run("configure", "collections", "limits", "set", str(config))
    quota = directory / "synthetic-quota.json"
    quota.write_text(json.dumps(QUALIFICATION_QUOTA), encoding="utf-8")
    run("configure-quota", "collections", "quota", "set", str(quota))
    first = run("index", "index")
    if first["indexed"] != count or first["source_bytes"] != count * size:
        raise AssertionError("synthetic corpus was not indexed completely")
    probes = sorted(set([0, count // 4, count // 2, count - 1]))
    for sample in range(3):
        for index in probes:
            found = run(f"search-{sample}-{index}", "search", f"airkprobe{index:08d}", "--limit", "1", "--max-bytes", "512")
            expected = f"group{index // 256:04d}/item{index:08d}.txt"
            if len(found["hits"]) != 1 or found["hits"][0]["citation"]["path"] != expected:
                raise AssertionError("exact sentinel retrieval gate failed")
    unchanged = run("reuse", "index")
    if unchanged["reused"] != count or unchanged["indexed"] != 0 or unchanged["generation"] != first["generation"]:
        raise AssertionError("unchanged streaming revision was not reused")
    path = "group0000/item00000000.txt"
    before = run("read-before", "read", path, "--start-line", "1", "--end-line", "1")
    (root / path).write_bytes(fixture_bytes(0, size, revised=True))
    (root / "group0000/item00000001.txt").unlink()
    retained = run("read-retained", "read", path, "--start-line", "1", "--end-line", "1")
    if any(retained[key] != before[key] for key in ("collection", "generation", "hits", "truncated")):
        raise AssertionError("live source changes altered retained evidence before publication")
    changed = run("changed", "index")
    if changed["indexed"] != 1 or changed["removed"] != 1 or changed["generation"] <= first["generation"]:
        raise AssertionError("changed/deleted source revision gate failed")
    revised = run("search-revised", "search", "revisedprobe00000000", "--limit", "1")
    deleted = run("search-deleted", "search", "airkprobe00000001", "--limit", "1")
    if revised["hits"][0]["citation"]["path"] != path or deleted["hits"]:
        raise AssertionError("published update/delete retrieval gate failed")
    usage = run("usage", "usage", "report")
    return {"kind": "original synthetic", "initial_files": count, "initial_source_bytes": count * size,
            "first_index": first, "reuse": unchanged, "changed": changed,
            "logical_index_bytes": logical_bytes(state), "usage": usage,
            "gates": "complete inventory, exact sentinel hits, unchanged reuse, retained prepublication evidence, changed/deleted publication"}


def fetch_cpython(directory):
    archive = directory / "cpython.tar.gz"
    digest = hashlib.sha256()
    count = 0
    with urllib.request.urlopen(CPYTHON_URL, timeout=120) as source, archive.open("xb") as output:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            count += len(block)
            if count > 134_217_728:
                raise ValueError("public archive exceeds 128 MiB download budget")
            digest.update(block)
            output.write(block)
    if digest.hexdigest() != CPYTHON_ARCHIVE_SHA256:
        raise ValueError("public archive differs from the qualified commit archive digest")
    root = directory / "cpython"
    root.mkdir()
    total = entries = skipped_links = 0
    prefix = "cpython-" + CPYTHON_REVISION
    with tarfile.open(archive, "r|gz") as stream:
        for member in stream:
            entries += 1
            parts = PurePosixPath(member.name).parts
            if entries > 100_000 or not parts or parts[0] != prefix or any(part in (".", "..") or ":" in part or "\\" in part for part in parts):
                raise ValueError("unsafe or oversized public archive")
            target = root.joinpath(*parts[1:])
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                total += member.size
                if member.size > 33_554_432 or total > 1_073_741_824:
                    raise ValueError("public archive exceeds extracted-byte budget")
                target.parent.mkdir(parents=True, exist_ok=True)
                with stream.extractfile(member) as source, target.open("xb") as output:
                    for block in iter(lambda: source.read(1024 * 1024), b""):
                        output.write(block)
            else:
                skipped_links += 1
    return root, {"repository": "https://github.com/python/cpython", "revision": CPYTHON_REVISION,
                  "archive_sha256": digest.hexdigest(), "download_bytes": count, "omitted_nonregular_entries": skipped_links}


def public_corpus(binary, directory, measurements, command_timeout=1800):
    root, provenance = fetch_cpython(directory)
    before = source_digest(root)
    state = directory / "cpython-state"
    config = directory / "cpython-limits.json"
    config.write_text(json.dumps(limits(before["files"], before["bytes"])), encoding="utf-8")
    base = [str(binary), "--root", str(root), "--data-dir", str(state)]
    def run(label, *args):
        result, metrics = measured(base + list(args), directory, "cpython-" + label, timeout=command_timeout)
        measurements.append(metrics)
        return result
    run("configure", "collections", "limits", "set", str(config))
    quota = directory / "cpython-quota.json"
    quota.write_text(json.dumps(QUALIFICATION_QUOTA), encoding="utf-8")
    run("configure-quota", "collections", "quota", "set", str(quota))
    report = run("index", "index")
    status = run("status", "status")
    for label, query, prefix in [("interpreter", "_PyEval_EvalFrameDefault", "Python/ceval.c"),
                                  ("documentation", "hashlib", "Doc/library/hashlib.rst")]:
        hits = run(label, "search", query, "--path-prefix", prefix, "--limit", "3", "--max-bytes", "2048")["hits"]
        if not hits or any(hit["citation"]["path"] != prefix for hit in hits):
            raise AssertionError("public-corpus exact query gate failed")
        expected_hash = hash_file(root / prefix)[0]
        if any(hit["citation"]["content_hash"] != expected_hash for hit in hits):
            raise AssertionError("public-corpus source citation hash differs")
    symbols = run("symbol", "symbol", "Path", "--language", "python", "--path-prefix", "Lib/pathlib")["facts"]
    if not any(fact["name"] == "Path" for fact in symbols):
        raise AssertionError("public-corpus Python definition gate failed")
    after = source_digest(root)
    if after != before:
        raise AssertionError("public source tree was mutated")
    return {"kind": "pinned public CPython", "provenance": provenance, "source": before,
            "index": report, "status": status, "logical_index_bytes": logical_bytes(state),
            "usage": run("usage", "usage", "report"), "gates": "exact code/doc queries, citation hashes, native Python definition, read-only source tree"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="new directory for fixtures/state/results; retained for review")
    parser.add_argument("--files", type=int, default=16_384)
    parser.add_argument("--bytes-per-file", type=int, default=8192)
    parser.add_argument("--cpython", action="store_true", help="explicitly download and index the pinned public CPython release")
    parser.add_argument("--command-timeout-seconds", type=int, default=1800, help="per-command deadline, 1..5400 seconds (default: 1800)")
    args = parser.parse_args()
    if not 1 <= args.command_timeout_seconds <= 5400:
        parser.error("command timeout must be 1..5400 seconds")
    if not 2 <= args.files <= 100_000 or not 128 <= args.bytes_per_file <= 1_048_576:
        parser.error("file count or size exceeds qualification harness bounds")
    if args.files * args.bytes_per_file > 2_147_483_648:
        parser.error("synthetic source bytes exceed the harness's 2 GiB limit")
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve(strict=True)
    measurements = []
    result = {"schema": 1, "platform": platform.platform(), "machine": machine_environment(directory),
              "binary_sha256": hash_file(binary)[0],
              "cache_conditions": "fresh index directory; OS caches are neither flushed nor controlled; repeated CLI calls have fresh process caches",
              "model": "none; this harness measures lexical/exact storage and lifecycle, not transformer quality",
              "quota_policy": QUALIFICATION_QUOTA,
              "command_timeout_seconds": args.command_timeout_seconds,
              "measurements": measurements, "corpora": []}
    try:
        result["corpora"].append(synthetic(binary, directory, args.files, args.bytes_per_file, measurements, args.command_timeout_seconds))
        if args.cpython:
            result["corpora"].append(public_corpus(binary, directory, measurements, args.command_timeout_seconds))
        result["passed"] = True
    except Exception as error:
        result.update(passed=False, error=str(error))
        raise
    finally:
        (directory / "report.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "report": str(directory / "report.json"), "corpora": len(result["corpora"])}))


if __name__ == "__main__":
    main()
