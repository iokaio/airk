# SPDX-License-Identifier: Apache-2.0
"""Prepare and run the owned unprofiled Windows Compact Resource20 selector."""

import argparse
import ctypes
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import types

SELF = Path(__file__).resolve()
HERE = SELF.parent
ROOT = HERE.parents[1]
CONTRACT = HERE / "COMPACT-RESOURCE20.md"
WATCH = HERE / "resource20_watch.py"
NATIVE_RESOURCE = (
    "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/"
    "software_v2/cuda_qualification/bounded_hybrid/resource20.rs"
)
R2 = HERE / "bounded_hybrid_r2.py"
R2_SHA = "62466aa06e8afc1a306008c967eed6dd76f8688721c6d2106c14fbb2bc4882e8"
R2_PACKET_SHA = "9bc6fb35a912568be7027c25ccdf7a1b21f23c596779469db8b2494fa3c11545"
PROFILE_EXECUTION_SHA = "1abbd515e534f4056e3c48380fffd290654e0b92dea67088ae291512f02cf2a2"
PROFILE_RESULT_SHA = "f4a80598876dfdfe8daef8eac941bc0baebae267d29a824245a36a2ec9307db7"
POLICY_IDENTITY = "4d1428b76bf100df952d82f91b08bd0b482bf80294b1d5e3825dda351443ef71"
INPUT_DIGEST = "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0"
SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::"
            "cuda_qualification::bounded_hybrid::cuda_compact_fp32_bounded_hybrid_resource20")
SCHEMA = "airk.private.compact-bounded-hybrid-resource20-plan/1"
RESULT = "airk.private.cuda-compact-bounded-hybrid-resource/1"
HOST = 32 << 30
PROOFS = {"native_result": 16 << 20, "operations": 128 << 10, "stdout": 64 << 20}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, sha, name, replacements=()):
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(path.resolve(strict=True) == path and 0 < len(raw) <= 2 << 20
            and hashlib.sha256(raw).hexdigest() == sha, "reviewed maintained owner differs")
    source = raw.decode("utf-8")
    for before, after in replacements:
        require(source.count(before) == 1, "reviewed refresh adaptation anchor differs")
        source = source.replace(before, after)
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(source, str(path), "exec", dont_inherit=True), module.__dict__)
    return module, {"path": str(path), "bytes": len(raw), "sha256": sha}


def successor(fresh, path, previous, capacity, sid, binding, deadline):
    pin, raw = fresh.captured(path, R2_PACKET_SHA, fresh.MAX_CHAIN_JSON, deadline)
    packet = fresh.decode(raw)
    r2 = load(R2, R2_SHA, "compact_pinned_successor")[0]
    require(packet["predecessor"] == previous and packet["capacity_policy"] == capacity,
            "R2 package does not retain the exact R1 and capacity pins")
    ancestor = packet["static_graph_binding"]
    require(set(ancestor) == set(binding) and all(ancestor[key] == binding[key] for key in
            binding.keys() - {"build_receipt", "cargo_json", "actual_static_ORT_rows", "image_exports"}),
            "R2 package changes the approved static lineage")
    return pin, r2.inference_image(packet, sid)


def refresh_owner():
    replacements = [
        ('args = parser.parse_args(argv)',
         'parser.add_argument("--resource-predecessor-packet", required=True)\n    args = parser.parse_args(argv)'),
        ('    capacity = capacity_pin(args.capacity_policy_sha256, fresh, deadline)',
         '    capacity = capacity_pin(args.capacity_policy_sha256, fresh, deadline)\n'
         '    resource_predecessor, resource_image = resource_successor(\n'
         '        fresh, args.resource_predecessor_packet, previous, capacity, component.SID, static, deadline)\n'
         '    expected = runtime_pins(dict(seed, runtime_files=expected), resource_image)'),
        ('prep, previous, launcher, contract, capacity, seed["powershell"],',
         'prep, previous, resource_predecessor, launcher, contract, capacity, seed["powershell"],'),
        ('        "predecessor": previous, "capacity_policy": capacity,',
         '        "resource_predecessor": resource_predecessor,\n'
         '        "predecessor": previous, "capacity_policy": capacity,'),
        ('        "preexisting_runtime_objects_unchanged": True, "images": output_bindings,',
         '        "resource_predecessor": resource_predecessor,\n'
         '        "preexisting_runtime_objects_unchanged": True, "images": output_bindings,'),
    ]
    owner, _ = load(R2, R2_SHA, "compact_resource20_refresh", replacements)
    owner.SELF, owner.CONTRACT = SELF, CONTRACT
    owner.resource_successor = successor
    return owner


