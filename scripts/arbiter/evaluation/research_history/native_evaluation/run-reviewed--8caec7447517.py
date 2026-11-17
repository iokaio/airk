# SPDX-License-Identifier: Apache-2.0
"""Root reviewed metadata-only launch; no task corpus opened."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import types
HERE=Path(__file__).resolve().parent

def sha(p):
    with Path(p).open("rb")as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol-sha256",required=True)
    parser.add_argument("--root-decision",type=Path,required=True)
    parser.add_argument("--root-decision-sha256",required=True)
    parser.add_argument("--execute-reviewed-metadata-only",action="store_true",required=True)
    args=parser.parse_args();protocol=HERE/"protocol.proposed.json";receipt=HERE/"execution-metadata-r1.json"
    if sha(protocol)!=args.protocol_sha256 or sha(args.root_decision)!=args.root_decision_sha256 or receipt.exists():raise ValueError("frozen fresh reviewed launch required")
    p=json.loads(protocol.read_bytes());seal=json.loads(Path(p["runtime_seal"]["path"]).read_bytes())
    if sha(p["runtime_seal"]["path"])!=p["runtime_seal"]["sha256"]:raise ValueError("runtime seal drift")
    rows=list(p["inputs"].values())+seal["files"]
    before={row["path"]:sha(row["path"]) for row in rows}
    if any(before[row["path"]]!=row["sha256"]for row in rows):raise ValueError("source/runtime/input drift")
    raw=Path(p["inputs"]["job_helper"]["path"]).read_bytes()
    helper=types.ModuleType("reviewed_metadata_job_owner");helper.__file__=p["inputs"]["job_helper"]["path"]
    exec(compile(raw,helper.__file__,"exec"),helper.__dict__)
    argv=[seal["runtime_executable"],"-I",str(HERE/"admission.py"),"--protocol",str(protocol),"--protocol-sha256",args.protocol_sha256,"--root-decision",str(args.root_decision),"--execute-reviewed-metadata-only"]
    result=helper.run_owned(argv,Path.cwd(),HERE/"execution-metadata-r1.log",60)
    unchanged=all(sha(k)==v for k,v in before.items()) and sha(protocol)==args.protocol_sha256 and sha(args.root_decision)==args.root_decision_sha256
    settled=helper.successful(result) and result.get("owned_job_forced_termination") is False
    output=Path(p["output_anchor"])/"materialized/opaque-reservation.json"
    report={"schema":"airk.private-selected-Context-metadata-owned-execution/1","owned_result":result,"strict_normal_close":settled,"inputs_unchanged":unchanged,"protocol_sha256":args.protocol_sha256,"decision_sha256":args.root_decision_sha256,"reservation_sha256":sha(output) if output.exists()else None,"task_question_gold_corpus_opened":False,"complete":bool(settled and unchanged and output.exists())}
    with receipt.open("x",encoding="utf-8")as stream:json.dump(report,stream,indent=2);stream.write("\n")
    print(json.dumps(report));return 0 if report["complete"]else 1
if __name__=="__main__":sys.exit(main())
