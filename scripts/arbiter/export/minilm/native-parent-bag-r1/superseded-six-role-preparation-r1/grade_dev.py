# SPDX-License-Identifier: Apache-2.0
"""Same-image six-role DEV-only grading via unchanged evidence owner; no NN/FINAL."""
from pathlib import Path
from fractions import Fraction
import argparse,hashlib,json,os,threading,time,types
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"
EXPECTED={"candidate-u8":"native-parent-bag-r1-u8","candidate-s8":"native-parent-bag-r1-s8","control-shipped-u8":"original-shipped-u8-ort130-evaluation","control-mixed-u8":"original-mixed-u8-ort130-evaluation","diagnostic-shipped-s8":"original-shipped-s8-ort130-evaluation","diagnostic-mixed-s8":"original-mixed-s8-ort130-evaluation"}
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def require(value,message):
    if not value:raise ValueError(message)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--packet",type=Path,required=True);p.add_argument("--packet-sha256",required=True);p.add_argument("--output",type=Path,required=True);a=p.parse_args()
    require(all(os.environ.get(k)=="1" for k in ["OMP_NUM_THREADS","MKL_NUM_THREADS","OPENBLAS_NUM_THREADS"]),"original singlethread grading environment required")
    require(sha(a.packet)==a.packet_sha256,"reviewed native packet changed");packet=json.loads(a.packet.read_bytes());pins=dict(packet["pins"]);pins[str(a.packet.resolve())]=a.packet_sha256;pins[str(Path(__file__).resolve())]=sha(__file__)
    helper_path=ROOT/"scripts/arbiter/export/minilm/visible-prefix-r1/grade_candidates.py";helper_pin="9253fdb047c16104379e60a02129a95ca7afa14ca89056132cbf595bb37aed6b"
    require(sha(helper_path)==helper_pin,"retained grader source changed");helper=types.ModuleType("_parent_bag_grade_helpers");helper.__file__=str(helper_path);exec(compile(helper_path.read_bytes(),str(helper_path),"exec"),helper.__dict__)
    tools=ROOT/"tools/retrieval-train";clock_path=ROOT/"scripts/arbiter/export/minilm/support/blind-metadata-r5/audit.py"
    sources={tools/"development_retrieval.py":"8ede0bd19ae3ff96ed534d662832eca32a8bde7408aea86051e542ce1cd34ad0",tools/"goal_sources.py":"603f4c64357dd4dfa1d51d1556c19ac17977d4b5ed0fcaa1d2aa44bca43274c7",tools/"goal_rubric.py":"3fae6cdf245d9fa0b18f1c4d63e29c135cdfbc873543f08a72f611222022544d",tools/"grade_native_retrieval.py":"34995d4431819e910a96b177625fe05c572a79072287ccc21322635fd0a1c0e1",clock_path:"a1687757532d766baac52f9c78547f30c1d3b340f9370cfe961d7817b237ad77"}
    modules=[]
    for name,path in [("_diagnostic_bounded",clock_path),("goal_rubric",tools/"goal_rubric.py"),("goal_sources",tools/"goal_sources.py"),("grade_native_retrieval",tools/"grade_native_retrieval.py"),("development_retrieval",tools/"development_retrieval.py")]:
        path,raw=helper.captured(path,sources[path]);modules.append((name,path,raw))
    pins.update({str(path):pin for path,pin in sources.items()});pins[str(helper_path)]=helper_pin
    fixture=MODELS/"a1-component-dev-native-fixture-r1";manifest=fixture/"manifest.private.json";rubric=fixture/"rubric.private.json";mask=MODELS/"a1-component-dev-evaluation-protocol-r1/quality-mask.json";corpus=MODELS/"a1-component-development-materialized-r1/corpus.private.json"
    pins[str(rubric)]="587bb503da85119b9f0f7fbb544a0216d655ac07691a3c6319a2a9b9fc1e3b2a"
    captures=[]
    for role,family in EXPECTED.items():
        execution=Path(packet["output"])/("execution-"+family)/"execution.json";capture=execution.with_name("capture.json")
        observed=json.loads(execution.read_bytes());require(observed.get("complete") is True and observed.get("packet_sha256")==a.packet_sha256,"all six actual same-image native stages required")
        require(observed.get("capture_sha256")==sha(capture),"native raw capture changed");pins[str(execution)]=sha(execution);pins[str(capture)]=sha(capture);captures.append((role,family,capture))
    for name,pin in pins.items():require(sha(name)==pin,"frozen grading input drift: "+name)
    require(not a.output.exists() and a.output.resolve().is_relative_to(MODELS),"fresh private grade output required")
    for frozen in [fixture,HERE,Path(packet["output"])]:require(not a.output.resolve().is_relative_to(frozen.resolve()) and not frozen.resolve().is_relative_to(a.output.resolve()),"grading output overlaps frozen inputs")
    with helper.verified_modules(modules) as loaded:
        owner=loaded["development_retrieval"];clock=loaded["_diagnostic_bounded"];clock.START=time.monotonic();clock.JOB_STATE=clock.enforce_budget()
        def expire():
            if not clock.done.wait(60):os._exit(124)
        threading.Thread(target=expire,daemon=True).start();metadata=json.loads(manifest.read_bytes());owner.verify_fixture(fixture,metadata);cache=loaded["goal_sources"].FrozenSourceMetadata(fixture,pins[str(manifest)])
        tasks=json.loads(rubric.read_bytes())["tasks"];frozen_mask=json.loads(mask.read_bytes());order=[t["id"] for t in json.loads(corpus.read_bytes())["tasks"]];results=[]
        for role,family,capture in captures:
            raw=json.loads(capture.read_bytes());require(raw["family"]==family and raw["binary_sha256"]==packet["binary"]["sha256"],"matched actual native identity differs")
            require([r["id"] for r in raw["records"]]==order and raw["quality_denominator"]==20 and raw["alignment_refusals"]==44,"original64/20/44 population differs")
            result=owner.grade_family(raw,tasks,frozen_mask,fixture,pins[str(manifest)],cache);result["role"]=role;results.append(result)
        comparisons=[]
        for candidate in [r for r in results if r["role"].startswith("candidate-")]:
            for control in [r for r in results if r["role"].startswith("control-")]:
                x,y=candidate["summary"],control["summary"];gain=Fraction(x["complete_count"]-y["complete_count"],20);delta=Fraction(*x["mean_evidence_recall_fraction"])-Fraction(*y["mean_evidence_recall_fraction"])
                comparisons.append({"candidate":candidate["family"],"control":control["family"],"complete_gain_fraction":[gain.numerator,gain.denominator],"mean_delta_fraction":[delta.numerator,delta.denominator],"descriptive_DEV_paired_reference_met":gain>=Fraction(3,100) and delta>=Fraction(-2,100),"FINAL_or_promotion_pass":False})
        owner.verify_fixture(fixture,metadata)
        for name,pin in pins.items():require(sha(name)==pin,"post-grading input drift")
        a.output.mkdir();clock.atomic_stream(a.output/"grading.private.json",{"schema":"airk.private-parent-bag-matched-DEV-grading/1","families":results,"paired_comparisons":comparisons,"source_inputs":pins,"FINAL_or_promotion_pass":False,"scope":"all64 DEV/20quality44refused; actual same-image sixroles; weakparent99rows population confound retained; no checkpoint search"})
        receipt={"elapsed_seconds":time.monotonic()-clock.START,"peak_committed_process_bytes":clock.peak_memory(),"hard_seconds":60,"hard_bytes":268435456,"singlethread_BLAS":True,"native_or_model_execution":False};clock.atomic_stream(a.output/"operation-receipt.json",receipt);clock.done.set();print(json.dumps({"summaries":[dict(role=r["role"],**r["summary"]) for r in results],"paired_comparisons":comparisons,"operation":receipt}))
if __name__=="__main__":main()
