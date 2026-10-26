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
import time

ROOT = Path(__file__).resolve().parents[1]

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
            "source": build["source"], "executables": {name: {"sha256": digest(runtime/name),
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

def corpus(source, revision):
    for number in range(32):
        directory=source/("selected" if number<8 else "other")
        directory.mkdir(exist_ok=True)
        (directory/f"fixture_{number:02}.rs").write_text(
            f"// CommonNeedle original synthetic source revision {revision}\n"+
            "\n".join(f"pub fn rare_{number:02}_{item}(value: usize) -> usize {{ value + {item+revision} }}" for item in range(8))+"\n",
            encoding="utf-8")

def run_one(runtime, output, repetitions, samples):
    output.mkdir()
    source=output/"source";data=output/"data";owner=output/"owner"
    for directory in (source,data,owner): directory.mkdir()
    corpus(source,0)
    fixture=clients.Fixture(runtime,owner,job_timeout=600)
    measurements={name:[] for name in ("initial_32_file_publication","warm_common","warm_rare","warm_prefix","replace_32_file_publication","delete_publication","warm_vector","replace_32_file_semantic_publication")}
    signatures={}
    try:
        fixture.start();fixture.register("cutover",source,data)
        fixture.rpc({"configure_watch":{"project":fixture.projects["cutover"]["binding"],"enabled":False}})
        fixture.configure("cutover",{"operation":"vectors","enabled":False})
        fixture.configure("cutover",{"operation":"usage","mode":"disabled"})
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
        vocabulary=json.dumps({"commonneedle":[1.0,0.0],"original":[0.0,1.0],"source":[.7,.7],"rare":[.5,-.5]}).encode()
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
        vector_arguments={"query":"CommonNeedle original","mode":"vector","limit":10,"max_bytes":8192,"response_mode":"full"}
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
        broker=scale.process_metrics(fixture.child,fixture.binary)
        # Shutdown/restart this fixture only, then verify exact retained evidence.
        fixture.close();fixture.log=(owner/"service-reopen.stderr").open("wb");fixture.child=None
        begin=time.perf_counter_ns();fixture.start()
        reopened={name:evidence(fixture.tool("cutover","airk_search",arguments)) for name,arguments in cases.items()}
        recovery_ms=(time.perf_counter_ns()-begin)/1e6
        if reopened!=signatures: raise AssertionError("Restart changed retained evidence")
        return {"identity":identity(runtime),"latency":{name:distribution(values) for name,values in measurements.items()},
                "first_query_ms":cold,"last_evidence":signatures,"inspection":inspection,
                "storage_sizes":status.get("storage_sizes"),"logical_data_bytes":scale.logical_bytes(data),
                "broker_process_counters":broker,"counter_scope":"broker only; project/helper RSS is not sampled",
                "restart_and_evidence_ms":recovery_ms,"source_digest":scale.source_digest(source)}
    finally: fixture.close()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-runtime",type=Path,required=True)
    parser.add_argument("--runtime",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--repetitions",type=int,default=5)
    parser.add_argument("--query-samples",type=int,default=8)
    args=parser.parse_args()
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
        "repetitions":args.repetitions,"query_samples_per_repetition":args.query_samples,"results":results,
        "comparisons":comparisons,"gate_passed":all(comparisons[name]["within_10_percent"] for name in gate_names),
        "limits":["No native power-loss injection","First publication has one sample/version","Broker RSS excludes project/helper memory",
                  "DiskANN native/model/retained-reader and process recovery qualification are separate required checks"]}
    (args.output/"report.json").write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"gate_passed":report["gate_passed"],"comparisons":comparisons},indent=2))
    if not report["gate_passed"]:raise SystemExit(1)

if __name__=="__main__":main()
