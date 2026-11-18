# SPDX-License-Identifier: Apache-2.0
"""Refresh a Compact CUDA image from the held /6 lineage and run its reviewed profile."""

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SELF = Path(__file__).resolve()
CONTRACT = HERE / "BOUNDED-HYBRID-R2.md"
R1 = HERE / "bounded_hybrid.py"
CAPACITY = HERE / "compact-capacity-r2.json"
CAPACITY_SHA = "071891ef57f12067bd386ccb0230f5018eb2a1143109d6aeeb0db731eab50586"
APPLIED_CAPACITY = {"arena_limit_bytes": 22 << 30, "warm_extra_bytes": 16 << 30,
                    "headroom_bytes": 2 << 30}
R1_SHA = "16c0e5e26b977b898454903a29bfc5c0d3e00ca2b83d394a2d044650052adc66"
R1_PACKET_SHA = "d00ffee78bf9063058b70bdc389c45f274b8973f7b38b8957c625c066eb1e911"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def code(path, sha):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            "canonical maintained launcher source required")
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == sha,
            "approved launcher source differs")
    return {"path": str(path), "bytes": len(raw), "sha256": sha}, raw


def load(pin, raw, name):
    owner = types.ModuleType(name)
    owner.__file__ = pin["path"]
    exec(compile(raw, pin["path"], "exec", dont_inherit=True), owner.__dict__)
    return owner


def core(row):
    return {key: row[key] for key in ("path", "bytes", "sha256")}


def inference_image(packet, sid):
    images = [row for row in packet["images"] if row["binary_id"] == "airk-inference"]
    require(packet["schema"] == "airk.preserved-native-test-images/1" and len(images) == 1
            and images[0]["original_file_owner_sid"] == images[0]["preserved_file_owner_sid"] == sid,
            "one same-human preserved Inference image required")
    return core(images[0])


def runtime_pins(seed, predecessor):
    pins = sorted([*seed["runtime_files"], predecessor], key=lambda row: row["path"])
    require(len({Path(row["path"]) for row in pins}) == len(pins)
            and all(Path(row["path"]).parent == Path(seed["runtime_root"]) for row in pins),
            "exact additive predecessor and inherited runtime required")
    return pins


def validate_runtime_rows(rows, expected):
    require(sorted([row["pin"] for row in rows], key=lambda row: row["path"]) == expected,
            "runtime differs from the complete original roster plus the exact R1 image")


def current_sources(receipt, preserver, deadline):
    files = receipt["groups"]["native"]["files"]
    require(isinstance(files, dict) and files, "actual native source roster missing")
    for relative, sha in files.items():
        path = Path(relative)
        require(not path.is_absolute() and not path.drive and path.as_posix() == relative
                and all(part not in (".", "..") for part in path.parts),
                "noncanonical native build source path")
        preserver.exact_pin(ROOT / path, sha, deadline=deadline)