def prepare_plan(args, source, r2, r2_pin, owner, prepared, preparation, owners, deadline):
    fresh, legacy, component, _, output_base, _ = owners
    packet_pin, raw = fresh.captured(args.image_packet, args.image_packet_sha256, legacy.MAX_JSON, deadline)
    packet = legacy.strict_json(raw)
    resource_predecessor, image = successor(fresh, packet["resource_predecessor"]["path"],
                                            packet["predecessor"], packet["capacity_policy"],
                                            component.SID, prepared["binding"], deadline)
    require(packet["resource_predecessor"] == resource_predecessor, "reviewed R2 predecessor pin differs")
    execution, raw = fresh.captured(args.profile_execution, PROFILE_EXECUTION_SHA, legacy.MAX_JSON, deadline)
    profiled = legacy.strict_json(raw)
    require(profiled["complete"] is True and profiled["failure"] is None
            and profiled["whole_phase_expired"] is False and profiled["native_result"]["sha256"] == PROFILE_RESULT_SHA,
            "settled R2 profiling evidence required")
    profile_proof = profiled["native_result"]
    observed = fresh.read_checked(profile_proof, legacy.MAX_JSON, deadline)
    require(observed["policy_identity"] == POLICY_IDENTITY, "validated R2 graph policy differs")
    watch, _ = fresh.captured(WATCH, args.watchdog_sha256, fresh.MAX_SOURCE, deadline)
    original_runtime = owner.runtime_with_predecessor
    owner.runtime_with_predecessor = lambda context, first: r2.runtime_pins(
        dict(context, runtime_files=original_runtime(context, first)), image)
    plan = owner.derive(source, args, prepared, preparation, owners, deadline)
    require(plan["context"]["image_packet"] == packet_pin, "Resource20 image packet drifted during preparation")
    build = fresh.read_checked(plan["context"]["native_build_receipt"], 32 << 20, deadline)
    native_source = output_base.pin(ROOT / NATIVE_RESOURCE)
    require(build["groups"]["native"]["files"].get(NATIVE_RESOURCE) == native_source["sha256"],
            "reviewed native Resource20 implementation changed after relink")
    extra = [r2_pin, resource_predecessor, execution, profile_proof, watch, native_source]
    plan["boundary"] = owner.merge(legacy, [*plan["boundary"], *extra])
    plan["context"]["input_seals"] = {pin["path"]: pin["sha256"] for pin in plan["boundary"]}
    plan.update(watchdog=watch, profile_execution=execution, profile_result=profile_proof)
    return plan


