# SPDX-License-Identifier: Apache-2.0
"""Reuse exact already-qualified launcher base-runtime pins; no native launch."""
from pathlib import Path
import hashlib,json,sys

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
SOURCE=HERE/"native-packet-r95-r1.json"
SOURCE_PIN="acc353886759a547ac8cef3f17c8c2f938898317c99f7924df1b05b5e5051b96"
BASE=ROOT/"artifacts/advanced-split/focus-native-qualification-preparation-r1/native-parity-packet-released-r2.json"

def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def main():
    output=HERE/"native-packet-r95-r1-proposed.json"
    if output.exists() or sha(SOURCE)!=SOURCE_PIN:raise ValueError("fresh actual packet required")
    parent=json.loads(BASE.read_bytes());runtime={name:pin for name,pin in parent["pins"].items() if not Path(name).is_relative_to(ROOT)}
    if len(runtime)!=19176:raise ValueError("exact unchanged actual Focus base runtime closure required")
    interpreter=[name for name in runtime if Path(name).resolve()==Path(sys.executable).resolve()]
    if len(interpreter)!=1:raise ValueError("actual launcher interpreter absent from reused closure")
    for name,pin in runtime.items():
        if sha(name)!=pin:raise ValueError("actual base runtime changed: "+name)
    packet=json.loads(SOURCE.read_bytes());packet["pins"].update(runtime)
    packet["pins"].update({str(BASE):sha(BASE),str(Path(__file__).resolve()):sha(__file__)})
    packet["status"]="PREPARED_FOR_ROOT_REVIEW"
    packet["materialized_R95_provenance"]={"binding":str(HERE/"materialized-build-r95/archive-binding.json"),
        "original_packet_sha256":SOURCE_PIN,"binary_origin":"Existing Root archived R95 PE bound to unchanged actual Cargo JSON executable record; not rebuilt/relabelled current output",
        "launcher_runtime_rows":len(runtime),"launcher_runtime_parent":str(BASE),"launcher_runtime_parent_sha256":sha(BASE),
        "launcher_python":interpreter[0],"current_unrelated_source_changes":"Disclosed in archive binding; all six compiled AIRK crate closures remain exact R95",
        "scope":"Only two cheap controls then typed emission,24native numerical cases,four DEV64 roles and matched DEV grade. No old six-model/FINAL205 execution or release claim."}
    output.write_text(json.dumps(packet,indent=2)+"\n")
    print(json.dumps({"packet":str(output),"sha256":sha(output),"binary":packet["binary"],"base_runtime_pins":len(runtime),"native_executed":False}))

if __name__=="__main__":main()
