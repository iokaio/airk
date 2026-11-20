# SPDX-License-Identifier: Apache-2.0
"""Independent grouping and clock checks for the fresh 32/32/36 CAL capture."""
from reference_inputs import HERE, checked, read, require

GROUPS = (tuple(range(4)), tuple(range(4,8)), tuple(range(8,13)))
FIXTURE = "airk.private.cal100-grouped-fixture/1"
PLAN = "airk.private.cal100-grouped-plan/1"
OBSERVER = "80e8ca821bf6828314f7c0a0c9e3932c6634d9c1e594a6c0f417a0115b9dc301"


TWO_PLAN = "airk.private.cal100-two-lifetime-plan/1"
TWO_FIXTURE = "airk.private.cal100-two-lifetime-fixture/1"
TWO_GROUPS = (tuple(range(7)), tuple(range(7,13)))
PLAN_FOR_FIXTURE = {FIXTURE: PLAN, TWO_FIXTURE: TWO_PLAN}


def shape(schema):
    require(schema in (PLAN,TWO_PLAN), "unregistered grouped lifetime plan")
    return TWO_GROUPS if schema == TWO_PLAN else GROUPS


def check_capacity_basis(basis, prior, focus):
    import runpy
    from reference_inputs import digest
    proof = HERE.parent/"cal_capture/dispatch_proof.py"
    require(digest(proof.read_bytes()) == "75b370b0446e0be1946896cb8978e8028aed0fb939320d4c72426f40386ea027",
            "changed native capacity proof")
    read_dispatch = runpy.run_path(str(proof))["read_journal"]
    require(set(basis) == {"phase_closures","observed","remaining","per_question_upper","group_upper"}
            and len(basis["phase_closures"]) == 11, "incorrect capacity-only evidence scope")
    dispatches = []
    for i, pin in enumerate(basis["phase_closures"]):
        lifetime = read(pin,prior/f"lifetime-{i:02}/lifetime-closure.json")
        fixture = prior/f"lifetime-{i:02}/fixture"
        require(read(lifetime["closed"],fixture/"closed.json")["service_lease_reacquired"] is True
                and lifetime["dispatch"] == read_dispatch(fixture,focus,setup_only=False),
                "original lifetime or capacity journal differs")
        count = lifetime["dispatch"]["focus_dispatches"]
        require(lifetime["schema"] == "airk.private.cal100-phase-lifetime/1" and lifetime["phase"] == i
                and type(count) is int and 0 <= count <= 8*28, "wrong historical capacity counter")
        dispatches.append(count)
    observed = [sum(dispatches[:7]),sum(dispatches[7:])]
    require(observed == basis["observed"] == [960,584] and basis["remaining"] == 12
            and basis["per_question_upper"] == 28
            and basis["group_upper"] == [observed[0],observed[1]+12*28] == [960,920],
            "changed capacity estimates or recorder allowance")


