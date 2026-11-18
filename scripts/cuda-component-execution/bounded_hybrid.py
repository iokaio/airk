# SPDX-License-Identifier: Apache-2.0
"""Run one privately owned Windows Compact four-width CUDA profiling diagnostic."""

import argparse
import copy
import ctypes
import datetime as dt
import hashlib
import os
from pathlib import Path
import sys
import time
import types

sys.dont_write_bytecode = True
SELF = Path(__file__).resolve()
HERE = SELF.parent
ROOT = HERE.parents[1]
CONTRACT = HERE / "BOUNDED-HYBRID.md"
SELECTOR = ("inference_engine::pool::arbiter::qualification::software_v2::"
            "cuda_qualification::bounded_hybrid::cuda_compact_fp32_bounded_hybrid_profile")
SCHEMA = "airk.private.compact-bounded-hybrid-plan/1"
RESULT = "airk.private.cuda-compact-bounded-hybrid-profile/1"
PREPARATION_SHA = "c919d0c9ede15d299d5b1562e2c27685e05e2748875d407e0df91e04c16d00e5"
GRAPH_EXECUTION_SHA = "28dabc469854c848ec55b8007c55ce401470c92a922e14b23e5f58dae9e3f455"
GRAPH_SHA = "2f4d3211e0bbf097c702acf90dcce046552a3e2d75dc49b2f5945d7988c76fee"
RETAINED_SHA = "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd"
SUPPLEMENTAL_SHA = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9"
HOST = 32 << 30
LINK_FIELDS = {"build_receipt", "cargo_json", "actual_static_ORT_rows", "image_exports"}
HYBRID_SOURCES = (
    "crates/airk-retrieval/src/arbiter/runtime/bounded_hybrid.rs",
    "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/bounded_hybrid.rs",
    "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification/hybrid_profile_trace.rs",
)
PROOFS = {"native_result": 16 << 20, "operations": 1 << 20,
          "stdout": 64 << 20, "runtime_profile": 64 << 20}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def bootstrap(path, sha):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            "absolute canonical bootstrap file required")
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == sha,
            "approved bootstrap pin differs: " + str(path))
    return {"path": str(path), "bytes": len(raw), "sha256": sha}, raw


def module(pin, raw, name):
    loaded = types.ModuleType(name)
    loaded.__file__ = pin["path"]
    exec(compile(raw, pin["path"], "exec", dont_inherit=True), loaded.__dict__)
    return loaded


def make_owners(old_plan, deadline):
    fresh_pin, raw = bootstrap(HERE / "fresh_graph.py", old_plan["graph_provenance_owner"]["sha256"])
    require(fresh_pin == old_plan["graph_provenance_owner"], "retained fresh graph owner differs")
    fresh = module(fresh_pin, raw, "hybrid_fresh_graph")
    roster_pin = old_plan["fresh_bootstrap"]
    roster = fresh.read_checked(roster_pin, fresh.MAX_SOURCE, deadline)
    require(set(roster) == set(fresh.SOURCES), "retained source roster differs")
    approved, content = {"roster": roster_pin}, {}
    for role, path in fresh.SOURCES.items():
        approved[role], content[role] = fresh.captured(path, roster[role], fresh.MAX_SOURCE, deadline)
    policy = old_plan["compact_host_policy"]
    actual, raw = fresh.captured(HERE / "compact_graph_host.py", policy["sha256"], fresh.MAX_SOURCE, deadline)
    require(actual == policy, "exact retained 32 GiB Compact Job adapter required")
    approved["memory_policy"] = policy
    job = module(policy, raw, "hybrid_compact_job_policy")
    component = job.load_graph_runner(fresh.SOURCES["component"], content["component"])
    legacy = module(approved["lease"], content["lease"], "hybrid_graph_lease")
    base = module(approved["native"], content["native"], "hybrid_native_owner")
    output_base = module(approved["native"], content["native"], "hybrid_bounded_output")
    component.base = lambda: base
    component.bind_graph_hash_deadline(output_base, deadline)
    adapter = fresh.FreshGraph(component, legacy, output_base, approved, fresh_pin,
                               old_plan["graph_provenance_contract"], None)
    return fresh, legacy, component, base, output_base, adapter


