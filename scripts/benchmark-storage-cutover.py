# SPDX-License-Identifier: Apache-2.0
"""Compare independently identified format-140/150 debug pairs in isolated original fixtures.

Measures native client end-to-end latency, including client startup and bounded
job polling. OS caches are warm and are not flushed. No private database is read.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
COMPARISON_QUOTA = {"root_bytes": 512 * 1024**2, "default_collection_bytes": 128 * 1024**2, "collections": {}}

def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded

clients = module("cutover_native_clients", "check-service-clients.py")
scale = module("cutover_native_measurements", "benchmark-scale.py")

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def identity(runtime):
    pair = json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))
    build = json.loads((runtime / "airk-build.json").read_text(encoding="utf-8"))
    names = ["airk.exe", "airk-embed.exe"] if os.name == "nt" else ["airk", "airk-embed"]
    if digest(runtime/names[0])!=pair["airk_sha256"] or digest(runtime/names[1])!=pair["embed_sha256"]:
        raise ValueError("Runtime pair executable hash mismatch")
    return {"runtime": str(runtime), "version": pair["version"], "profile": build["profile"],
            "source": build["source"], "rustc": build["rustc"], "executables": {name: {"sha256": digest(runtime/name),
                "bytes": (runtime/name).stat().st_size} for name in names}}

def distribution(values):
    ordered = sorted(values)
    quantile = lambda fraction: ordered[max(0, math.ceil(len(ordered)*fraction)-1)]
    return {"count": len(ordered), "p50_ms": quantile(.5), "p95_ms": quantile(.95),
            "p99_ms": quantile(.99), "min_ms": ordered[0], "max_ms": ordered[-1],
            "throughput_per_second":len(values)*1000/sum(values), "mean_ms": statistics.mean(ordered), "stdev_ms": statistics.stdev(ordered) if len(ordered)>1 else 0,
            "samples_ms": values}

def evidence(result):
    return [{"path": hit["citation"]["path"], "location": hit["citation"].get("location"),
             "text_sha256": hashlib.sha256(hit["text"].encode()).hexdigest()} for hit in result["hits"]]

def fixture_token(number):
    return "fixture" + chr(ord("a") + number // 26) + chr(ord("a") + number % 26)

def corpus(source, revision):
    for number in range(32):
        directory=source/("selected" if number<8 else "other")
        directory.mkdir(exist_ok=True)
        (directory/f"fixture_{number:02}.rs").write_text(
            f"// CommonNeedle original synthetic source revision {revision}\n"+
            "\n".join(f'pub fn rare_{number:02}_{item}(value: usize) -> usize {{ let _marker = "{fixture_token(number*8+item)}"; value + {item+revision} }}' for item in range(8))+"\n",
            encoding="utf-8")

def process_snapshot():
    """Read process ancestry and current RSS; never signal a discovered process."""
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes
        class Entry(ctypes.Structure):
            _fields_ = [("size", wintypes.DWORD), ("usage", wintypes.DWORD),
                        ("pid", wintypes.DWORD), ("heap", ctypes.c_size_t),
                        ("module", wintypes.DWORD), ("threads", wintypes.DWORD),
                        ("parent", wintypes.DWORD), ("priority", wintypes.LONG),
                        ("flags", wintypes.DWORD), ("exe", wintypes.WCHAR * 260)]
        class Memory(ctypes.Structure):
            _fields_ = [("size", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
                (name, ctypes.c_size_t) for name in ("peak_rss", "rss", "peak_pool",
                    "pool", "peak_nonpaged", "nonpaged", "pagefile", "peak_pagefile")]
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
        kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
        kernel.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
        kernel.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.K32GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Memory), wintypes.DWORD]
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        snapshot = kernel.CreateToolhelp32Snapshot(2, 0)
        if snapshot == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        rows = {}
        try:
            entry = Entry(); entry.size = ctypes.sizeof(entry)
            present = kernel.Process32FirstW(snapshot, ctypes.byref(entry))
            while present:
                rows[entry.pid] = {"parent": entry.parent, "name": entry.exe, "rss": None}
                present = kernel.Process32NextW(snapshot, ctypes.byref(entry))
        finally:
            kernel.CloseHandle(snapshot)
        # Only read memory for this fixture's lineage, not unrelated AIRK sessions.
        owned = descendant_ids(rows, os.getpid())
        for pid in owned:
            handle = kernel.OpenProcess(0x1000, False, pid)
            if handle:
                try:
                    memory = Memory(); memory.size = ctypes.sizeof(memory)
                    if kernel.K32GetProcessMemoryInfo(handle, ctypes.byref(memory), memory.size):
                        rows[pid]["rss"] = memory.rss
                    else:
                        exit_code = wintypes.DWORD()
                        if kernel.GetExitCodeProcess(handle, ctypes.byref(exit_code)) and exit_code.value != 259:
                            rows[pid]["exited_before_rss"] = True
                finally:
                    kernel.CloseHandle(handle)
            elif ctypes.get_last_error() == 87:
                # A short-lived client can disappear after the ancestry snapshot.
                # Access-denied and other failures remain missing observations.
                rows[pid]["exited_before_rss"] = True
        return rows
    if Path("/proc/self/stat").exists():
        rows = {}
        for directory in Path("/proc").iterdir():
            if not directory.name.isdigit(): continue
            try:
                value = (directory/"stat").read_text()
                name, fields = value[value.index("(")+1:value.rindex(")")], value[value.rindex(")")+2:].split()
                rows[int(directory.name)] = {"parent": int(fields[1]), "name": name,
                                             "rss": int(fields[21])*os.sysconf("SC_PAGE_SIZE")}
            except (OSError, ValueError, IndexError):
                continue  # A child may exit between enumeration and observation.
        return rows
    result = subprocess.run(["ps", "-axo", "pid=,ppid=,rss=,comm="],
                            check=True, capture_output=True, text=True)
    rows = {}
    for line in result.stdout.splitlines():
        pid, parent, rss, name = line.split(None, 3)
        rows[int(pid)] = {"parent": int(parent), "name": Path(name).name, "rss": int(rss)*1024}
    return rows

def descendant_ids(rows, root):
    descendants = {root}
    while True:
        expanded = descendants | {pid for pid, row in rows.items() if row["parent"] in descendants}
        if expanded == descendants: return descendants - {root}
        descendants = expanded

class ResourceSampler:
    """Sample this Python fixture's native children, plus its isolated data tree."""
    def __init__(self, data):
        self.data = data
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.samples = 0
        self.peak_rss = self.peak_disk = self.peak_processes = 0
        self.missing_rss = 0
        self.error = None

    def sample(self):
        rows = process_snapshot()
        descendants = descendant_ids(rows, os.getpid())
        owned = [row for pid, row in rows.items() if pid in descendants
                 and row["name"].lower() in ("airk", "airk.exe", "airk-embed", "airk-embed.exe")
                 and not row.get("exited_before_rss", False)]
        self.samples += 1
        self.peak_processes = max(self.peak_processes, len(owned))
        self.missing_rss += sum(row["rss"] is None for row in owned)
        self.peak_rss = max(self.peak_rss, sum(row["rss"] or 0 for row in owned))
        disk = 0
        for path in self.data.rglob("*"):
            try:
                if path.is_file() and not path.is_symlink(): disk += path.stat().st_size
            except FileNotFoundError:
                continue  # Private staging may retire during this observation.
        self.peak_disk = max(self.peak_disk, disk)

    def run(self):
        try:
            while not self.stop.wait(.1): self.sample()
        except Exception as error:
            self.error = repr(error)

    def finish(self):
        self.stop.set(); self.thread.join()
        if self.error: raise RuntimeError(f"Resource sampling failed: {self.error}")
        if not self.samples or not self.peak_rss or self.missing_rss:
            raise RuntimeError(f"Incomplete native process memory samples: {self.result()}")

    def result(self):
        return {"interval_ms": 100, "samples": self.samples,
                "peak_simultaneous_native_rss_bytes": self.peak_rss,
                "peak_observed_native_processes": self.peak_processes,
                "missing_rss_observations": self.missing_rss,
                "peak_sampled_logical_data_bytes": self.peak_disk,
                "scope": "fixture descendant broker/project/helper/client processes; sampled sum of current RSS; data tree logical lengths",
                "limits": "100ms sampling may miss short-lived peaks; shared pages can be counted in multiple processes; logical lengths are not allocated disk blocks"}