def refresh(r1, argv, started, started_utc):
    parser = argparse.ArgumentParser(description="Preserve the newly linked Compact CUDA images.")
    for name in ("approved-script-sha256", "approved-contract-sha256", "graph-preparation",
                 "predecessor-packet", "receipt", "receipt-sha256", "log", "log-sha256",
                 "native-group", "source-commit", "output", "inference-name", "capacity-policy-sha256"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args(argv)
    deadline = started + 600
    launcher, _ = code(SELF, args.approved_script_sha256)
    contract, _ = code(CONTRACT, args.approved_contract_sha256)
    prep, raw = r1.bootstrap(args.graph_preparation, r1.PREPARATION_SHA)
    prepared = json.loads(raw)
    fresh, legacy, component, _, _, adapter = r1.make_owners(prepared["plan"], deadline)
    seed = legacy.execution_plan(prepared, prep)
    adapter.identify(seed)
    component.guard(seed, deadline=deadline)
    previous, raw = fresh.captured(args.predecessor_packet, R1_PACKET_SHA, legacy.MAX_JSON, deadline)
    old_packet = legacy.strict_json(raw)
    static = r1.plan_binding(old_packet, prepared["binding"])
    previous_image = inference_image(old_packet, component.SID)
    expected = runtime_pins(seed, previous_image)
    capacity = capacity_pin(args.capacity_policy_sha256, fresh, deadline)

    preserver_pin, raw = fresh.captured(static["preserver_source"]["path"],
                                        static["preserver_source"]["sha256"], fresh.MAX_SOURCE, deadline)
    require(preserver_pin == static["preserver_source"], "reviewed preserver pin differs")
    preserver = load(preserver_pin, raw, "compact_r2_preserver")
    # Preserve the existing guard/copy implementations. Bound their digest reads
    # with the current invocation's original clock.
    pin_file = preserver.pin
    preserver.pin = lambda path, **_: pin_file(path, deadline=deadline)
    guard_sha = seed["checked_guard"]["sha256"]
    build = preserver.exact_pin(args.receipt, args.receipt_sha256, deadline=deadline)
    cargo = preserver.exact_pin(args.log, args.log_sha256, deadline=deadline)
    receipt = preserver.fresh_json(build, 32 << 20, deadline)
    preserver.validate_receipt(receipt, args.native_group, Path(cargo["path"]).name)
    head = preserver.subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
                                                timeout=min(10, deadline - time.monotonic())).strip()
    require(head == args.source_commit, "reviewed source commit differs")
    current_sources(receipt, preserver, deadline)
    cargo_text = preserver.fresh_bytes(cargo, 64 << 20, deadline).decode("utf-8")
    ort, selected = preserver.cargo_rows(cargo_text, Path(static["archive"]["path"]))
    target = Path(receipt["target_directory"]).resolve(strict=True)
    originals, exports = {}, {}
    for binary, row in selected:
        path = preserver.regular(row["executable"])
        require(path.is_relative_to(target) and 0 < path.stat().st_size <= 4 << 30,
                "actual Cargo artifact outside its recorded target/size")
        preserver.pe(path)
        original = preserver.snapshot(path)
        originals[binary] = original
        exports[original["pin"]["sha256"]] = preserver.exported_collectors(path, deadline=deadline)
    source_inference = originals["airk-inference"]["pin"]
    require(source_inference["sha256"] not in {row["sha256"] for row in expected},
            "independent newly linked Inference image required")
    binding = dict(static, build_receipt=build, cargo_json=cargo,
                   actual_static_ORT_rows=ort, image_exports=exports)

    out, runtime = Path(args.output), Path(seed["runtime_root"])
    preserver.validate_destinations(out, runtime, args.inference_name)
    inference_out = runtime / args.inference_name
    preserver.guard(out, preserver.ARTIFACT_ROOT, guard_sha)
    preserver.guard(inference_out, preserver.USER_ROOT, guard_sha)
    runtime_policy = preserver.policy(runtime)
    preserver.owner_policy(runtime_policy, payload=True)
    before = preserver.runtime_census(runtime, guard_sha)
    validate_runtime_rows(before, expected)

    # Reopen only the authenticated current /6 replay inputs and reviewed runtime.
    # Retained build-tree derivation was completed in the immutable preparation.
    static_inputs = [binding[key] for key in fresh.BINDING_PINS]
    pins = r1.merge(legacy, [*static_inputs, *binding["source_pins"], *expected,
                            prep, previous, launcher, contract, capacity, seed["powershell"],
                            seed["checked_guard"], *[row["pin"] for row in originals.values()]])
    old_rows = {legacy.key(row["pin"]["path"]): row for row in prepared["boundary"]}
    expected_held = {legacy.key(pin["path"]): old_rows[legacy.key(pin["path"])] for pin in pins
                     if legacy.key(pin["path"]) in old_rows and old_rows[legacy.key(pin["path"])]["pin"] == pin}
    output_bindings = []
    later_addition = None
    with legacy.Lease(seed, pins, deadline, expected=expected_held) as lease:
        require(lease.json(prep["path"]) == prepared and lease.json(previous["path"]) == old_packet,
                "held antecedent evidence differs")
        context = dict(seed, native_image=source_inference, native_build_receipt=build)
        adapter.replay(context, binding, lease, adapter.held_preserver(lease, context))
        preserver.make_directory(out, preserver.ARTIFACT_ROOT, guard_sha)
        copied_images = out / "images"
        preserver.make_directory(copied_images, preserver.ARTIFACT_ROOT, guard_sha)
        for binary, row in selected:
            inference = binary == "airk-inference"
            destination = inference_out if inference else copied_images / Path(row["executable"]).name
            output_root = preserver.USER_ROOT if inference else preserver.ARTIFACT_ROOT
            fresh.check(deadline)
            copied = preserver.copy_one(originals[binary], destination, output_root, guard_sha, payload=inference)
            machine = preserver.pe(destination)
            if inference:
                later_addition = {"name": destination.name, **copied}
            image = {"binary_id": binary, **copied["pin"], "original_path": row["executable"],
                     "original_file_owner_sid": component.SID, "preserved_file_owner_sid": component.SID}
            output_bindings.append({"materialized_image": image, "actual_cargo_artifact": row,
                                    "pe": machine, "original": originals[binary], "preserved": copied})
        require(later_addition is not None, "additive Inference copy missing")
        preserver.unchanged_runtime(before, preserver.runtime_census(runtime, guard_sha), later_addition)
        require(preserver.policy(runtime) == runtime_policy, "runtime parent policy changed")
        current_sources(receipt, preserver, deadline)
        require(preserver.subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
                                                   timeout=min(10, deadline - time.monotonic())).strip() == head,
                "reviewed source commit changed during refresh")
        lease.refresh()
        held = lease.records()
    fresh.check(deadline)

    inventory = {
        "schema": "airk.preserved-native-test-images/1", "utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "source_commit": head, "native_group": args.native_group, "actual_build_receipt": build,
        "images": [row["materialized_image"] for row in output_bindings], "static_graph_binding": binding,
        "predecessor": previous, "capacity_policy": capacity, "qualification": False,
        "native_model_request_dispatched": False,
        "scope": "Independent held /6 Compact relink copies with one additive Inference filename.",
    }
    packet = out / "native-test-images.json"
    proof = out / "cargo-image-bindings.json"
    preserver.write_json(packet, inventory, preserver.ARTIFACT_ROOT, guard_sha)
    packet_pin = preserver.pin(packet)
    preserver.write_json(proof, {
        "schema": "airk.private.held-cuda-image-refresh/1", "inventory": packet_pin,
        "build_receipt": build, "cargo_json": cargo, "static_binding": binding,
        "reviewed_preparation": prep, "predecessor": previous, "launcher": launcher, "contract": contract,
        "capacity_policy": capacity,
        "runtime_source_census": before, "runtime_addition": later_addition,
        "preexisting_runtime_objects_unchanged": True, "images": output_bindings,
        "boundary": held, "opened_handles": len(lease.files), "closed_handles": lease.closed_count,
        "kept_original_deadline": True, "opened_utc": started_utc.isoformat(),
        "elapsed_seconds_to_packet": time.monotonic() - started, "qualification": False,
        "native_model_request_dispatched": False,
    }, preserver.ARTIFACT_ROOT, guard_sha)
    print(json.dumps({"inventory": packet_pin, "proof": preserver.pin(proof),
                      "qualification": False}, allow_nan=False))