def merge(legacy, items):
    pins = {}
    for pin in items:
        legacy.file_pin(pin)
        key = legacy.key(pin["path"])
        require(key not in pins or pins[key] == pin, "conflicting held inputs")
        pins[key] = pin
    return [pins[key] for key in sorted(pins)]


def plan_binding(new_packet, old_binding):
    binding = new_packet["static_graph_binding"]
    require(set(binding) == set(old_binding)
            and all(binding[key] == old_binding[key] for key in binding.keys() - LINK_FIELDS),
            "relinked image changes the retained /6 static producer, archive, manifest or runtime")
    return binding


def selected_rows(retained, supplemental):
    def widths(fixture):
        return [len(row["input"]["input_ids"]) for row in fixture["rows"]]

    require(widths(retained) == [17, 128, 8192] and widths(supplemental) == [32, 1024],
            "exact retained and supplemental widths required")
    require(all(retained[key] == supplemental[key]
                for key in ("schema", "model", "compiler", "resource_policy", "original_reference_assets")),
            "supplemental fixture model, compiler or original-source identity differs")
    return [supplemental["rows"][0], retained["rows"][1],
            supplemental["rows"][1], retained["rows"][2]]


def derive(source, args, prepared, preparation_pin, owners, deadline):
    fresh, legacy, component, base, output_base, adapter = owners
    # Authenticate the settled retained preparation before selecting its static evidence.
    archived_plan = legacy.execution_plan(prepared, preparation_pin)
    graph_execution, raw = fresh.captured(args.graph_execution, GRAPH_EXECUTION_SHA, legacy.MAX_JSON, deadline)
    execution = legacy.strict_json(raw)
    earlier_plan = execution["plan"]
    require(execution["complete"] is True and execution["failure"] is None
            and execution["graph_metadata_observed"] is True
            and execution["whole_phase_expired"] is False
            and fresh.read_checked(earlier_plan, legacy.MAX_JSON, deadline) == archived_plan,
            "exact settled Compact graph history required")
    graph = execution["native_result"]
    require(graph["sha256"] == GRAPH_SHA, "reviewed native graph differs")
    packet_pin, raw = fresh.captured(args.image_packet, args.image_packet_sha256, legacy.MAX_JSON, deadline)
    packet = legacy.strict_json(raw)
    binding = plan_binding(packet, prepared["binding"])
    build = packet["actual_build_receipt"]
    build_value = fresh.read_checked(build, 32 << 20, deadline)
    files = build_value["groups"]["native"]["files"]
    images = [entry for entry in packet["images"] if entry["binary_id"] == "airk-inference"]
    require(len(images) == 1, "exactly one new preserved Inference image required")
    image = {key: images[0][key] for key in ("path", "bytes", "sha256")}
    plan = copy.deepcopy(archived_plan)
    out = Path(args.output_root)
    require(out.is_absolute() and out.parent == component.ANCHOR and not out.exists(),
            "fresh direct approved private output required")
    require(image["sha256"] != plan["native_image"]["sha256"]
            and Path(image["path"]).parent == Path(plan["runtime_root"]),
            "newly linked Inference image must be beside the reviewed runtime manifest")
    runtime = sorted([*plan["runtime_files"], image], key=lambda row: row["path"])
    require(output_base.census(plan["runtime_root"]) == runtime,
            "runtime differs beyond the single additive native image")
    metadata = fresh.read_checked(plan["metadata_receipt"], legacy.MAX_JSON, deadline)
    old = fresh.read_checked(metadata["plan"], legacy.MAX_JSON, deadline)
    old_build = fresh.read_checked(old["native_build_receipt"], 32 << 20, deadline)
    old_files = old_build["groups"]["native"]["files"]
    required = component.rebind_native_sources(old["required_native_sources"], old_files, files)
    plan.update(image_packet=packet_pin, native_build_receipt=build, native_image=image,
                native_group=packet["native_group"], runtime_files=runtime,
                output_root=str(out), private_census=str(out / "private-census"),
                required_native_sources=required,
                native_source_introductions=component.native_source_introductions(old_files, required),
                native_source_transition=component.native_source_transition(old["input_seals"], old_files, required))
    for relative, sha in required.items():
        plan["input_seals"][str(ROOT / relative)] = sha
    for pin in (packet_pin, build, image, binding["cargo_json"]):
        plan["input_seals"][pin["path"]] = pin["sha256"]
    request = fresh.read_checked(plan["native_input"], legacy.MAX_JSON, deadline)
    retained = output_base.pin(request["numeric_fixture"]["path"])
    require(retained["sha256"] == request["numeric_fixture"]["sha256"] == RETAINED_SHA,
            "exact retained Compact independent references required")
    supplemental, raw = fresh.captured(args.supplemental_fixture, SUPPLEMENTAL_SHA, legacy.MAX_JSON, deadline)
    second = legacy.strict_json(raw)
    selected_rows(fresh.read_checked(retained, legacy.MAX_JSON, deadline), second)
    supplemental_producer = output_base.pin(second["reference_producer"]["path"])
    require(supplemental_producer["sha256"] == second["reference_producer"]["sha256"],
            "supplemental producer pin differs")
    require(set(HYBRID_SOURCES) <= files.keys(), "hybrid sources missing from relink")
    addon_sources = [output_base.pin(ROOT / relative) for relative in HYBRID_SOURCES]
    require(all(files[relative] == row["sha256"] for relative, row in zip(HYBRID_SOURCES, addon_sources)),
            "reviewed Inference or Retrieval hybrid source changed after relink")
    adapter.identify(plan)
    explicit = [*source, preparation_pin, graph_execution, earlier_plan, graph, supplemental,
                supplemental_producer, *addon_sources]
    read = lambda pin, limit: fresh.read_checked(pin, limit, deadline)
    derived = fresh.derive_boundary(legacy, plan, binding, output_base, read)
    pins = merge(legacy, [*derived, *explicit])
    plan["input_seals"] = {pin["path"]: pin["sha256"] for pin in pins}
    return {"schema": SCHEMA, "selector": SELECTOR, "context": plan, "static_binding": binding,
            "graph": graph, "retained_fixture": retained, "supplemental_fixture": supplemental,
            "retained_preparation": preparation_pin, "launcher": source[0], "contract": source[1],
            "boundary": pins}