def validate_result(value, plan, owned, lease, r2, whole_ns, journal):
    context = plan["context"]
    actual_r2 = lease.json(plan["profile_result"]["path"])
    native_input = lease.json(context["native_input"]["path"])
    mode_policy = value["policy_identity"]
    require(value["schema"] == RESULT and value["complete"] is True and value["error"] is None
            and value["qualified"] is value["product_admission"] is value["process_cuda_poisoned"] is False
            and value["pid"] == owned["owned_leader_pid"]
            and value["native_binary_sha256"] == context["native_image"]["sha256"]
            and value["execution_user"] == value["default_owner"] == context["human_sid"]
            and value["native_host_reservation_bytes"] == HOST
            and value["actual_job"] == {"process_memory_limit_bytes": HOST, "job_memory_limit_bytes": HOST,
                                        "flags": 0x2308}
            and actual_r2["policy_identity"] == POLICY_IDENTITY
            and isinstance(mode_policy, str) and len(mode_policy) == 64
            and all(x in "0123456789abcdef" for x in mode_policy)
            and mode_policy not in {POLICY_IDENTITY, "0" * 64}
            and value["input_digest"] == INPUT_DIGEST and value["applied_capacity"] == r2.APPLIED_CAPACITY
            and value["tokens"] == 8192 and value["profiling_enabled"] is False
            and value["resource_policy"] == native_input["policy"]
            and value["resource_policy_digest"] == native_input["policy_digest"]
            and value["original_whole_expires_monotonic_ns"] == whole_ns
            and value["operation_deadline_ms"] == 120000
            and Path(value["operation_journal"]) == journal
            and value["profile"] == lease.json(context["profile"]["path"])["manifest"]
            and value["bounds"] == lease.json(context["request"]["path"])["bounds"]
            and all(value[key] is True for key in ("session_teardown_observed", "native_control_cleanup_observed",
                                                   "native_host_charge_release_observed")),
            "native Resource20 identity, capacity or teardown proof differs")
    pins = {"request": context["request"], "native_input": context["native_input"],
            "source_manifest": native_input["source_manifest"],
            "runtime_manifest": context["descriptor"], "reviewed_graph": plan["graph"],
            "retained_fixture": plan["retained_fixture"], "capacity_policy": plan["capacity_policy"]}
    require(all(lease.base.core_pin_matches(value[key], pin) for key, pin in pins.items()),
            "native Resource20 input pin differs")
    series = value["resource"]
    samples = series["samples"]
    us = [row["elapsed_us"] for row in samples]
    rss = [row["peak_process_rss_bytes"] for row in samples] + [value["peak_process_rss_bytes"],
                                                               value["warmup"]["peak_process_rss_bytes"]]
    all_times = [value["load_us"], value["warmup"]["elapsed_us"], *us]
    require(series["complete"] is True and series["attempted_samples"] == len(samples) == 20
            and series["failure"] is None and series["reservation_bytes"] == HOST
            and all(type(item) is int and 0 <= item <= 120_000_000 for item in all_times)
            and all(type(item) is int and 0 < item <= HOST for item in rss)
            and series["nearest_rank_p95_us"] == sorted(us)[18]
            and series["maximum_us"] == max(us), "invalid complete 20-sample resource series")
    watch = owned["before_GO"]["operation_watch"]
    events = watch["observed_events"]
    calls = [value["load_native_call_us"], value["warmup"]["native_call_elapsed_us"],
             *value["native_call_samples_us"]]
    require(watch["actual_process_exit_observed"] is True and watch["exit_code"] == 0
            and watch["completed_return_markers"] == len(calls) == len(all_times) == 22
            and len(events) == 44 and calls == [event["native_call_elapsed_us"] for event in events[1::2]]
            and all(0 <= call <= elapsed for call, elapsed in zip(calls, all_times)),
            "native timing differs from mandatory external operation proof")
    return events


