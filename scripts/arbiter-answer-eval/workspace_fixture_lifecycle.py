# SPDX-License-Identifier: Apache-2.0
"""Ownership proof for writable experimental scopes beside a frozen CUDA fixture."""
import hashlib
import json
from pathlib import Path
import re
import sqlite3


def require(value, message):
    if not value:
        raise ValueError(message)


def digest_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def change_proof(before, after, allowed, retired=frozenset()):
    missing = before - after
    require(retired <= allowed and all(row[0] in retired for row in missing),
            "an earlier unretired source job receipt disappeared")
    added = after - before
    require(all(row[0] in allowed for row in added), "source I/O on a non-workspace binding")
    def digest(rows):
        return hashlib.sha256(json.dumps(sorted(rows), separators=(",", ":")).encode()).hexdigest()
    return {"initial_count": len(before), "initial_sha256": digest(before), "current_count": len(after),
            "current_sha256": digest(after), "new_workspace_jobs": sorted(added), "receipts_removed_by_native_retirement": sorted(missing)}


def settlement(control, before, after, members):
    db = (Path(control) / "service-1.4.sqlite3").resolve(strict=True)
    missing_bindings = {row[0] for row in before - after}
    retired = set()
    with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as connection:
        for binding in missing_bindings & members.keys():
            index = members[binding]["index"]
            acknowledged = connection.execute(
                "select exists(select 1 from service_removed_projects where binding=?) "
                "and not exists(select 1 from projects where binding=? and (registered<>0 or cleanup_guard is not null)) "
                "and not exists(select 1 from index_definitions where id=? and (tombstone=0 or legacy_binding<>?))",
                (binding, binding, index, binding)).fetchone()[0]
            if acknowledged:
                retired.add(binding)
    return change_proof(before, after, set(members), retired)


def admission(fixtures, prior, roster_path, roster_digest):
    roster_path = Path(roster_path).resolve(strict=True)
    fixtures = Path(fixtures).resolve(strict=True)
    require(roster_path.parent.name == "native-state"
            and roster_path.is_relative_to(fixtures.parent / "agent-cost")
            and digest_file(roster_path) == roster_digest, "workspace roster location or SHA-256 changed")
    roster = json.loads(roster_path.read_bytes())
    require(roster.get("schema") == "airk.eval.workspace-roster/1" and 0 < len(roster["cases"]) <= 64,
            "expected a bounded experimental workspace roster")
    require(prior.get("restartable") is True and prior.get("restart_blockers") == {
        "unfinished_jobs": [], "setup_pending": []}, "prior native install has live jobs or incomplete setup")
    original = {r["binding"] for r in prior["indexes"].values()}
    original_indexes = {r["index_id"] for r in prior["indexes"].values()}
    members = {}
    life = fixtures / prior["lifetime"]
    parent = roster_path.parent
    db = (Path(prior["control"]) / "service-1.4.sqlite3").resolve(strict=True)
    with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as connection:
        for case, digest in roster["cases"].items():
            require(re.fullmatch(r"[A-Za-z0-9_-]+\.(?:native|candidate)", case), "invalid case directory")
            setup = parent / case / "setup.jsonl"
            require(digest_file(setup) == digest, "native setup receipt changed")
            saved = json.loads(setup.read_bytes().splitlines()[-1])["receipt"]
            binding = saved["request"]["registration"]["binding"]
            row = connection.execute("select payload, registered from projects where binding=?", (binding,)).fetchone()
            require(row and row[1] == 1 and json.loads(row[0]) == saved["request"]["registration"],
                    "native project no longer matches the registered setup")
            index_row = connection.execute("select legacy_binding,active_registration,tombstone from index_definitions where id=?",
                                           (saved["index_id"],)).fetchone()
            require(index_row == (binding, binding, 0), "native index does not own this active registration")
            project = json.loads(row[0])
            require(Path(project["root"]).samefile(parent.parent / "runs" / case / "work")
                    and Path(project["data_root"]).samefile(parent / case / "data")
                    and binding not in original | set(members)
                    and saved["index_id"] not in original_indexes | {r["index"] for r in members.values()},
                    "mutable registration is aliased or outside its disposable case")
            members[binding] = {"case": case, "index": saved["index_id"], "setup_sha256": digest}
        jobs = connection.execute("select project,id,kind from service_jobs where kind in ('index','vectors') "
                                  "order by seq").fetchall()
    result = {"roster": str(roster_path), "roster_sha256": roster_digest, "members": members}
    if prior["state"] == "closed":
        return result
    # A frozen-only worker writes its ready handle and real native close before its global job assertion.
    # Preserve that failure. Authorize the next lifetime from independent native and ownership receipts.
    owned_path, closed_path = life / "owned.json", life / "fixture" / "closed.json"
    owned = json.loads(owned_path.read_bytes())
    closed = json.loads(closed_path.read_bytes())
    reused = json.loads((life / "reused-indexes.json").read_bytes())
    require(prior["state"] == "ready" and prior.get("stop_reason") == "stop file" and not prior.get("error")
            and prior["service_exit_code"] == 0 and prior["final_stage_idle"] is True
            and closed["instance"] == prior["instance"] and closed["shutdown_completed"] is True
            and closed["service_lease_reacquired"] is True, "missing verified native closure")
    require(owned["exit_code"] == 1 and owned["owned_descendants_settled"] is True
            and owned["owned_job_final_active_processes"] == 0 and owned["owned_job_final_member_pids"] == []
            and owned["owned_job_forced_termination"] is False and owned["scripts_sha256_unchanged"] is True
            and owned["child_assigned_before_resume"] is True, "missing verified owned process settlement")
    require((life / "outer.log").read_text().strip().endswith(
        "ValueError: settled reuse observed new indexing or vector jobs"), "unexpected frozen-only launcher failure")
    require(digest_file(life / "reused-indexes.json") == prior["reused_indexes"]["sha256"]
            and reused["instance"] == prior["instance"] and reused["new_index_and_vector_jobs"] == 0
            and set(reused["repositories"]) == set(prior["indexes"]), "frozen initial reuse receipt changed")
    # The original launcher recorded its initial count. service_jobs.seq is AUTOINCREMENT. Only the
    # original repository bindings may occupy that entire prefix, and only declared cases may follow.
    count = reused["preserved_index_and_vector_jobs"]
    require(0 < count < len(jobs) and {x[0] for x in jobs[:count]} == original
            and {x[0] for x in jobs[count:]} <= members.keys(), "unexpected frozen source job history")
    result["recovered_predecessor"] = {"lifetime": prior["lifetime"], "service_close_sha256": digest_file(closed_path),
            "owned_sha256": digest_file(owned_path), "original_launcher_exit": 1,
            "source_jobs": change_proof(set(jobs[:count]), set(jobs), set(members))}
    return result