def acquire(plan, plan_pin, prepared, owners, deadline):
    fresh, legacy, component, base, output_base, adapter = owners
    context = plan["context"]
    retained = {legacy.key(row["pin"]["path"]): row for row in prepared["boundary"]}
    pins = merge(legacy, [*plan["boundary"], *([plan_pin] if plan_pin else [])])
    expected = {legacy.key(pin["path"]): retained[legacy.key(pin["path"])] for pin in pins
                if legacy.key(pin["path"]) in retained and retained[legacy.key(pin["path"])]["pin"] == pin}
    lease = legacy.Lease(context, pins, deadline, expected=expected)
    try:
        if plan_pin:
            require(lease.json(plan_pin["path"]) == plan, "held hybrid plan differs")
        require(lease.json(plan["retained_preparation"]["path"]) == prepared,
                "held /6 preparation differs")
        lease.base = legacy.execution_base(lease)
        lease.plan_pin = plan_pin
        packet = lease.json(context["image_packet"]["path"])
        binding = plan_binding(packet, prepared["binding"])
        require(binding == plan["static_binding"], "held static binding differs")
        adapter.replay(context, binding, lease, adapter.held_preserver(lease, context))
        component.preflight(context, lease.base, before=True, deadline=deadline, graph_lease=lease)
        return lease
    except BaseException:
        lease.close()
        raise


