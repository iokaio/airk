# SPDX-License-Identifier: Apache-2.0
"""Verify Q598 r2's genuine stopped-incarnation changes against the last idle Search state."""
from collections import Counter
import json
from pathlib import Path
import sqlite3

CLOSED = "f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771"


def verify(reuse, prior, digest):
    root = reuse["E"]/"compact-development-TRAIN_Q598-focus1024-run-r2"
    check, read, linked = reuse["require"], reuse["read"], reuse["linked"]
    check(prior == root/"owned.json" and digest == CLOSED, "foreign Q598 r2 retirement")
    owner=read(prior,digest)
    proof=owner["shared_installation"]
    check(reuse["settled"](owner) and owner["exit_code"]==1 and not owner["timed_out"] and owner["source_seals_unchanged"],
          "incomplete Q598 r2 retirement")
    check({p.name for p in (root/"commands").glob("*.command.json")} ==
          {"failure-stage-at-shutdown.command.json","shutdown.command.json"} and "answer_command" not in proof,"unexpected earlier work")
    ready, closed, saved = (linked(proof[key]) for key in ("ready","closed","saved"))
    check(closed["shutdown_completed"] and closed["service_lease_reacquired"] and closed["instance"]==ready["health"]["instance"],
          "Q598 r2 did not close its native lease")
    status=linked(proof["failure_stage"])
    idle=linked(linked(proof["restart_request"])["status"])
    check(status["configuration"] == idle["configuration"] == saved["configuration"] and status["configuration"]["revision"]==13 and
          all(status[k]["availability"]=="warming" for k in ("focus","context")) and
          all(idle[k]["queued"]==idle[k]["running"]==0 for k in ("focus","context")), "retirement or original idle source differs")
    old,new=(linked(proof["control_"+phase]) for phase in ("before","after"))
    check(proof["control_before"]["sha256"]=="9bc4b0cd2c95fb9ac70c819508142cab5076c1449c27a97adf16e8a2bc1ff2a9" and
          old["directories"]==new["directories"] and old["immutable"]==new["immutable"] and old["files"].keys()==new["files"].keys(),
          "original Search after-state or full file census differs")
    original=root/"control-before"
    current=reuse["CONTROL"]
    check({p.relative_to(current).as_posix() for p in current.rglob("*") if p.is_file()} == new["files"].keys(),
          "current physical file census changed")
    differences={k for k,v in old["files"].items() if v!=new["files"][k]}
    check(differences=={"http-1/endpoint.json","logs-1.4/current.jsonl","service-1.4.sqlite3"}, "unknown persisted mutation")
    for k in new["files"]:
        check(reuse["link"](current/k)["sha256"] == new["files"][k], "physical post-state differs")
    for k in differences:
        check(reuse["link"](original/k)["sha256"]==old["files"][k],"original mutable backup differs")
    k="http-1/endpoint.json"
    a,b=read(original/k),read(current/k)
    check({k for k in a if a[k]!=b[k]}=={"generation","service_instance","predecessor_digest"} and
          b["generation"]==a["generation"]+1 and b["service_instance"]==closed["instance"], "unexpected endpoint incarnation")
    k="logs-1.4/current.jsonl"
    prefix=(original/k).read_bytes()
    full=(current/k).read_bytes()
    check(full.startswith(prefix),"service log rewritten")
    events=[json.loads(line) for line in full[len(prefix):].splitlines()]
    check([e["kind"] for e in events]==["service_started","model_recovery","shutdown_requested","service_stopped"]
          and all(e["instance"]==closed["instance"] for e in events),"unexpected service log changes")
    key="service-1.4.sqlite3"
    with sqlite3.connect((original/key).as_uri()+"?mode=ro&immutable=1",uri=True) as first, sqlite3.connect(
            (current/key).as_uri()+"?mode=ro&immutable=1",uri=True) as second:
        schema="select * from sqlite_master order by name"
        check(list(first.execute(schema))==list(second.execute(schema)),"control database schema changed")
        for (table,) in first.execute("select name from sqlite_master where type='table'"):
            query='select * from "'+table.replace('"','""')+'"'
            left, right = list(first.execute(query)),list(second.execute(query))
            if table=="service_lifecycle_state":
                check(left==[(1,0,a["service_instance"])] and right==[(1,0,b["service_instance"])],"foreign service lifecycle")
            else:
                check(Counter(left)==Counter(right),"control database content changed: "+table)
    return proof["previous"]