def execute(plan, plan_pin, lease, owners, r2, start_ns, started_utc):
    fresh, legacy, component, _, output_base, adapter = owners
    context = plan["context"]
    started = start_ns / 1e9
    whole_ns = start_ns + 3_600_000_000_000
    deadline = whole_ns / 1e9
    require(context["parent_python"] == output_base.pin(sys.executable)
            and component.token_owner(ctypes.c_void_p(-1)) == component.SID,
            "actual parent interpreter or Windows default owner differs")
    parent, owned = lease.base.helpers(context)
    watch_owner = lease.load(plan["watchdog"]["path"], "compact_resource20_held_watch")
    policy_owner = adapter.output_policy_owner(lease, context)
    lock = lease.json(context["output_guard_lock"]["path"])
    cpu = policy_owner.cpu_owner(lock)
    out = Path(context["output_root"])
    lock["build_path"]["fresh_child"] = out.name
    policy = policy_owner.identity_and_parent(cpu, out, lock)
    component.mkdir(context, out, deadline=deadline)
    component.mkdir_siblings(context, [out / "temp", Path(context["private_census"])], deadline=deadline)
    paths = {"native_result": out / "native-result.json", "operations": out / "native-result.operations.jsonl",
             "stdout": out / "stdout-stderr.log"}
    component.guard_many(context, [*paths.values(), out / "stdin.txt", out / "execution.json"], deadline=deadline)
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
    env.update(PATH="C:/Windows/System32;C:/Windows", TEMP=str(out / "temp"), TMP=str(out / "temp"),
               TMPDIR=str(out / "temp"), AIRK_PRIVATE_CENSUS_BASE=context["private_census"],
               AIRK_CUDA_QUALIFICATION_OUTPUT=str(paths["native_result"]),
               AIRK_CUDA_HYBRID_WHOLE_EXPIRES_NS=str(whole_ns))
    for prefix, pin in (("AIRK_CUDA_QUALIFICATION_INPUT", context["request"]),
                        ("AIRK_CUDA_HYBRID_GRAPH", plan["graph"]),
                        ("AIRK_CUDA_HYBRID_RETAINED", plan["retained_fixture"]),
                        ("AIRK_CUDA_HYBRID_CAPACITY", plan["capacity_policy"])):
        env[prefix], env[prefix + "_SHA256"] = pin["path"], pin["sha256"]
    argv = [context["native_image"]["path"], "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"]
    receipt = {"schema": "airk.private.owned-compact-bounded-hybrid-resource20/1", "plan": plan_pin,
               "command": argv, "execution_user_sid": component.SID, "parent_pid": os.getpid(),
               "parent_python": context["parent_python"], "output_policy": policy,
               "input_seals": dict(context["input_seals"], **{plan_pin["path"]: plan_pin["sha256"]}),
               "started_utc": started_utc.isoformat(), "original_deadline": (started_utc + dt.timedelta(hours=1)).isoformat(),
               "original_deadline_monotonic_ns": whole_ns, "original_deadline_monotonic": deadline,
               "whole_phase_deadline_not_refreshed": True, "no_automatic_retries": True,
               "qualification": False, "product_admission": False, "complete": False, "failure": None}
    watch = None

    def authenticate(api, job, info, child_deadline):
        nonlocal watch
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, component.SID,
                                                 context["native_image"], owned.image_proof)
        require(component.token_owner(info.process) == component.SID, "suspended native token owner differs")
        proof.update(default_owner_sid=component.SID, component_job=component.configure_job(api, job))
        profile = lease.json(context["profile"]["path"])["manifest"]
        identity = {"input_sha256": context["request"]["sha256"], "profile": profile,
                    "process_id": int(info.pid), "native_binary_sha256": context["native_image"]["sha256"]}
        state = watch_owner.Operations(identity, INPUT_DIGEST, whole_ns)
        watch = watch_owner.WindowsWatch(paths["operations"], paths["stdout"], paths["native_result"], state,
                                  lambda path: component.guard(context, path, True, deadline=deadline))
        fresh.check(deadline)
        return proof

    def observe(api, job, info, child_deadline):
        return watch.run(api, job, info, child_deadline)

    def output_pin(path):
        cap = PROOFS[next(label for label, value in paths.items() if value == path)]
        require(0 < path.stat().st_size <= cap, "required native output outside its bound")
        return output_base.pin(path)

    try:
        with (out / "stdin.txt").open("xb"):
            pass
        component.guard(context, out / "stdin.txt", True, deadline=deadline)
        fresh.check(deadline)
        with (out / "stdin.txt").open("rb") as initial, paths["stdout"].open("xb") as stdout:
            receipt["owned"] = owned.run_windows_owned_training(
                argv, ROOT, stdout, deadline - time.monotonic(), env, initial, HOST,
                before_resume=authenticate, after_resume=observe, cleanup_seconds=5, absolute_deadline=deadline)
        require(parent.settled(receipt["owned"])
                and component.finite(receipt["owned"].get("peak_host_committed_bytes"), 1, HOST),
                "owned native Job did not naturally settle within host bounds")
        component.guard_many(context, paths.values(), True, deadline=deadline)
        for label, path in paths.items():
            receipt[label] = output_pin(path)
        value = output_base.read(paths["native_result"])
        events = validate_result(value, plan, receipt["owned"], lease, r2, whole_ns, paths["operations"])
        require([legacy.strict_json(line) for line in paths["operations"].read_bytes().splitlines()] == events,
                "final operation journal differs from actual independent observations")
        require({p.name for p in out.iterdir()} == {p.name for p in paths.values()}
                | {"stdin.txt", "temp", "private-census"}, "unexpected output in unprofiled Resource20")
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["actual_cleanup_failure"] = error.owned_cleanup
        if hasattr(error, "operation_watch"):
            receipt["operation_watch_failure"] = error.operation_watch
        raise
    finally:
        prior = sys.exc_info()[1]
        try:
            if watch is not None:
                watch.close()
            require(lease.base.census(context["runtime_root"]) == context["runtime_files"], "final runtime roster drift")
            adapter.replay(context, plan["static_binding"], lease, adapter.held_preserver(lease, context))
            component.guard_many(context, [out, *out.rglob("*")], True, deadline=deadline)
        except BaseException as error:
            receipt["complete"] = False
            receipt["final_input_or_output_error"] = {"type": type(error).__name__, "message": str(error)}
            receipt["failure"] = receipt["failure"] or receipt["final_input_or_output_error"]
        receipt["final_guard_errors"], hasher = component.final_output_hasher(context, paths, deadline, output_pin)
        component.finish(receipt, paths, started, deadline, hasher, proofs=PROOFS,
                         finalizer=lambda: adapter.finish_lease(lease, receipt))
        component.write_new(context, out / "execution.json", receipt)
        if prior is None and not receipt["complete"]:
            raise ValueError(receipt["failure"])
    return receipt


