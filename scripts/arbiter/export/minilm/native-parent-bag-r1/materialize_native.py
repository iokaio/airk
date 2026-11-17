# SPDX-License-Identifier: Apache-2.0
"""Prepare exact Rust test source only after actual CPU export/reference success."""
from pathlib import Path
import argparse,hashlib,json,re
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());HERE=Path(__file__).resolve().parent;MODELS=ROOT/"artifacts/advanced-split/models"
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--execution-sha256",required=True);p.add_argument("--output",type=Path,required=True);a=p.parse_args()
    execution=MODELS/"native-parent-bag-export-execution-r1/execution.json"
    if sha(execution)!=a.execution_sha256:raise ValueError("actual CPU execution changed")
    actual=json.loads(execution.read_bytes());owned=actual["owned"]
    if not actual.get("complete") or actual.get("post_input_drift") or owned.get("exit_code")!=0 or owned.get("owned_job_final_active_processes")!=0:raise ValueError("complete natural CPU stage required")
    exports=MODELS/"native-parent-bag-exports-r1";oracle=MODELS/"native-parent-bag-native-inputs-r1/receipt.json";eager=MODELS/"native-parent-bag-eager-reference-r1/receipt.json"
    source=json.loads((exports/"receipt.json").read_bytes());numeric=json.loads(oracle.read_bytes());reference=json.loads(eager.read_bytes())
    if source["source_model_sha256"]!="e4d2c66590e830cc9fedde5061e86a5195891db490a7eb967e3ec9c52c566d29" or source["training_receipt_sha256"]!="64fa619f43a7a2eb9f9ee1d634c241107ec9ecf79b300fbe93e05f587429d237":raise ValueError("exact new TRAIN provenance required")
    if len(numeric["cases"])!=12 or {c["bucket"] for c in numeric["cases"]}!={32,64,128,256} or len(reference["cases"])!=12 or reference["maximum_fp32_onnx_component_error"]>1e-5:raise ValueError("unchanged12case independent eager/ONNX oracle required")
    for c in numeric["cases"]:
        if set(c["results"])!={"mixed-u8","mixed-s8"} or any(not v["finite"] or v["cosine"]<.995 or v["l2"]>.1 for v in c["results"].values()):raise ValueError("candidate numeric floor failed; no materialization")
    substitutions={"ORACLE_SHA256":sha(oracle),"EXPORT_RECEIPT_SHA256":sha(exports/"receipt.json"),"EXECUTION_SEAL_SHA256":actual["seal_sha256"],"U8_ONNX_SHA256":sha(exports/"u8/model.onnx"),"S8_ONNX_SHA256":sha(exports/"s8/model.onnx")}
    text=(HERE/"native_parent_bag.rs.in").read_text()
    for name,pin in substitutions.items():
        token="@"+name+"@"
        if text.count(token)!=1:raise ValueError("nonunique Rust pin placeholder")
        text=text.replace(token,pin)
    if re.search(r"@[A-Z0-9_]+@",text) or a.output.exists():raise ValueError("unresolved or nonfresh private Rust source")
    if not a.output.resolve().is_relative_to(MODELS):raise ValueError("private preparation output only; public mount needs root review")
    a.output.write_text(text);print(json.dumps({"source":str(a.output),"sha256":sha(a.output),"actual_export_substitutions":substitutions,"mounted":False}))
if __name__=="__main__":main()
