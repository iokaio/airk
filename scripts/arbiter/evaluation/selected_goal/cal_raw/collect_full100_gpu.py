# SPDX-License-Identifier: Apache-2.0
"""Join the seven sealed first56 CUDA phases with six original terminal phases without rerunning earlier forwards."""
import argparse
import json
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
prefix = runpy.run_path(str(HERE/"collect_first56_gpu.py"))
E = prefix["E"]
PREFIX = "ad0b23ea2625e795fb7acd985613d26de33e1133817829f46464a0dcdb2558fe"


def collect(projection_sha):
    study = runpy.run_path(str(HERE/"cutlass_study_inputs.py"))
    pin, require, read = prefix["pin"],study["require"],study["json_pin"]
    sealed = pin(E/"compact-r5-first56-protected-gpu.private.json")
    require(sealed["sha256"]==PREFIX,"original first56 GPU roster changed")
    old = read(sealed)
    require(old["projection"]["sha256"]==prefix["PROJECTION"] and len(old["provider"])==7,
            "original seven phases required")
    project = pin(E/"r5-chronological-native-full100-r1/projection-receipt.json")
    require(project["sha256"]==projection_sha,"terminal projection changed")
    full = read(project)
    require(len(full["terminal"])==3 and len(full["lifetimes"])==2,"original full100 terminal closure required")
    old_model = read(old["projection"])["models"]["compact"]
    model = full["models"]["compact"]
    rows = read(model)["rows"]
    require(len(rows)==100 and [r["ordinal"] for r in rows]==list(range(100)) and read(old_model)["rows"]==rows[:56],
            "original ordering, compiler output or remaining budget changed")
    refs = [*old["provider"],*(pin(E/f"compact-cutlass-cal-r5-full100-phase{i:02}-r1/execution.json") for i in range(7,13))]
    counts = []
    for i,ref in enumerate(refs):
        expected = rows[i*8:(i+1)*8]
        owner = old_model if i<7 else model
        scored = study["provider"](ref,i,expected,{owner["sha256"]:owner})
        require([r["id"] for r in scored]==[r["id"] for r in expected],"native phase order changed")
        counts.append(sum(r["status"]=="raw_scores_only" for r in scored))
    return {"scope":"R5 full100 protected Compact raw native results","first56":sealed,"projection":project,
            "rows":len(rows),"provider":refs,"forward_counts":counts,"gain_rule_applied":False,
            "consumer":pin(HERE/"cutlass_study_inputs.py"),"collector":pin(Path(__file__).resolve())}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--projection-sha256",required=True)
    print(json.dumps(collect(p.parse_args().projection_sha256),allow_nan=False))