def journal(events, value):
    require(len(events) == 10, "exact load and four numeric operation pairs required")
    identity = {"input_sha256": value["request"]["sha256"], "profile": value["profile"],
                "process_id": value["pid"], "native_binary_sha256": value["native_binary_sha256"]}
    clock_expiry = value["original_expires_monotonic_ns"]
    previous = 0
    for index, event in enumerate(events):
        ordinal, phase = divmod(index, 2)
        begin, now, expiry = (event[key] for key in
                              ("started_monotonic_ns", "observed_monotonic_ns", "expires_monotonic_ns"))
        require(event["schema"] == "airk.arbiter.native-operation/1"
                and event["sequence"] == index + 1 and event["identity"] == identity
                and event["kind"] == ("load" if ordinal == 0 else "numeric")
                and event["ordinal"] == ordinal and event["phase"] == ("started", "returned")[phase]
                and event["clock"] == "windows_performance_counter"
                and event["operation_input_digest"] == (None if ordinal == 0 else value["rows"][ordinal-1]["input_digest"])
                and all(type(item) is int for item in (begin, now, expiry))
                and previous <= now and 0 < begin <= now < expiry == clock_expiry,
                "native operation identity, clock or order differs")
        if phase == 0:
            require(now == begin and event["native_success"] is None
                    and event["native_call_elapsed_us"] is None, "native start observation differs")
        else:
            require(begin == events[index-1]["started_monotonic_ns"] and event["native_success"] is True
                    and type(event["native_call_elapsed_us"]) is int
                    and 0 <= event["native_call_elapsed_us"] <= 120_000_000
                    and (ordinal == 0 or event["native_call_elapsed_us"] == value["rows"][ordinal-1]["native_call_elapsed_us"]),
                    "native return proof differs")
        previous = now


def validate_result(value, plan, lease, owned, profile_pin, component):
    context = plan["context"]
    require(value["schema"] == RESULT and value["complete"] is True and value.get("error") is None
            and value["qualified"] is value["product_admission"] is value["process_cuda_poisoned"] is False
            and value["pid"] == owned["owned_leader_pid"]
            and value["native_binary_sha256"] == context["native_image"]["sha256"]
            and value["execution_user"] == value["default_owner"] == component.SID
            and value["native_host_reservation_bytes"] == HOST
            and value["actual_job"] == {"process_memory_limit_bytes": HOST,
                                        "job_memory_limit_bytes": HOST, "flags": 0x2308}
            and value["profile"] == lease.json(context["profile"]["path"])["manifest"]
            and component.finite(value["peak_process_rss_bytes"], 1, HOST)
            and all(value[key] is True for key in ("session_teardown_observed",
                    "native_control_cleanup_observed", "native_host_charge_release_observed")),
            "native identity, resource or teardown proof differs")
    for label, expected in (("request", context["request"]), ("native_input", context["native_input"]),
                            ("runtime_manifest", context["descriptor"]),
                            ("reviewed_graph", plan["graph"]), ("retained_fixture", plan["retained_fixture"]),
                            ("supplemental_fixture", plan["supplemental_fixture"]),
                            ("runtime_profile", profile_pin)):
        require(lease.base.core_pin_matches(value[label], expected), "native pin differs: " + label)
    require(value["runtime_profile"]["bytes"] == profile_pin["bytes"], "runtime profile length differs")
    request = lease.json(context["request"]["path"])
    require(value["bounds"] == request["bounds"], "native resource bounds differ from the reviewed request")
    fixtures = [lease.json(plan[key]["path"]) for key in ("retained_fixture", "supplemental_fixture")]
    rows = selected_rows(*fixtures)
    require(len(value["rows"]) == 4
            and [run["tokens"] for run in value["profiled_runs"]] == [32, 128, 1024, 8192],
            "four ordered native profiled forwards required")
    for expected, actual in zip(rows, value["rows"]):
        require(actual["tokens"] == len(expected["input"]["input_ids"])
                and actual["name"] == expected["name"]
                and actual["input_digest"] == expected["input"]["input_digest"]
                and len(actual["raw_scores"]) == 8
                and all(component.finite(score, -float("inf")) for score in actual["raw_scores"])
                and actual["argmax_matches"] is True
                and all(component.finite(actual[key], 0, limit) for key, limit in (
                    ("same_artifact_raw_error", .001), ("original_fp32_raw_error", .001),
                    ("same_artifact_mass_error", .05), ("original_fp32_mass_error", .05))),
                "one or more independently compared native rows failed")


