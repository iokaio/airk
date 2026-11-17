# SPDX-License-Identifier: Apache-2.0
"""Create an additive CPU export seal only after a separate root stage release."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"support"))
from recipe_paths import private_run_root, fresh_output
import argparse,hashlib,json
ROOT=next(p for p in Path(__file__).resolve().parents if (p/"Cargo.toml").is_file() and (p/"crates/airk-core").is_dir());SOURCE_DIR=Path(__file__).resolve().parent;HERE=private_run_root(ROOT);MODELS=ROOT/"artifacts/advanced-split/models"
PARENT=MODELS/"encoder-recovery-preparation-r1/execution-seal-train-r3.json"
def sha(p):
    with Path(p).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def add_rows(rows,paths):
    output={str(Path(r["path"]).resolve()).casefold():dict(r) for r in rows}
    if len(output)!=len(rows):raise ValueError("duplicate parent pin")
    for path in paths:
        path=Path(path).resolve();row={"path":str(path),"sha256":sha(path),"bytes":path.stat().st_size};key=str(path).casefold()
        if key in output and output[key]!=row:raise ValueError("conflicting original runtime pin; no reseal")
        output[key]=row
    return list(output.values())
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--release",type=Path,required=True);p.add_argument("--release-sha256",required=True);a=p.parse_args()
    if sha(a.release)!=a.release_sha256:raise ValueError("root release changed")
    release=json.loads(a.release.read_bytes());provenance=HERE/"provenance.json"
    if release!={"action":"CPU-export-parent-bag-epoch1-U8-S8-and-independent-reference","provenance_sha256":sha(provenance),"runner_sha256":sha(SOURCE_DIR/"runner.py")}:raise ValueError("wrong separate CPU stage release")
    frozen=json.loads(provenance.read_bytes());parent=json.loads(PARENT.read_bytes())
    training=json.loads(Path(frozen["training_closure"]["path"]).read_bytes())
    if sha(Path(frozen["training_closure"]["path"]))!=frozen["training_closure"]["sha256"] or training.get("failure") is not None:raise ValueError("actual TRAIN closure changed")
    for name,pin in frozen["checkpoint"].items():
        if sha(Path(name))!=pin:raise ValueError("actual epoch1 checkpoint changed")
    files=[x for x in SOURCE_DIR.iterdir() if x.is_file()]+[provenance,Path(sys.modules["recipe_paths"].__file__)]+[Path(x) for x in frozen["checkpoint"]]+[Path(frozen[k]["path"]) for k in ["training_receipt","training_closure","recipe"]]+[a.release.resolve(),PARENT,ROOT/"target/local-vector-model/minilm-l6-v2-u8-v1/LICENSE",ROOT/"tools/retrieval-train/training_execution.py"]
    rows=add_rows(parent["files"],files)
    output=fresh_output(HERE,HERE/"cpu-export-execution")
    if output.exists():raise ValueError("fresh CPU stage directory required; no replay")
    for row in rows:
        if sha(row["path"])!=row["sha256"]:raise ValueError("runtime/source/input drift: "+row["path"])
    packet={"schema":"airk.private-parent-bag-CPU-export-seal/1","parent_runtime":{"path":str(PARENT),"sha256":sha(PARENT),"retained_pins":len(parent["files"]),"dropped_pins":0},"files":rows,
      "file_roles":parent["file_roles"],"limits":{"whole_seconds":600,"aggregate_host_bytes":8589934592,"job_processes":2,"CPU_only":True},"release":{"path":str(a.release.resolve()),"sha256":a.release_sha256},"provenance_sha256":sha(provenance),"output":str(output)}
    output.mkdir();target=output/"seal.json";target.write_text(json.dumps(packet,indent=2)+"\n");print(json.dumps({"seal":str(target),"sha256":sha(target),"pins":len(rows),"executed":False}))
if __name__=="__main__":main()
