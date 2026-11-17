# SPDX-License-Identifier: Apache-2.0
"""Actual existing large-asset SHA under original60s/256MiB; no grading or NN."""
from pathlib import Path
import hashlib,json,os,threading,time,types
import grade_dev as owner

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def main():
    output=HERE/"bounded-probe-r1.json"
    if output.exists():raise ValueError("fresh bounded probe required")
    source=ROOT/"artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r5/audit.py"
    raw=source.read_bytes()
    if hashlib.sha256(raw).hexdigest()!="9c540c08c2e56d868059ab8425193e52f0d9141f75d16df01f622c679032425b":raise ValueError("unchanged original budget owner required")
    budget=types.ModuleType("_bounded_grader_probe");budget.__file__=str(source);exec(compile(raw,str(source),"exec"),budget.__dict__)
    budget.START=time.monotonic();budget.JOB_STATE=budget.enforce_budget()
    def expire():
        if not budget.done.wait(60):os._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    packet=ROOT/"artifacts/advanced-split/models/native-parent-bag-native-r95-preparation-r1/native-packet-r95-r1-proposed.json"
    pins=json.loads(packet.read_bytes())["pins"]
    candidates=[name for name in pins if Path(name).name=="onnxruntime.lib"]
    if len(candidates)!=1:raise ValueError("one actual existing pinned static runtime asset required")
    asset=Path(candidates[0]);size=asset.stat().st_size
    if size<=budget.LIMIT:raise ValueError("probe must exceed unchanged total memory allowance")
    observed_whole_read_refusal=False
    try:asset.read_bytes()
    except MemoryError:observed_whole_read_refusal=True
    if not observed_whole_read_refusal:raise ValueError("original whole-read failure not reproduced under hard budget")
    actual=owner.sha(asset)
    if actual!=pins[str(asset)]:raise ValueError("streaming digest differs from original actual asset pin")
    result={"actual_asset":str(asset),"asset_bytes":size,"sha256":actual,"old_whole_read_MemoryError":True,
        "new_streaming_digest_matches":True,"hard_seconds":60,"hard_bytes":budget.LIMIT,
        "peak_committed_process_bytes":budget.peak_memory(),"elapsed_seconds":time.monotonic()-budget.START,
        "grade_or_NN_executed":False,"frozen_sources_changed":False}
    output.write_text(json.dumps(result,indent=2)+"\n");budget.done.set();print(json.dumps(result))

if __name__=="__main__":main()