def check_plan(plan):
    require(plan["phase_groups"] == [list(group) for group in shape(plan["schema"])],
            "grouped CAL must preserve the registered original phase partition")
    capture = HERE.parent/"cal_capture"
    expected = {capture/name for name in (
        "controller.py","prepare.py","phase.cjs","worker.cjs","window_scope.cjs","dispatch_proof.py","launch.py",
        "phased/capture.py","phased/lifetime.py","grouped/run_grouped.py","grouped/group_service.py","grouped/installation_mutex.py")}
    if plan["schema"] == TWO_PLAN:
        expected.add(capture/"grouped/two_lifetimes.py")
    expected.update((HERE.parents[3]/"provision-static-onnx-runtime.py", capture.parents[1]/"run_compact_answer.py",
                     capture.parent/"replay_cal_population.py",
                     HERE.parents[3]/"arbiter/analysis/public_source_admission/native_cohort_source_reads.py"))
    require(len(plan["sources"]) == len(expected) and {r["path"] for r in plan["sources"]} == {str(p) for p in expected},
            "grouped source closure incomplete")
    for ref in plan["sources"]:
        checked(ref)
    from reference_inputs import CAMPAIGN
    prior = CAMPAIGN/"cal100-focus1024-capture-r4"
    if plan["schema"] == TWO_PLAN:
        audit = read(plan["input_audit"])
        check_capacity_basis(plan["capacity_basis"],prior,audit["profiles"]["arbiter-focus-qwen3-reranker-06b"])
    previous = read(plan["prior_attempt"],prior/"outer-owned.json")
    require(type(previous["exit_code"]) is int and previous["exit_code"] != 0
            and previous["owned_descendants_settled"] is True and previous["owned_job_final_active_processes"] == 0
            and previous["owned_job_final_member_pids"] == [] and not (prior/"fixture-closure.json").exists(),
            "R4 lacks settled incomplete evidence")
    recorder = plan["recorder"]
    require(set(recorder) == {"source","admission_limit","record_limit","byte_limit"}
            and recorder["source"]["sha256"] == OBSERVER
            and recorder["admission_limit"] == 1024 and recorder["record_limit"] == 8192
            and recorder["byte_limit"] == 64 << 20, "original observer admission/byte caps differ")
    source = HERE.parents[4]/"crates/airk-service/src/service_runtime/evaluation_fixture_tests/execution_observer.rs"
    require(recorder["source"]["path"] == str(source) and checked(recorder["source"]),
            "original observer source changed")


def check_mutex(root, plan, outer_sha, release_sha, last):
    from reference_inputs import reference
    require(isinstance(release_sha,str) and len(release_sha)==64, "pinned final group mutex release required")
    acquired = read(plan["installation_mutex"],root/"control-mutex-acquired.json")
    released = read(reference(root/"control-mutex-released.json",release_sha))
    name = r"Local\AIRK-20261116-Focus1024-Reuse"
    require(set(acquired)=={"name","owner_pid","at_ms"} and acquired["name"]==released["name"]==name
            and type(acquired["owner_pid"]) is int and acquired["owner_pid"]>0
            and released["owner_pid"] == acquired["owner_pid"]
            and type(acquired["at_ms"]) is int and type(released["at_ms"]) is int
            and acquired["at_ms"] <= plan["start_ms"] <= last <= released["at_ms"]
            and released["acquired"]==plan["installation_mutex"]
            and released["outer"]==reference(root/"outer-owned.json",outer_sha),
            "unclosed or foreign shared installation ownership")


def check_clocks(lifetime, rows, common):
    previous = lifetime["start_ms"]
    require(rows and type(previous) is int and rows[0]["start_ms"] == previous,
            "first phase must charge its native startup")
    for row in rows:
        start,end,deadline = (row[k] for k in ("start_ms","end_ms","deadline_ms"))
        require(all(type(n) is int for n in (start,end,deadline)) and previous <= start < end <= deadline
                and deadline == min(start+1800000,common), "overlapping or extended group phase")
        previous = end
    require(previous <= lifetime["end_ms"] <= rows[-1]["deadline_ms"],
            "last phase exceeded its original bound during native shutdown")


def check_group(directory, lifetime, group, plan, previous):
    phases = list(shape(plan["schema"])[group])
    require(lifetime["schema"] == "airk.private.cal100-group-lifetime/1" and lifetime["group"] == group
            and lifetime["phases"] == phases and len(lifetime["phase_closures"]) == len(phases)
            and type(lifetime["end_ms"]) is int and previous <= lifetime["start_ms"] < lifetime["end_ms"],
            "wrong native group, overlapping lifetime or incomplete original phase closure")
    owners = [read(pin,directory/f"phase-{phase:02}.owned.json")
              for phase,pin in zip(phases,lifetime["phase_closures"],strict=True)]
    check_clocks(lifetime,owners,plan["deadline_ms"])
    dispatch = lifetime["dispatch"]
    require(type(dispatch["focus_dispatches"]) is int and 0 <= dispatch["focus_dispatches"] <= 1024
            and type(dispatch["records"]) is int and 0 <= dispatch["records"] <= 8192,
            "native group exceeded its own recorder limits")
    return lifetime["phase_closures"]
