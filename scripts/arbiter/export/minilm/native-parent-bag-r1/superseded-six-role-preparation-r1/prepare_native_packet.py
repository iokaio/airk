# SPDX-License-Identifier: Apache-2.0
"""Bind only actual new Rust compiler/artifacts and successful CPU outputs; no native execution."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"support"))
from recipe_paths import private_run_root, fresh_output
import argparse,hashlib,json
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());SOURCE_DIR=Path(__file__).resolve().parent;HERE=private_run_root(ROOT);MODELS=ROOT/"artifacts/advanced-split/models"
SOURCE="crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs"
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--artifact-lock",type=Path,required=True);p.add_argument("--compile-receipt",type=Path,required=True);p.add_argument("--output",type=Path,required=True);a=p.parse_args()
    a.output=fresh_output(HERE,a.output)
    lock=json.loads(a.artifact_lock.read_bytes());compile=json.loads(a.compile_receipt.read_bytes());source=ROOT/SOURCE
    if compile["exit_code"]!=0 or compile["groups"]["native"]["files"].get(SOURCE)!=sha(source):raise ValueError("actual compiler must consume exact reviewed new sibling")
    if SOURCE not in Path(lock["kernel_depinfo"]["path"]).read_text().replace("\\","/"):raise ValueError("new sibling absent from actual depinfo")
    artifacts=[lock["binary"],lock["kernel_depinfo"],lock["retrieval_depinfo"],*lock["artifacts"]]
    pins={str(Path(sys.modules["recipe_paths"].__file__)):sha(Path(sys.modules["recipe_paths"].__file__)),str(a.artifact_lock.resolve()):sha(a.artifact_lock),str(a.compile_receipt.resolve()):sha(a.compile_receipt),str(source):sha(source)}
    for artifact in artifacts:
        if sha(artifact["path"])!=artifact["sha256"]:raise ValueError("retained artifact changed")
        pins[artifact["path"]]=artifact["sha256"]
    for item in [SOURCE_DIR/"run_native.py",ROOT/"tools/retrieval-train/training_execution.py",HERE/"provenance.json",HERE/"cpu-export-execution/seal.json",HERE/"cpu-export-execution/execution.json",MODELS/"native-parent-bag-native-inputs-r1/receipt.json",MODELS/"native-parent-bag-eager-reference-r1/receipt.json",MODELS/"native-parent-bag-exports-r1/receipt.json",MODELS/"native-parent-bag-exports-r1/stage-receipt.json",MODELS/"a1-component-development-materialized-r1/corpus.private.json",MODELS/"a1-component-dev-evaluation-protocol-r1/quality-mask.json",MODELS/"a1-component-dev-native-fixture-r1/manifest.private.json"]:pins[str(item)]=sha(item)
    cpu=json.loads((HERE/"cpu-export-execution/execution.json").read_bytes())
    if cpu.get("complete") is not True or cpu.get("post_input_drift"):raise ValueError("actual complete natural CPU export required")
    exported=json.loads((MODELS/"native-parent-bag-exports-r1/receipt.json").read_bytes())
    stage=json.loads((MODELS/"native-parent-bag-exports-r1/stage-receipt.json").read_bytes())
    if stage.get("execution_seal_sha256")!=cpu.get("seal_sha256") or stage.get("export_receipt_sha256")!=sha(MODELS/"native-parent-bag-exports-r1/receipt.json") or stage.get("oracle_receipt_sha256")!=sha(MODELS/"native-parent-bag-native-inputs-r1/receipt.json") or stage.get("eager_receipt_sha256")!=sha(MODELS/"native-parent-bag-eager-reference-r1/receipt.json"):raise ValueError("actual CPU export/oracle/eager relationship changed")
    for profile in exported["profiles"]:
        short=profile["precision"][-2:]
        for asset in profile["assets"]:
            path=MODELS/"native-parent-bag-exports-r1"/short/asset["name"]
            if sha(path)!=asset["sha256"]:raise ValueError("exported asset changed")
            pins[str(path)]=asset["sha256"]
    output=fresh_output(HERE,HERE/"native")
    closed=MODELS/"native-encoder-qualification-input-r1.json";pins[str(closed)]=sha(closed)
    if pins[str(closed)]!="3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81":raise ValueError("original closed baseline descriptors changed")
    baseline_rows=json.loads(closed.read_bytes())
    for row in baseline_rows:
        if not row["family"].startswith("original-"):continue
        manifest=Path(row["directory"])/"model.json"
        if sha(manifest)!=row["model_json_sha256"]:raise ValueError("original baseline manifest changed")
        pins[str(manifest)]=row["model_json_sha256"];metadata=json.loads(manifest.read_bytes())
        for name,key in [("model.onnx","model_sha256"),("tokenizer.json","tokenizer_sha256"),("LICENSE","license_sha256")]:pins[str(Path(row["directory"])/name)]=metadata[key]
    packet={"schema":"airk.private-parent-bag-native-preparation/1","binary":lock["binary"],"compile_receipt":str(a.compile_receipt.resolve()),"pins":pins,"output":str(output),"master":str(output/"master.json"),"limits":{"seconds":1800,"host_bytes":8589934592,"max_job_processes":2},"families":["native-parent-bag-r1-u8","native-parent-bag-r1-s8"],"scope":{"original_DEV_requests":64,"quality_denominator":20,"alignment_refusals":44,"FINAL":False,"promotion":False,"training":False},"closed_controls":str(closed),"requires_root_stage_release":True}
    a.output.write_text(json.dumps(packet,indent=2)+"\n");print(json.dumps({"packet":str(a.output),"sha256":sha(a.output),"executed":False}))
if __name__=="__main__":main()