def main():
    start_ns, started_utc = time.monotonic_ns(), dt.datetime.now(dt.timezone.utc)
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    if sys.argv[1:2] == ["--refresh-image"]:
        r2 = refresh_owner()
        r1_pin, raw = r2.code(r2.R1, r2.R1_SHA)
        r2.refresh(r2.load(r1_pin, raw, "compact_frozen_r1"), sys.argv[2:], start_ns / 1e9, started_utc)
        return
    r2, r2_pin = load(R2, R2_SHA, "compact_frozen_r2")
    r1_pin, raw = r2.code(r2.R1, r2.R1_SHA)
    owner = r2.reviewed_launcher(r1_pin, raw, (start_ns / 1e9, started_utc))
    owner.SCHEMA, owner.SELECTOR = SCHEMA, SELECTOR
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--prepare-plan", action="store_true")
    mode.add_argument("--execute-reviewed-plan", action="store_true")
    for name in ("plan", "approved-script-sha256", "approved-contract-sha256"):
        parser.add_argument("--" + name, required=True)
    for name in ("plan-sha256", "graph-preparation", "graph-execution", "image-packet",
                 "image-packet-sha256", "supplemental-fixture", "output-root",
                 "capacity-policy-sha256", "profile-execution", "watchdog-sha256"):
        parser.add_argument("--" + name)
    args = parser.parse_args()
    deadline = start_ns / 1e9 + (600 if args.prepare_plan else 3600)
    source = [owner.bootstrap(SELF, args.approved_script_sha256)[0],
              owner.bootstrap(CONTRACT, args.approved_contract_sha256)[0]]
    if args.prepare_plan:
        require(all(getattr(args, name) for name in ("graph_preparation", "graph_execution", "image_packet",
                    "image_packet_sha256", "supplemental_fixture", "output_root",
                    "capacity_policy_sha256", "profile_execution", "watchdog_sha256")),
                "complete Resource20 preparation inputs required")
        prep, raw = owner.bootstrap(args.graph_preparation, owner.PREPARATION_SHA)
    else:
        plan_pin, raw = owner.bootstrap(args.plan, args.plan_sha256)
        plan = json.loads(raw)
        require(plan["schema"] == SCHEMA and plan["selector"] == SELECTOR
                and [plan["launcher"], plan["contract"]] == source, "reviewed Resource20 plan differs")
        prep, raw = owner.bootstrap(plan["retained_preparation"]["path"], owner.PREPARATION_SHA)
        require(plan["retained_preparation"] == prep, "reviewed static preparation differs")
    prepared = json.loads(raw)
    owners = owner.make_owners(prepared["plan"], deadline)
    component = owners[2]
    component.guard(prepared["plan"], deadline=deadline)
    if args.prepare_plan:
        plan = prepare_plan(args, source, r2, r2_pin, owner, prepared, prep, owners, deadline)
        with owner.acquire(plan, None, prepared, owners, deadline) as lease:
            lease.refresh()
        component.write_new(plan["context"], Path(args.plan), plan)
        result = owners[3].pin(args.plan)
    else:
        with owner.acquire(plan, plan_pin, prepared, owners, deadline) as lease:
            result = execute(plan, plan_pin, lease, owners, r2, start_ns, started_utc)
        result = {"complete": result["complete"], "elapsed_seconds": result["elapsed_seconds"],
                  "native_result": result["native_result"]}
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
