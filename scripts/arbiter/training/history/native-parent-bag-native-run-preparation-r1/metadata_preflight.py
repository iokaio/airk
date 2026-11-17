# SPDX-License-Identifier: Apache-2.0
"""Read-only actual64/20/44/366 identity and frozen113-source checks; no model runtime."""
from pathlib import Path
import argparse,hashlib,json,stat
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=HERE.parent
FROZEN=MODELS/"native-parent-bag-export-source-r1"
def sha(path):
    path=Path(path);before=path.stat().st_size;remaining=before+1;hash=hashlib.sha256();total=0
    with path.open("rb") as stream:
        while remaining:
            block=stream.read(min(4<<20,remaining))
            if not block:break
            total+=len(block);remaining-=len(block);hash.update(block)
    if total!=before or path.stat().st_size!=before:raise ValueError("file changed during bounded preparation read")
    return hash.hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    if args.output.exists() or args.output.parent.resolve()!=HERE:raise ValueError("fresh private metadata receipt required")
    seal=json.loads((MODELS/"native-parent-bag-export-execution-r1/seal.json").read_bytes())
    exports=[row for row in seal["files"] if Path(row["path"]).parent==FROZEN]
    training=[row for row in seal["files"] if Path(row["path"]).is_relative_to(ROOT/"tools/retrieval-train")]
    if len(exports)!=22 or len(training)!=91:raise ValueError("unchanged22+91 source populations required")
    for row in exports+training:
        if sha(row["path"])!=row["sha256"]:raise ValueError("frozen source drift: "+row["path"])
    fixture=MODELS/"a1-component-dev-native-fixture-r1"
    corpus=MODELS/"a1-component-development-materialized-r1/corpus.private.json"
    mask=MODELS/"a1-component-dev-evaluation-protocol-r1/quality-mask.json"
    manifest=fixture/"manifest.private.json"
    tasks=json.loads(corpus.read_bytes())["tasks"];quality=json.loads(mask.read_bytes());sources=json.loads(manifest.read_bytes())
    ids=[task["id"] for task in tasks];admitted={"hotpot:"+value for value in quality["quality_question_ids"]};refused={"hotpot:"+value for value in quality["refused_question_ids"]}
    if len(ids)!=64 or len(set(ids))!=64 or len(admitted)!=20 or len(refused)!=44 or admitted&refused or set(ids)!=admitted|refused:
        raise ValueError("actual fixed64 DEV identities and original20/44 mask required")
    if any(task["split"]!="development" for task in tasks):raise ValueError("non-DEV task refused")
    if sources["corpus_sha256"]!=sha(corpus) or sources["quality_mask_sha256"]!=sha(mask):raise ValueError("source manifest population binding differs")
    rows=sources["files"];expected={row["file"] for row in rows}
    if len(rows)!=366 or len(expected)!=366:raise ValueError("actual closed366 source roster required")
    actual=set()
    for path in (fixture/"sources").rglob("*"):
        metadata=path.lstat()
        if path.is_symlink() or getattr(metadata,"st_file_attributes",0)&0x400:raise ValueError("source alias refused")
        if stat.S_ISREG(metadata.st_mode):actual.add(path.relative_to(fixture).as_posix())
        elif not stat.S_ISDIR(metadata.st_mode):raise ValueError("source special file refused")
    if actual!=expected:raise ValueError("source files added or omitted")
    for row in rows:
        path=fixture/row["file"]
        if path.stat().st_size!=row["bytes"] or sha(path)!=row["sha256"]:raise ValueError("actual supplied source bytes differ")
    cpu=MODELS/"native-parent-bag-export-execution-r1/execution.json";execution=json.loads(cpu.read_bytes())
    if execution.get("complete") is not True or execution.get("post_input_drift") or execution["owned"].get("exit_code")!=0:
        raise ValueError("complete unchanged actual CPU export required")
    result={"schema":"airk.private-parent-bag-next-native-metadata/1","actual_DEV_requests":64,"quality_denominator":20,
        "alignment_refusals":44,"actual_source_roster":366,"frozen_export_sources_verified":22,"frozen_public_training_sources_verified":91,
        "corpus_sha256":sha(corpus),"mask_sha256":sha(mask),"manifest_sha256":sha(manifest),"actual_CPU_execution_sha256":sha(cpu),
        "public_native_child_sha256":sha(ROOT/"crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs"),
        "Cargo_executed":False,"native_or_NN_executed":False,"FINAL_read":False,"promotion":False,
        "remaining":"private-base amendment review/mount, fresh actual Cargo JSON identity, Rust emitter,2x12 native numeric,4same-image DEV64 captures, DEV grading and independent later acceptance"}
    args.output.write_text(json.dumps(result,indent=2)+"\n");print(json.dumps(result))
if __name__=="__main__":main()
