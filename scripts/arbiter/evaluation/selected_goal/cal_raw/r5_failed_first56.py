# SPDX-License-Identifier: Apache-2.0
"""Reopen R5's previously sealed first 56 after its independently failed terminal closure."""
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import r5_chronological_inputs as original
from phase_reference import checked, require
from assemble import json_pin

ROOT = original.ROOT
PREFIX = "a8be658b0e414115dba40ffa9460054b47d8d68886d943c8858ce56f7c7181e6"
LIFE = "1f389796b48ceccb9df2e79fe906a38787c2aeabf241988741cf97f558769731"
OUTER = "23aedfee1e92ef826714f37b852301894f048f4ad3964e11eb75e65fd7b4e276"
RELEASE = "a96669494a7e7a8ee37c45609a1950454097efe45a4f6b3fdf7a7e7fc2759779"
SHUTDOWN = "98db2e5b96ba854e8f69f82c31b76a343cf6d1fee88aa5f96e143747ddc04672"
PHASE = "2e2377f941ffdccfc14b7330659dfcfcfda516b567d3f0bf69f41a15d7f1c178"


def read(path, digest):
    return json_pin({"path":str(path),"sha256":digest})


def failure():
    plan = read(ROOT/"lifetime-plan.json",original.PLAN)
    acquired = read(ROOT/"control-mutex-acquired.json",original.ACQUIRED)
    outer = read(ROOT/"outer-owned.json",OUTER)
    release = read(ROOT/"control-mutex-released.json",RELEASE)
    path = ROOT/"lifetime-01"
    phase = read(path/"phase-12.owned.json",PHASE)
    closed = read(path/"fixture/closed.json",SHUTDOWN)
    require(not (ROOT/"fixture-closure.json").exists() and not (path/"lifetime-closure.json").exists(),
            "conflicting R5 terminal evidence")
    for job in (outer,phase):
        require(job["exit_code"] == 1 and job["owned_descendants_settled"] is True and
                job["timed_out"] is job["owned_job_forced_termination"] is False and
                job["owned_job_final_active_processes"] == 0 and job["owned_job_final_member_pids"] == [],
                "unverified failed Job settlement")
    for name, filename, sha in (("acquired","control-mutex-acquired.json",original.ACQUIRED),
                                 ("outer","outer-owned.json",OUTER)):
        require(Path(release[name]["path"]) == ROOT/filename and release[name]["sha256"] == sha,"wrong mutex chain")
    require(acquired["name"] == release["name"] and acquired["owner_pid"] == release["owner_pid"] == 30592 and
            acquired["at_ms"] <= plan["start_ms"] < phase["end_ms"] <= release["at_ms"] and
            closed["shutdown_completed"] is closed["service_lease_reacquired"] is True, "foreign terminal installation")
    transcript = checked(phase["transcript"])
    require(Path(phase["transcript"]["path"]) == path/"phase-12.jsonl","foreign last transcript")
    events = [json.loads(line) for line in transcript.splitlines() if line.startswith(b'{"kind":')]
    ids = plan["phase_ids"][12]
    def observed(kind, ident):
        return [ident(e) for e in events if e["kind"] == kind]
    require(len(ids) == 4 and observed("cal_question",lambda e:e["evidence"]["id"]) == ids[:2] and
            observed("window_grant",lambda e:e["question_id"]) == ids[:3] and
            observed("window_release",lambda e:e["question_id"]) == ids[:2], "unexpected last phase outcome")
    return plan


def reopen(pin):
    require(Path(pin["path"]) == original.CAMPAIGN/"r5-chronological-native-first56-r1/projection-receipt.json"
            and pin["sha256"] == PREFIX,"only the originally sealed 56 may be reopened")
    receipt = json_pin(pin)
    require(receipt["lifetimes"] == [LIFE] and not receipt["terminal"] and receipt["original_requests"] == 56
            and receipt["plan_sha256"] == original.PLAN and receipt["acquired_sha256"] == original.ACQUIRED
            and receipt["gain_certificates"] == receipt["NN_invocations"] == 0,"unexpected original prefix")
    plan = failure()
    original.check_plan(plan)
    reports = original.inspect(ROOT,0,original.PLAN,LIFE,all_phases=True)
    model = json_pin(receipt["models"]["compact"])
    refs = model["phases"]
    require(len(reports) == len(refs) == 7 and all(json_pin(ref) == report for ref,report in zip(refs,reports,strict=True)),
            "first lifetime phase evidence differs")
    rebuilt = original.projections(plan,reports,refs)["compact"]
    require(rebuilt == model and len(rebuilt["rows"]) == 56,"original first56 compiler projection differs")
    return rebuilt["rows"],"r5_original"


if __name__ == "__main__":
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Python -I -B -S required")
    rows,_ = reopen({"path":str(original.CAMPAIGN/"r5-chronological-native-first56-r1/projection-receipt.json"),"sha256":PREFIX})
    print(json.dumps({"verified_original_prefix":len(rows),"terminal":"incomplete","whole_cal100":False,
                      "gain_certificates":0,"new_native_forwards":0,"prefix_sha256":PREFIX,"failed_outer_sha256":OUTER}))
