# SPDX-License-Identifier: Apache-2.0
"""Seal a future actual fresh-image packet; no compile/test/NN call in this helper."""
from pathlib import Path
import argparse,hashlib,json,sys

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
BASE=ROOT/"artifacts/advanced-split/focus-native-qualification-preparation-r1/native-parity-packet-released-r2.json"

def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--parent",type=Path,required=True);p.add_argument("--parent-sha256",required=True)
    p.add_argument("--cargo-json",type=Path,required=True);p.add_argument("--cargo-json-sha256",required=True)
    p.add_argument("--output",type=Path,required=True);a=p.parse_args()
    if a.output.exists() or a.output.parent.resolve()!=HERE or sha(a.parent)!=a.parent_sha256 or sha(a.cargo_json)!=a.cargo_json_sha256:
        raise ValueError("fresh output and actual reviewed parent/Cargo JSON hashes required")
    packet=json.loads(a.parent.read_bytes());receipt_path=Path(packet["compile_receipt"])
    receipt=json.loads(receipt_path.read_bytes())
    if receipt["exit_code"]!=0 or any(receipt.get(key) is not True for key in
        ["inputs_unchanged","runner_unchanged","runtime_unchanged","command_inputs_unchanged"]):
        raise ValueError("actual successful unchanged new compiler receipt required")
    rows=[]
    for line in a.cargo_json.read_text().splitlines():
        try:row=json.loads(line)
        except ValueError:continue
        rows.append(row)
    images=[row for row in rows if row.get("reason")=="compiler-artifact" and row.get("target",{}).get("name")=="airk_kernel" and row.get("profile",{}).get("test") and row.get("executable")]
    if len(images)!=1 or sha(packet["binary"]["path"])!=packet["binary"]["sha256"]:
        raise ValueError("one actual new Kernel artifact and intact retained copy required")
    roots={Path(row["manifest_path"]).parent for row in rows if row.get("reason")=="compiler-artifact"
           and row.get("manifest_path") and Path(row["manifest_path"]).is_relative_to(ROOT/"crates")}
    if {path.name for path in roots}!={"airk-core","airk-process","airk-ingest","airk-store","airk-retrieval","airk-kernel"}:
        raise ValueError("unexpected compiled Kernel AIRK dependency closure")
    prefixes=tuple(path.relative_to(ROOT).as_posix()+"/" for path in roots)
    consumed={name:pin for name,pin in receipt["groups"]["native"]["files"].items()
              if name.startswith(prefixes) or name in {"Cargo.toml","Cargo.lock","rust-toolchain.toml"}
              or name.startswith((".cargo/","third_party/"))}
    for name,pin in consumed.items():
        if sha(ROOT/name)!=pin:raise ValueError("new compiled source changed: "+name)
        packet["pins"][str(ROOT/name)]=pin
    runtime={name:pin for name,pin in json.loads(BASE.read_bytes())["pins"].items() if not Path(name).is_relative_to(ROOT)}
    if len(runtime)!=19176 or not any(Path(name).resolve()==Path(sys.executable).resolve() for name in runtime):
        raise ValueError("exact actual reviewed base runtime/interpreter closure required")
    for name,pin in runtime.items():
        if sha(name)!=pin:raise ValueError("actual base runtime changed: "+name)
    packet["pins"].update(runtime)
    for path in [a.parent.resolve(),a.cargo_json.resolve(),Path(__file__).resolve(),BASE]:packet["pins"][str(path)]=sha(path)
    packet["status"]="PREPARED_FOR_ROOT_REVIEW"
    packet["fresh_image_provenance"]={"original_actual_Cargo_artifact":images[0],"retained_copy":packet["binary"],
        "copy_binding":"retained immediately from the original actual Cargo executable path by reviewed retain_compiler.py; Cargo JSON retains its original executable field",
        "consumed_AIRK_crates":sorted(path.name for path in roots),"current_source_pins":len(consumed),
        "base_runtime_rows":len(runtime),"no_R95_run_inheritance":True,
        "scope":"Fresh whole one-image batch only; two cheap controls, typed emission,24numeric,fourDEV64,matchedDEV grade. No FINAL or promotion."}
    a.output.write_text(json.dumps(packet,indent=2)+"\n")
    print(json.dumps({"packet":str(a.output),"sha256":sha(a.output),"binary":packet["binary"],"NN_executed":False}))

if __name__=="__main__":main()
