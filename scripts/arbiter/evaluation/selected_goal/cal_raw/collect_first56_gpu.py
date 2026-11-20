# SPDX-License-Identifier: Apache-2.0
"""Reopen the seven fresh R5 Compact CUDA phases and emit ordered scientific source pins."""
import hashlib
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
E = HERE.parents[4]/"artifacts/v190/final-plan/exec-20261116T052942Z"
PROJECTION = "a8be658b0e414115dba40ffa9460054b47d8d68886d943c8858ce56f7c7181e6"


def pin(path):
    with path.open("rb") as stream:
        checksum = hashlib.file_digest(stream,"sha256").hexdigest()
    return {"path":str(path),"bytes":path.stat().st_size,"sha256":checksum}


def collect():
    study = runpy.run_path(str(HERE/"cutlass_study_inputs.py"))
    project = pin(E/"r5-chronological-native-first56-r1/projection-receipt.json")
    study["require"](project["sha256"] == PROJECTION,"original first56 projection changed")
    original = study["json_pin"](project)
    model = original["models"]["compact"]
    rows = study["json_pin"](model)["rows"]
    study["require"]([r["ordinal"] for r in rows] == list(range(56)),"original first56 chronology changed")
    refs, counts = [], []
    for i in range(7):
        receipt = pin(E/f"compact-cutlass-cal-r5-first56-phase{i:02}-r1/execution.json")
        phase = rows[i*8:(i+1)*8]
        verified = study["provider"](receipt,i,phase,{model["sha256"]:model})
        study["require"]([r["id"] for r in verified] == [r["id"] for r in phase],"provider IDs changed")
        counts.append(sum(r["status"]=="raw_scores_only" for r in verified))
        refs.append(receipt)
    return {"scope":"fresh R5 first56 protected Compact raw forwards","projection":project,"provider":refs,
            "forward_counts":counts,"rows":len(rows),"complete_population":False,"gain_rule_applied":False,
            "consumer":pin(HERE/"cutlass_study_inputs.py"),"collector":pin(Path(__file__).resolve())}


if __name__=="__main__":
    print(json.dumps(collect(),allow_nan=False))
