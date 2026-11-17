# SPDX-License-Identifier: Apache-2.0
"""Reuse frozen packet preparation, then seal the reviewed private-base amendment."""
from pathlib import Path
import argparse,hashlib,json,shutil,subprocess,sys
ROOT=Path(__file__).resolve().parents[4];HERE=Path(__file__).resolve().parent;MODELS=HERE.parent
FROZEN=MODELS/"native-parent-bag-export-source-r1"
def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ["artifact-lock","compile-receipt","output","anchor-prefix"]:parser.add_argument("--"+name,type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists() or args.output.parent.resolve()!=HERE.resolve():raise ValueError("fresh packet inside reviewed preparation required")
    if not args.anchor_prefix.is_absolute() or args.anchor_prefix.parent.resolve()!=MODELS.resolve() or not args.anchor_prefix.name.startswith("native-parent-bag-native-private-anchor-"):
        raise ValueError("fresh absolute private anchor prefix under models required")
    parent=args.output.with_name(args.output.name+".parent.json")
    subprocess.run([sys.executable,"-B",str(FROZEN/"prepare_native_packet.py"),"--artifact-lock",str(args.artifact_lock),
        "--compile-receipt",str(args.compile_receipt),"--output",str(parent)],check=True,cwd=ROOT)
    packet=json.loads(parent.read_bytes())
    seal=json.loads((MODELS/"native-parent-bag-export-execution-r1/seal.json").read_bytes())
    frozen=[row for row in seal["files"] if Path(row["path"]).parent==FROZEN]
    training=[row for row in seal["files"] if Path(row["path"]).is_relative_to(ROOT/"tools/retrieval-train")]
    if len(frozen)!=22 or len(training)!=91:raise ValueError("all22 export and91 public training source pins required")
    frozen+=training
    for row in frozen:
        if sha(row["path"])!=row["sha256"]:raise ValueError("frozen export source changed")
        packet["pins"][row["path"]]=row["sha256"]
    powershell=shutil.which("pwsh.exe")
    if powershell is None:raise ValueError("actual PowerShell image required")
    paths=[HERE/"run_native.py",HERE/"anchor_runtime.py",HERE/"anchor_bridge.ps1",Path(powershell),
        MODELS/"a1-fresh-final-materializer-r5/prepare-anchor.ps1",MODELS/"observe-qasper-final-anchor-r3.ps1",
        ROOT/"crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs"]
    for path in paths:packet["pins"][str(path.resolve())]=sha(path)
    packet["private_anchor_prefix"]=str(args.anchor_prefix)
    packet["anchor_powershell"]=str(Path(powershell).resolve())
    packet["private_base_preparation"]={"parent_packet_sha256":sha(parent),"frozen_export_sources":22,"frozen_public_training_sources":91,
        "scope":"Same selected four roles; actual protected current-SID anchors, no old FINAL or new model authority"}
    args.output.write_text(json.dumps(packet,indent=2)+"\n")
    print(json.dumps({"packet":str(args.output),"sha256":sha(args.output),"executed":False}))
if __name__=="__main__":main()