def execute(plan, plan_pin, lease, owners, clock):
    fresh, legacy, component, base, output_base, adapter = owners
    started, started_utc, deadline_utc, deadline = clock
    context = plan["context"]
    require(context["parent_python"] == output_base.pin(sys.executable)
            and component.token_owner(ctypes.c_void_p(-1)) == component.SID,
            "parent interpreter or Windows token default owner differs")
    parent, owner = lease.base.helpers(context)
    policy_owner = adapter.output_policy_owner(lease, context)
    lock = lease.json(context["output_guard_lock"]["path"])
    cpu = policy_owner.cpu_owner(lock)
    out = Path(context["output_root"])
    lock["build_path"]["fresh_child"] = out.name
    policy = policy_owner.identity_and_parent(cpu, out, lock)
    component.mkdir(context, out, deadline=deadline)
    component.mkdir_siblings(context, [out / "temp", Path(context["private_census"])], deadline=deadline)
    profile_prefix = out / "ort-profile"
    paths = {"native_result": out / "native-result.json",
             "operations": out / "native-result.operations.jsonl",
             "stdout": out / "stdout-stderr.log", "runtime_profile": profile_prefix}
    component.guard_many(context, [*paths.values(), out / "stdin.txt", out / "execution.json"], deadline=deadline)
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith(("AIRK_", "ORT_", "PYTHON", "CUDA_", "CUDNN_"))}
    env.update(PATH="C:/Windows/System32;C:/Windows", TEMP=str(out / "temp"),
               TMP=str(out / "temp"), TMPDIR=str(out / "temp"),
               AIRK_PRIVATE_CENSUS_BASE=context["private_census"],
               AIRK_CUDA_QUALIFICATION_OUTPUT=str(paths["native_result"]),
               AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX=str(profile_prefix))
    for prefix, pin in (("AIRK_CUDA_QUALIFICATION_INPUT", context["request"]),
                        ("AIRK_CUDA_HYBRID_GRAPH", plan["graph"]),
                        ("AIRK_CUDA_HYBRID_RETAINED", plan["retained_fixture"]),
                        ("AIRK_CUDA_HYBRID_SUPPLEMENTAL", plan["supplemental_fixture"])):
        env[prefix], env[prefix + "_SHA256"] = pin["path"], pin["sha256"]
    argv = [context["native_image"]["path"], "--exact", SELECTOR, "--ignored", "--nocapture", "--test-threads=1"]
    receipt = {"schema": "airk.private.owned-compact-bounded-hybrid-profile/1", "plan": plan_pin,
               "command": argv, "execution_user_sid": component.SID, "parent_pid": os.getpid(),
               "parent_python": context["parent_python"], "output_policy": policy,
               "input_seals": dict(context["input_seals"], **{plan_pin["path"]: plan_pin["sha256"]}),
               "started_utc": started_utc.isoformat(), "original_deadline": deadline_utc.isoformat(),
               "original_deadline_monotonic": deadline, "whole_phase_deadline_not_refreshed": True,
               "qualification": False, "product_admission": False, "complete": False, "failure": None}

    def authenticate(api, job, info, child_deadline):
        proof = parent.suspended_authentication(api, info, deadline, child_deadline, component.SID,
                                                context["native_image"], owner.image_proof)
        require(component.token_owner(info.process) == component.SID, "suspended native token owner differs")
        proof["default_owner_sid"] = component.SID
        proof["component_job"] = component.configure_job(api, job)
        fresh.check(deadline)
        return proof

    def output_pin(path):
        require(0 < path.stat().st_size <= PROOFS[next(label for label, value in paths.items() if value == path)],
                "native output size outside retained bound")
        return output_base.pin(path)

    try:
        with (out / "stdin.txt").open("xb"):
            pass
        component.guard(context, out / "stdin.txt", True, deadline=deadline)
        fresh.check(deadline)
        with (out / "stdin.txt").open("rb") as initial, paths["stdout"].open("xb") as stdout:
            receipt["owned"] = owner.run_windows_owned_training(
                argv, ROOT, stdout, deadline - time.monotonic(), env, initial, HOST,
                before_resume=authenticate, cleanup_seconds=5, absolute_deadline=deadline)
        require(parent.settled(receipt["owned"])
                and component.finite(receipt["owned"].get("peak_host_committed_bytes"), 1, HOST),
                "owned native Job did not naturally settle within host bounds")
        profiles = list(out.glob(profile_prefix.name + "*.json"))
        require(len(profiles) == 1, "exactly one native ORT profile required")
        paths["runtime_profile"] = profiles[0]
        component.guard_many(context, paths.values(), True, deadline=deadline)
        for label, path in paths.items():
            receipt[label] = output_pin(path)
        value = output_base.read(paths["native_result"])
        validate_result(value, plan, lease, receipt["owned"], receipt["runtime_profile"], component)
        events = [legacy.strict_json(line) for line in paths["operations"].read_bytes().splitlines()]
        journal(events, value)
        receipt["complete"] = True
    except BaseException as error:
        receipt["failure"] = {"type": type(error).__name__, "message": str(error)}
        if hasattr(error, "owned_cleanup"):
            receipt["actual_cleanup_failure"] = error.owned_cleanup
        raise
    finally:
        failed = sys.exc_info()[1]
        try:
            require(lease.base.census(context["runtime_root"]) == context["runtime_files"],
                    "final runtime roster drift")
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
        if failed is None and not receipt["complete"]:
            raise ValueError(receipt["failure"])
    return receipt


