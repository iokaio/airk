# SPDX-License-Identifier: Apache-2.0
"""Independently reverse the exact R6 service/worker path edits to both frozen R5 source digests."""
import hashlib
import json


def restore(data, substitutions, original_sha):
    for current,original in substitutions:
        current,original=current.encode(),original.encode()
        if data.count(current)!=1: raise ValueError("ambiguous candidate source adjustment")
        data=data.replace(current,original)
    if hashlib.sha256(data).hexdigest()!=original_sha:
        raise ValueError("extra logic in candidate service or phase worker")


def verify(root, capture, folder, audit_sha, legacy_directory, projected):
    old="87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661"
    restore(projected["worker.cjs"],[
        ("require("+json.dumps(str(capture/"phase.cjs"))+")","require('./phase.cjs')"),
        ("require("+json.dumps(str(capture/"window_scope.cjs"))+")","require('./window_scope.cjs')"),
        (json.dumps(str(root)),"path.resolve(__dirname,'../../../../..')"),
        (audit_sha,old),("'cal100-fused-candidate-launch-r1'","'cal100-focus1024-launch-r1'"),
        (json.dumps(str(capture/"controller.py")),"path.join(__dirname, 'controller.py')"),
    ],"22216d4262e25e24f2c5c53138dbc77a486f8373b6e55d70c850f45fb4d40b51")
    restore(projected["group_service.py"],[
        ("from controller import ","from controller import AUDIT_SHA, "),
        ("from prepare import SELECTOR, pin\n\nAUDIT_SHA = "+repr(audit_sha)+"\nWORKER = Path("+repr(str(folder/"worker.cjs"))+")",
         "from prepare import SELECTOR, pin"),
        ('link(WORKER if n == "worker.cjs" else HERE / n) for n in ("worker.cjs",','link(HERE / n) for n in ("worker.cjs",'),
        ("[node, WORKER,",'[node, HERE/"worker.cjs",'),
        ('env.update(audit["fixture_environment"])\n    env["PATH"] = '+repr(str(legacy_directory))+' + os.pathsep + env["PATH"]',
         'env.update(audit["fixture_environment"])'),
    ],"d4c6acb3dc9b091127c97ba13932fcf3ad8bb95282cac92778a2b1c3970b9533")
