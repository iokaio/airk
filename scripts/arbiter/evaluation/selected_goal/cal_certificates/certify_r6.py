# SPDX-License-Identifier: Apache-2.0
"""Issue gains only from an authenticated, whole original-clock CAL capture."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from certify import certificate
from reference_inputs import digest, load_inputs, read, reference, require
from r6_epoch import ORIGINAL, ORIGINAL_SHA, ROOT, R7_ROOT, preflight
from r6_phased_closure import phased_events


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit",type=Path,required=True)
    parser.add_argument("--audit-sha256",required=True)
    parser.add_argument("--service-sha256",required=True)
    parser.add_argument("--plan-sha256",required=True)
    parser.add_argument("--outer-sha256",required=True)
    parser.add_argument("--closure-sha256",required=True)
    parser.add_argument("--release-sha256",required=True)
    parser.add_argument("--resume-r6",action="store_true",help="verify the R7 successor capture")
    a=parser.parse_args()
    require(sys.flags.isolated and sys.dont_write_bytecode and sys.flags.no_site,"checked isolated Python required")
    root = R7_ROOT if a.resume_r6 else ROOT
    plan_pin=reference(root/"lifetime-plan.json",a.plan_sha256)
    context,audit=preflight(root,plan_pin,reference(a.audit,a.audit_sha256),a.service_sha256)
    focus=audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
    _,ids,tasks,ledger,utility,policy,sources,index_root=load_inputs(reference(ORIGINAL,ORIGINAL_SHA),focus)
    pairs=phased_events(root,a.outer_sha256,a.closure_sha256,audit,a.audit_sha256,ids,sources,index_root,
                             tasks,release_sha=a.release_sha256,r6=context)
    rows=[certificate(e,tasks[e["id"]],audit,window,policy,ledger,utility) for e,window in pairs]
    require([r["id"] for r in rows]==ids,"unclosed original CAL population")
    closure=read(reference(root/"fixture-closure.json",a.closure_sha256))
    refs=audit["inputs"]
    files=("certify_r6.py","r6_epoch.py","r6_phased_closure.py","r6_group.py","r6_questions.py","r6_effective_sources.py","certify.py","phased_closure.py",
           "grouped_closure.py","capture_closure.py","reference_inputs.py")
    if a.resume_r6:
        files += ("r7_restart.py",)
    report={"schema":"airk.private-selected-Context-CAL-reference-preflight/1","complete":True,"qualified_profiles":[],
            "Focus_configuration":"ON","Focus_profile":focus,"MiniLM_pipeline":audit["pipeline"],
            "corpus_sha256":refs["corpus"]["sha256"],"source_manifest_sha256":refs["manifest"]["sha256"],
            "rubric_sha256":refs["rubric"]["sha256"],"task_policy":policy,"human_semantic_gold_count":0,
            "capture_sha256":a.closure_sha256,"outer_owned_sha256":a.outer_sha256,"lifetime_plan_sha256":a.plan_sha256,
            "lifetime_closure_sha256":[p["sha256"] for p in closure["lifetimes"]],"mutex_release_sha256":a.release_sha256,
            "launch_audit_sha256":a.audit_sha256,"focus_tokens":1024,"service_sha256":a.service_sha256,
            "source_sha256":{name:digest((HERE/name).read_bytes()) for name in files},
            "native_capture_status_counts":Counter(r["native_capture_status"] for r in rows),
            "scope":"Original human support gain from additional verified reads only. Terminal labels unavailable.","rows":rows}
    print(json.dumps(report,ensure_ascii=True,separators=(",",":"),allow_nan=False))


if __name__=="__main__":
    main()