def capacity_pin(sha, fresh, deadline):
    require(sha == CAPACITY_SHA, "exact approved R2 capacity policy SHA required")
    return fresh.captured(CAPACITY, sha, fresh.MAX_SOURCE, deadline)[0]


def reviewed_launcher(r1_pin, raw, clock):
    fragments = {
        'runtime = sorted([*plan["runtime_files"], image], key=lambda row: row["path"])':
            'predecessor = predecessor_image(packet, component.SID, fresh, deadline)\n'
            '    runtime = sorted([*runtime_with_predecessor(plan, predecessor), image], key=lambda row: row["path"])',
        '"runtime differs beyond the single additive native image"':
            '"runtime differs beyond the exact R1 and R2 additive images"',
        'supplemental_producer, *addon_sources]':
            'supplemental_producer, previous_launcher, packet["predecessor"], capacity, *addon_sources]',
        '    explicit = [*source, preparation_pin, graph_execution, earlier_plan, graph, supplemental,':
            '    capacity = reviewed_capacity(packet, args.capacity_policy_sha256, fresh, deadline)\n'
            '    explicit = [*source, preparation_pin, graph_execution, earlier_plan, graph, supplemental,',
        '"boundary": pins}': '"capacity_policy": capacity, "boundary": pins}',
        '    args = parser.parse_args()':
            '    parser.add_argument("--capacity-policy-sha256")\n    args = parser.parse_args()',
        '("AIRK_CUDA_HYBRID_SUPPLEMENTAL", plan["supplemental_fixture"])):':
            '("AIRK_CUDA_HYBRID_SUPPLEMENTAL", plan["supplemental_fixture"]),\n'
            '                        ("AIRK_CUDA_HYBRID_CAPACITY", plan["capacity_policy"])):',
        '"airk.private.owned-compact-bounded-hybrid-profile/1"':
            '"airk.private.owned-compact-bounded-hybrid-profile/2"',
        'started, started_utc = time.monotonic(), dt.datetime.now(dt.timezone.utc)':
            'started, started_utc = outer_clock',
    }
    source = raw.decode("utf-8")
    for before, after in fragments.items():
        require(source.count(before) == 1, "reviewed R1 adaptation anchor differs")
        source = source.replace(before, after)
    owner = load(r1_pin, source.encode("utf-8"), "compact_bounded_hybrid_r2")
    owner.SELF, owner.CONTRACT = SELF, CONTRACT
    owner.previous_launcher = r1_pin
    owner.outer_clock = clock
    owner.SCHEMA = "airk.private.compact-bounded-hybrid-plan/2"
    owner.runtime_with_predecessor = runtime_pins

    def predecessor_image(packet, sid, fresh, deadline):
        prior = packet["predecessor"]
        require(prior["sha256"] == R1_PACKET_SHA, "exact predecessor packet required")
        return inference_image(fresh.read_checked(prior, fresh.MAX_CHAIN_JSON, deadline), sid)

    def reviewed_capacity(packet, sha, fresh, deadline):
        pin = capacity_pin(sha, fresh, deadline)
        require(packet["capacity_policy"] == pin, "image refresh and capacity approval differ")
        return pin

    validate_r1 = owner.validate_result

    def validate_result(value, plan, lease, owned, profile_pin, component):
        validate_r1(value, plan, lease, owned, profile_pin, component)
        require(lease.base.core_pin_matches(value["capacity_policy"], plan["capacity_policy"])
                and value["applied_capacity"] == APPLIED_CAPACITY,
                "native applied R2 capacity proof differs")

    owner.predecessor_image = predecessor_image
    owner.reviewed_capacity = reviewed_capacity
    owner.validate_result = validate_result
    return owner


def main():
    started = time.monotonic()
    started_utc = dt.datetime.now(dt.timezone.utc)
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    pin, raw = code(R1, R1_SHA)
    if sys.argv[1:2] == ["--refresh-image"]:
        refresh(load(pin, raw, "compact_bounded_hybrid_r1"), sys.argv[2:], started, started_utc)
    else:
        reviewed_launcher(pin, raw, (started, started_utc)).main()


if __name__ == "__main__":
    main()