def run_one(runtime, output, repetitions, samples):
    output.mkdir()
    source=output/"source";data=output/"data";owner=output/"owner"
    for directory in (source,data,owner): directory.mkdir()
    fixture=clients.Fixture(runtime,owner,job_timeout=600)
    measurements={name:[] for name in ("initial_32_file_publication","warm_common","warm_rare","warm_prefix","replace_32_file_publication","delete_publication","warm_vector","replace_32_file_semantic_publication")}
    signatures={}
    sampler = ResourceSampler(data)
    sampler.thread.start()
    try:
        fixture.start();fixture.register("cutover",source,data)
        fixture.rpc({"configure_watch":{"project":fixture.projects["cutover"]["binding"],"enabled":False}})
        quota = owner/"comparison-quota.json"
        quota.write_text(json.dumps(COMPARISON_QUOTA),encoding="utf-8")
        fixture.configure("cutover",{"operation":"quota_policy","path":str(quota.resolve())})
        fixture.configure("cutover",{"operation":"vectors","enabled":False})
        fixture.configure("cutover",{"operation":"usage","mode":"disabled"})
        # Finish configuration-triggered empty scans before creating measured sources.
        fixture.wait(fixture.enqueue("cutover","index"))
        assert fixture.tool("cutover","airk_status",{"response_mode":"full"})["files"] == 0
        corpus(source,0)
        begin=time.perf_counter_ns();fixture.wait(fixture.enqueue("cutover","index"))
        measurements["initial_32_file_publication"].append((time.perf_counter_ns()-begin)/1e6)
        cases={"warm_common":{"query":"CommonNeedle","mode":"lexical","limit":100,"max_bytes":65536,"response_mode":"full"},
               "warm_rare":{"query":"rare_03_4","mode":"lexical","limit":10,"max_bytes":8192,"response_mode":"full"},
               "warm_prefix":{"query":"CommonNeedle","path_prefix":"selected/","mode":"lexical","limit":100,"max_bytes":65536,"response_mode":"full"}}
        cold={}
        for name,arguments in cases.items():
            begin=time.perf_counter_ns();result=fixture.tool("cutover","airk_search",arguments)
            cold[name]=(time.perf_counter_ns()-begin)/1e6
            signatures[name]=evidence(result)
        for repetition in range(repetitions):
            for sample in range(samples):
                for name,arguments in cases.items():
                    begin=time.perf_counter_ns();result=fixture.tool("cutover","airk_search",arguments)
                    measurements[name].append((time.perf_counter_ns()-begin)/1e6)
                    if evidence(result)!=signatures[name]: raise AssertionError("Warm query evidence changed")
            # Every file changes while source locations and record counts stay fixed.
            corpus(source,repetition+1)
            begin=time.perf_counter_ns();fixture.wait(fixture.enqueue("cutover","index"))
            measurements["replace_32_file_publication"].append((time.perf_counter_ns()-begin)/1e6)
            target=source/"other/fixture_31.rs";saved=target.read_bytes();target.unlink()
            begin=time.perf_counter_ns();fixture.wait(fixture.enqueue("cutover","index"))
            measurements["delete_publication"].append((time.perf_counter_ns()-begin)/1e6)
            target.write_bytes(saved);fixture.wait(fixture.enqueue("cutover","index"))
            # Establish the next repetition's exact evidence after publication.
            signatures={name:evidence(fixture.tool("cutover","airk_search",arguments)) for name,arguments in cases.items()}
        model=output/"model";model.mkdir()
        # Distinct source directions avoid tied top-k results hiding a ranking difference.
        vocabulary=json.dumps({fixture_token(number):[math.cos(number*.004), math.sin(number*.004)]
                               for number in range(32*8)}).encode()
        (model/"vocabulary.json").write_bytes(vocabulary)
        (model/"model.json").write_text(json.dumps({"schema":1,"name":"original storage comparison fixture","license":"Apache-2.0","dimensions":2,"sha256":hashlib.sha256(vocabulary).hexdigest()}),encoding="utf-8")
        fixture.configure("cutover",{"operation":"vectors","enabled":True})
        fixture.configure("cutover",{"operation":"install_model","directory":str(model)})
        def semantic_ready():
            deadline=time.monotonic()+600
            while True:
                readiness=fixture.tool("cutover","airk_status",{"response_mode":"full"})["semantic_readiness"]
                if readiness["selected_artifacts"]==32 and readiness["completed_artifacts"]==32:return
                if time.monotonic()>deadline:raise AssertionError(f"Semantic publication did not complete: {readiness}")
                time.sleep(.05)
        fixture.wait(fixture.enqueue("cutover","index"));semantic_ready()
        vector_arguments={"query":fixture_token(0),"mode":"vector","limit":10,"max_bytes":8192,"response_mode":"full"}
        cases["warm_vector"]=vector_arguments
        signatures={name:evidence(fixture.tool("cutover","airk_search",arguments)) for name,arguments in cases.items()}
        for repetition in range(repetitions):
            for sample in range(samples):
                begin=time.perf_counter_ns();result=fixture.tool("cutover","airk_search",vector_arguments)
                measurements["warm_vector"].append((time.perf_counter_ns()-begin)/1e6)
                if evidence(result)!=signatures["warm_vector"]:raise AssertionError("Vector evidence changed within one publication")
            corpus(source,repetitions+repetition+1)
            begin=time.perf_counter_ns();fixture.wait(fixture.enqueue("cutover","index"));semantic_ready()
            measurements["replace_32_file_semantic_publication"].append((time.perf_counter_ns()-begin)/1e6)
            signatures={name:evidence(fixture.tool("cutover","airk_search",arguments)) for name,arguments in cases.items()}
        fixture.wait(fixture.enqueue("cutover","inspect"))
        inspection=fixture.rpc({"inspection":{"project":fixture.projects["cutover"]["binding"]}})
        status=fixture.tool("cutover","airk_status",{"response_mode":"full"})
        # Shutdown/restart this fixture only, then verify exact retained evidence.
        fixture.close();fixture.log=(owner/"service-reopen.stderr").open("wb");fixture.child=None
        begin=time.perf_counter_ns();fixture.start()
        reopened={name:evidence(fixture.tool("cutover","airk_search",arguments)) for name,arguments in cases.items()}
        recovery_ms=(time.perf_counter_ns()-begin)/1e6
        if reopened!=signatures: raise AssertionError("Restart changed retained evidence")
        sampler.finish()
        return {"identity":identity(runtime),"latency":{name:distribution(values) for name,values in measurements.items()},
                "first_query_ms":cold,"last_evidence":signatures,"inspection":inspection,
                "storage_sizes":status.get("storage_sizes"),"logical_data_bytes":scale.logical_bytes(data),
                "sampled_resources":sampler.result(),
                "restart_and_evidence_ms":recovery_ms,"source_digest":scale.source_digest(source)}
    finally:
        sampler.stop.set(); sampler.thread.join()
        fixture.close()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-runtime",type=Path,required=True)
    parser.add_argument("--runtime",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--repetitions",type=int,default=5)
    parser.add_argument("--query-samples",type=int,default=8)
    args=parser.parse_args()
    args.output = args.output.resolve()
    if args.output.exists(): parser.error("Choose a fresh isolated output directory")
    if args.repetitions<3 or args.query_samples<4: parser.error("At least three repetitions and four samples are required")
    baseline,current=identity(args.baseline_runtime.resolve()),identity(args.runtime.resolve())
    if baseline["version"]!="1.4.4" or current["version"]!="1.4.5" or baseline["profile"]!=current["profile"]:
        parser.error("Use identified 1.4.4/1.4.5 pairs with matching build profiles")
    args.output.mkdir(parents=True)
    results=[]
    for name,runtime in (("baseline",args.baseline_runtime.resolve()),("current",args.runtime.resolve())):
        print(f"Measuring {name} {runtime}",flush=True)
        results.append(run_one(runtime,args.output/name,args.repetitions,args.query_samples))
    if results[0]["last_evidence"]!=results[1]["last_evidence"] or results[0]["source_digest"]!=results[1]["source_digest"]:
        raise AssertionError("Format140/150 evidence or synthetic sources differ")
    comparisons={}
    for name in results[0]["latency"]:
        before=results[0]["latency"][name]["p95_ms"];after=results[1]["latency"][name]["p95_ms"]
        comparisons[name]={"baseline_p95_ms":before,"current_p95_ms":after,"ratio":after/before,"within_10_percent":after<=before*1.1}
    gate_names=["warm_common","warm_rare","warm_prefix","replace_32_file_publication","delete_publication","warm_vector","replace_32_file_semantic_publication"]
    report={"schema":"airk-storage-cutover-benchmark/1","machine":scale.machine_environment(args.output),
        "fixture":"32 original generated Rust files,8 functions/file; lexical-only then identical original2-dimensional offline model; usage disabled",
        "cache_conditions":"Sequential versions; warm OS caches, no cache flush; first-query observations reported separately",
        "measurement_scope":"Native client process startup+direct worker roundtrip; publication enqueue through observed job completion with 50ms bounded poll",
        "repetitions":args.repetitions,"query_samples_per_repetition":args.query_samples,
        "quota_policy":COMPARISON_QUOTA,"results":results,
        "comparisons":comparisons,"gate_passed":all(comparisons[name]["within_10_percent"] for name in gate_names),
        "limits":["Identified debug-build comparison, not an isolated storage-engine speedup; receipts do not attest per-package overrides or RUSTFLAGS; baseline SHA2 override is unverified; candidate uses the incoming CI SHA2 opt-level=3 setting",
                  "No native power-loss injection","First publication has one sample/version","100ms resource sampling may miss short-lived peaks",
                  "DiskANN native/model/retained-reader and process recovery qualification are separate required checks"]}
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"gate_passed":report["gate_passed"],"comparisons":comparisons},indent=2))
    if not report["gate_passed"]:raise SystemExit(1)

if __name__=="__main__":main()