def main():
    started, started_utc = time.monotonic(), dt.datetime.now(dt.timezone.utc)
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--prepare-plan", action="store_true")
    mode.add_argument("--verify-plan", action="store_true")
    mode.add_argument("--execute-reviewed-plan", action="store_true")
    for name in ("plan", "approved-script-sha256", "approved-contract-sha256"):
        parser.add_argument("--" + name, required=True)
    for name in ("plan-sha256", "graph-preparation", "graph-execution", "image-packet",
                 "image-packet-sha256", "supplemental-fixture", "output-root"):
        parser.add_argument("--" + name)
    args = parser.parse_args()
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    window = 600 if args.prepare_plan else 120
    cutoff = started_utc + dt.timedelta(seconds=window)
    deadline = started + window
    source = [bootstrap(SELF, args.approved_script_sha256)[0],
              bootstrap(CONTRACT, args.approved_contract_sha256)[0]]
    import json
    if args.prepare_plan:
        require(all(getattr(args, name) for name in ("graph_preparation", "graph_execution",
                    "image_packet", "image_packet_sha256", "supplemental_fixture", "output_root")),
                "complete preparation inputs required")
        preparation_pin, raw = bootstrap(args.graph_preparation, PREPARATION_SHA)
        prepared = json.loads(raw)
        owners = make_owners(prepared["plan"], deadline)
        component = owners[2]
        component.guard(prepared["plan"], deadline=deadline)
        plan = derive(source, args, prepared, preparation_pin, owners, deadline)
        with acquire(plan, None, prepared, owners, deadline) as lease:
            lease.refresh()
        component.write_new(plan["context"], Path(args.plan), plan)
        result = owners[3].pin(args.plan)
    else:
        plan_pin, raw = bootstrap(args.plan, args.plan_sha256)
        plan = json.loads(raw)
        require(plan["schema"] == SCHEMA and plan["selector"] == SELECTOR
                and [plan["launcher"], plan["contract"]] == source, "reviewed hybrid plan scope differs")
        preparation_pin, raw = bootstrap(plan["retained_preparation"]["path"], PREPARATION_SHA)
        require(plan["retained_preparation"] == preparation_pin, "retained preparation pin differs")
        prepared = json.loads(raw)
        owners = make_owners(prepared["plan"], deadline)
        owners[2].guard(plan["context"], deadline=deadline)
        with acquire(plan, plan_pin, prepared, owners, deadline) as lease:
            if args.verify_plan:
                lease.refresh()
                result = {"verified_unqualified": True, "elapsed_seconds": time.monotonic() - started}
            else:
                result = execute(plan, plan_pin, lease, owners, (started, started_utc, cutoff, deadline))
                result = {key: result[key] for key in ("complete", "elapsed_seconds", "runtime_profile")}
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
