# SPDX-License-Identifier: Apache-2.0
"""Re-evaluate the retained 16 GiB original whose parent misclassified its unprofiled stdout."""
import hashlib
import json
from pathlib import Path
import runpy
import sys
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from capacity_ballast import CAMPAIGN, natural
from capacity_ballast_worker import GIB, require
from capacity_sweep import candidate_samples, receipt
from protected_capacity_sweep import GRAPH, MAXIMUM_PROOF, PROBE, PressureTrial, verify_host_grant


def read_pin(path, expected, _base=None):
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest()==expected, f"changed {path.name}")
    return {"path":str(path),"sha256":expected,"bytes":len(raw)}


def read_json(path):
    return json.loads(Path(path).read_bytes())


def main():
    source = CAMPAIGN/"compact-capacity-protected-16-gib-pressure-fit-r2"/"execution.json"
    outer_hash = "140ec1cc6a99838c2af21a3940c2dfa5f671fb62fff9c28009833a26189b42a6"
    native_hash = "a8931ef78662c4ac00e3d97ffe59e68242e4053a5345dd10826f05b017cbdaae"
    outer = receipt(source,outer_hash)
    native_path = CAMPAIGN/(source.parent.name+"-native")/"execution.json"
    n = receipt(native_path,native_hash)
    require(outer["complete"] and natural(outer["job"]) and natural(n["owned"]) and outer["all_workers_joined"]
            and outer["trial"]["native_execution"]["sha256"]==native_hash
            and not n["complete"] and n["error"]=="ValueError: all 28 measured FP32 memory-efficient dispatches required",
            "original mismatch or incomplete settlement")
    data = native_path.parent
    def reopen(key, name):
        evidence = n[key]
        path = data/name
        require(Path(evidence["path"])==path,"wrong native output")
        read_pin(path,evidence["sha256"])
        return path.read_bytes()
    grant = json.loads(reopen("pressure_grant","temp/pressure-admission.json"))
    verify_host_grant(json.loads(reopen("pressure_host_grant","temp/pressure-host-admission.json")),n,12*GIB)
    raw = reopen("original_stdout","stdout-stderr.log")
    probe = runpy.run_path(str(PROBE))
    result,_,_ = probe["model_output"](raw,"pressure-fit",{"require":require})
    require(result["typed_graph_sha256"]==GRAPH and grant["candidate_pid"]==n["owned"]["owned_leader_pid"] and
            grant["available_bytes"]==outer["admission"]["bytes"] and grant["tier_gib"]==outer["tier_gib"]==16,
            "original device grant mismatch")
    check = PressureTrial(16,None)
    check.payload = grant
    check.verify_native(result["pressure"],n["pressure_grant"]["sha256"])
    timing = runpy.run_path(str(PROBE.with_name("compact_cutlass_timing.py")))
    expected,controls = timing["inputs"](CAMPAIGN,GRAPH,MAXIMUM_PROOF,SimpleNamespace(read=read_json),read_pin)
    rows = result["timing"]["rows"]
    summary = timing["summarize"]("pressure-fit",rows,expected)
    expiry = grant["expires_monotonic_ns"]
    env = {"AIRK_COMPACT_CUTLASS_EXPIRES_NS":str(expiry),"AIRK_COMPACT_CUTLASS_TYPED_GRAPH":GRAPH,
           "AIRK_COMPACT_CUTLASS_EXPECTED_WORDS":json.dumps(expected,separators=(",",":"))}
    events_raw = (data/"temp/protected.operations.jsonl").read_bytes()
    events = [json.loads(line) for line in events_raw.splitlines()]
    timing["confirm_journal"](events,rows,expiry,env,n["image"]["sha256"])
    require(events[0]["identity"]["process_id"]==grant["candidate_pid"],"wrong original journal PID")
    memory = candidate_samples(n,outer["admission"],limit_field="observed_limits",host_limit=12*GIB,require_shared_clock=True)
    require(memory==outer["trial"]["candidate_wddm"] and not memory["verified"] and memory["peak_shared_bytes"]>0,
            "original observer or positive shared usage differs")
    inside = [s for s in n["owned"]["before_GO"]["wddm"]["samples"]
              if s["end_monotonic_ns"] >= rows[0]["start_us"]*1000 and s["start_monotonic_ns"] <= rows[0]["finish_us"]*1000]
    require(inside and all(type(s["segment_bytes"]["Local Usage"]) is int and s["segment_bytes"]["Local Usage"]>0
                           and type(s["segment_bytes"]["Non Local Usage"]) is int and s["segment_bytes"]["Non Local Usage"]>0
                           for s in inside), "missing original forward segment observations")
    return {"original_outer_sha256":outer_hash,"original_native_sha256":native_hash,"native_receipt_complete":False,
            "original_stdout_sha256":n["original_stdout"]["sha256"],"reopened_journal_sha256":hashlib.sha256(events_raw).hexdigest(),
            "reopened_controls":controls,"model_output_parser_sha256":hashlib.sha256(PROBE.read_bytes()).hexdigest(),
            "numerical_words_exact":True,"capacity_admitted":False,"outcome":"observed_shared_usage",
            "timing":summary,"counts":memory,"in_forward_segment_bytes":[s["segment_bytes"] for s in inside],
            "grant":result["pressure"],"native_call_us":rows[0]["native_call_us"],
            "host_limit_bytes":12*GIB,"peak_host_committed_bytes":n["owned"]["peak_host_committed_bytes"]}


if __name__=="__main__":
    print(json.dumps(main(),indent=2))
