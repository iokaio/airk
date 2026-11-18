# SPDX-License-Identifier: Apache-2.0
"""Finish a fresh CUDA configure whose settled command passed before an output audit timeout."""

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
SCHEMA = "airk.private.cuda-fresh-configure-audit-recovery-recipe/1"
CONFIGURE_DRIVER = "scripts/cuda-static-runtime/build/fresh_configure.py"
MAX_METADATA = 32 << 20


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def read_bytes(pins, pin, deadline):
    path = pins.check_pin(pin, deadline)
    require(pin["bytes"] <= MAX_METADATA, "fresh configure audit evidence exceeds its bound")
    with path.open("rb") as stream:
        raw = stream.read(pin["bytes"] + 1)
    require(len(raw) == pin["bytes"] and hashlib.sha256(raw).hexdigest() == pin["sha256"],
            "fresh configure audit evidence changed during read")
    pins.deadline_check(deadline)
    return raw


def check_failed_audit(outer, log, original_pin, original, sid, utc, now):
    argv = [
        "-B", "-S", CONFIGURE_DRIVER, "--recipe", original_pin["path"], "--recipe-sha256", original_pin["sha256"],
    ]
    require(type(outer["exit_code"]) is int and outer["exit_code"] == 1 and outer["raw_receipt"] is None
            and outer["input_hash"] == original_pin["sha256"] and outer["arguments"] == argv
            and Path(outer["command"]) == Path(original["tools"]["python"]["path"])
            and outer["qualification"] is False and outer["human_sid"] == sid,
            "same-launch failed configure wrapper evidence required")
    require(b'guard_paths(sorted((root / "b").rglob("*")), True, deadline)' in log
            and b"subprocess.TimeoutExpired:" in log and b"timed out after 10.0 seconds" in log,
            "configure failure was not the post-command output audit timeout")
    started = utc(outer["started_utc"])
    require(type(outer["elapsed_seconds"]) in (int, float)
            and 0 < outer["elapsed_seconds"] < original["wall_seconds"],
            "failed configure wrapper elapsed time differs")
    finished = started + dt.timedelta(seconds=outer["elapsed_seconds"])
    require(started <= finished <= now, "failed configure wrapper chronology differs")
    return started, finished


def recovery_deadline(clock, utc_now, started, wall_seconds, campaign_end, original_deadline):
    require(type(wall_seconds) is int and 0 < wall_seconds <= 10800, "original configure wall bound differs")
    cutoff = min(started + dt.timedelta(seconds=wall_seconds), campaign_end)
    deadline = min(original_deadline, clock + (cutoff - utc_now).total_seconds())
    require(clock < deadline, "original configure recovery deadline is exhausted")
    return deadline


def replay_stages(pins, cpu, compiler, sources, configure_driver, r8, recipe, lock, root, config, raw, guard, deadline):
    original_deadline = raw["original_deadline_monotonic"]
    patch_tool = recipe["tools"]["patch"]["path"]
    source, frontend = root / "source", root / "frontend"
    compatibility = {"msvc_ciso646_compat": True} if recipe.get("msvc_ciso646_compat") is True else {}
    stages = [
        (name, cwd, [patch_tool, "--batch", "--forward", "--fuzz=0", "--binary", "--ignore-whitespace",
                     "--no-backup-if-mismatch", "--reject-file=-", "-p1",
                     "--input=" + str(root / "patch-inputs" / (name + ".patch"))])
        for name, cwd, _, _, _ in sources.patch_roster(source, frontend, lock, **compatibility)
    ]
    command = configure_driver.configure_command(cpu, r8, root, recipe["tools"], frontend)
    stages.append(("configure", root, command))
    paths = [
        root / "logs" / (name + suffix)
        for name, _, _ in stages for suffix in (*compiler.STAGE_SUFFIXES.values(), ".receipt.json")
    ]
    guard(paths, True, deadline)
    previous = None
    receipts = []
    for name, cwd, argv in stages:
        stage_pin = config if name == "configure" else pins.pin(root / "logs" / (name + ".receipt.json"), deadline)
        proof = pins.parse_json(read_bytes(pins, stage_pin, deadline))
        retained = compiler.verify_stage(pins, cpu, root, proof, name, argv, original_deadline, deadline, cwd=cwd)
        require(retained == stage_pin and (name != "configure" or proof == raw),
                "retained fresh configure stage evidence differs")
        result = proof["actual_tree_result"]
        began = compiler.utc(result["start_utc"])
        if previous is not None:
            require(previous <= began, "retained fresh configure stages are out of order")
        previous = compiler.utc(result["finished_utc"])
        receipts.append(retained)
    return command, receipts


def retain(path, data, guard, deadline, pins):
    guard([path], False, deadline)
    pins.deadline_check(deadline)
    with path.open("xb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    guard([path], True, deadline)
    pins.deadline_check(deadline)
    return pins.pin(path, deadline)


def encode(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def execute(recipe_path, expected_sha):
    now = dt.datetime.now(dt.timezone.utc)
    started_monotonic = time.monotonic()
    bootstrap = started_monotonic + 10800
    pin_reader = REPO / "scripts/cuda-static-incremental.py"
    pins = load(pin_reader, "fresh_finalize_pins")
    recipe_pin = pins.pin(recipe_path, bootstrap)
    require(recipe_pin["sha256"] == expected_sha, "fresh output-audit recovery recipe pin differs")
    recovery = pins.parse_json(read_bytes(pins, recipe_pin, bootstrap))
    require(os.name == "nt" and set(recovery) == {"schema", "repository_root", "configure_recipe",
                "failed_outer_receipt", "failed_outer_log", "configure_stage", "owners",
                "execution_authorized", "qualification"}
            and recovery["schema"] == SCHEMA and recovery["repository_root"] == str(REPO)
            and recovery["execution_authorized"] is True and recovery["qualification"] is False,
            "reviewed fresh configure audit recovery recipe required")
    owners = {
        "driver": Path(__file__), "compile": HERE / "fresh_compile.py",
        "source_replay": HERE / "fresh_retained_source.py", "pin_reader": pin_reader,
    }
    require(set(recovery["owners"]) == set(owners), "fresh output-audit recovery owners differ")
    for name, path in owners.items():
        require(pins.check_pin(recovery["owners"][name], bootstrap) == path,
                "pinned fresh recovery owner path differs")
    compiler = load(owners["compile"], "fresh_finalize_compile")
    original_pin, outer_pin, log_pin, config_pin = (
        recovery[name] for name in ("configure_recipe", "failed_outer_receipt", "failed_outer_log", "configure_stage")
    )
    original = pins.parse_json(read_bytes(pins, original_pin, bootstrap))
    require(original["schema"] == "airk.private.cuda-fresh-configure-recipe/1"
            and original["repository_root"] == str(REPO) and original["execution_authorized"] is True
            and original["qualification"] is False and set(original["owners"]) == set(compiler.OWNERS)
            and all(Path(original["owners"][name]["path"]) == path for name, path in compiler.OWNERS.items()),
            "original fresh configuration owner closure differs")
    root = Path(original["fresh_output"])
    require(root.parent == compiler.ANCHOR and Path(config_pin["path"]) == root / "logs/configure.receipt.json",
            "actual configure stage is outside its original root")
    outer = pins.parse_json(read_bytes(pins, outer_pin, bootstrap))
    log = read_bytes(pins, log_pin, bootstrap)
    raw = pins.parse_json(read_bytes(pins, config_pin, bootstrap))
    require(outer["log_sha256"] == log_pin["sha256"] and compiler.valid_seconds(raw["original_deadline_monotonic"]),
            "original failed log or configure cutoff differs")
    pins.check_pin(original["owners"]["guards"], bootstrap)
    component = load(compiler.OWNERS["guards"], "fresh_finalize_guard")
    original_start, failed_end = check_failed_audit(outer, log, original_pin, original, component.SID, compiler.utc, now)
    tree = raw["actual_tree_result"]
    require(original_start <= compiler.utc(tree["start_utc"]) <= compiler.utc(tree["finished_utc"]) <= failed_end,
            "actual configure command does not belong to the failed outer run")
    deadline = recovery_deadline(started_monotonic, now, original_start, original["wall_seconds"],
                                 compiler.CAMPAIGN_END, raw["original_deadline_monotonic"])
    compiler.verify_frozen_inputs(pins, original, deadline)
    require(Path(sys.executable).resolve() == Path(original["tools"]["python"]["path"]),
            "fresh configure audit replay uses a different Python executable")
    component.guard(original, deadline=deadline)
    guard = compiler.output_guard(component, original)
    guard([root, *(Path(row["path"]) for row in (recipe_pin, original_pin, outer_pin, log_pin, config_pin))],
          True, deadline)
    output = root / "fresh-configure.json"
    evidence = root / "fresh-configure-audit-recovery.json"
    require(not output.exists() and not evidence.exists()
            and not any((root / "logs" / (name + ".request.json")).exists() for name, _ in compiler.STAGE_TARGETS),
            "existing configure audit recovery or subsequent stage preserved")
    guard([output, evidence], False, deadline)
    cpu, r8, configure_driver, sources = (
        load(compiler.OWNERS[name], "fresh_finalize_" + name) for name in ("cpu", "r8", "driver", "source_stage")
    )
    lock = pins.parse_json(read_bytes(pins, original["owners"]["lock"], deadline))
    argv, stage_receipts = replay_stages(pins, cpu, compiler, sources, configure_driver, r8, original,
                                        lock, root, config_pin, raw, guard, deadline)
    files = compiler.build_files(root, guard, deadline, include_caches=True)
    compiler.require_clean_build(root, files)
    source_replay = load(owners["source_replay"], "fresh_finalize_retained_source")
    compatibility = {"msvc_ciso646_compat": True} if original.get("msvc_ciso646_compat") is True else {}
    source = source_replay.reconstruct(
        destination=root, source_archive=Path(original["source_archive"]["path"]),
        dependency_archives={name: Path(row["path"]) for name, row in original["dependency_archives"].items()},
        lock=lock, cpu=cpu, r8=r8, sources=sources, pins=pins, guard_paths=guard, deadline=deadline,
        **compatibility,
    )
    facts = configure_driver.configured_inputs(cpu, r8, root, argv, lock, source["collector_source"], **compatibility)
    require(cpu.source_inventory(source["source"], exclude_mirror=True) == source["source_inventory"]
            and cpu.source_inventory(source["frontend"]) == source["frontend_inventory"]
            and cpu.source_inventory(source["mirror"]) == source["mirror_inventory"],
            "retained CUDA source changed during recovery")
    compiler.verify_frozen_inputs(pins, original, deadline)
    for row in (recipe_pin, original_pin, outer_pin, log_pin, *stage_receipts, *recovery["owners"].values()):
        pins.check_pin(row, deadline)
    finished = dt.datetime.now(dt.timezone.utc)
    require(failed_end <= finished, "configure audit recovery clock regressed")
    receipt = {
        "schema": "airk.private.cuda-fresh-configured/1", "status": "CONFIGURED_UNQUALIFIED",
        "recipe": original_pin, "toolchain": original["tools"], "configure": raw,
        "configured_graph": facts, "collector_source": source["collector_source"],
        "source_inventory": source["source_inventory"], "frontend_inventory": source["frontend_inventory"],
        "mirror_inventory": source["mirror_inventory"], "patch_inputs": source["patch_inputs"],
        "started_utc": original_start.isoformat(), "finished_utc": finished.isoformat(),
        "elapsed_seconds": (finished - original_start).total_seconds(), "qualification": False,
    }
    compiler.configure_window(receipt, original)
    body = encode(receipt)
    expected = {"path": str(output), "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}
    recovery_evidence = {
        "schema": "airk.private.cuda-fresh-configure-audit-recovery/1",
        "recipe": recipe_pin, "failed_outer_receipt": outer_pin, "failed_outer_log": log_pin,
        "stage_receipts": stage_receipts, "configured_receipt": expected, "qualification": False,
        "started_utc": now.isoformat(), "finished_utc": finished.isoformat(),
        "original_deadline_monotonic": raw["original_deadline_monotonic"], "effective_deadline_monotonic": deadline,
    }
    retained_evidence = retain(evidence, encode(recovery_evidence), guard, deadline, pins)
    actual = retain(output, body, guard, deadline, pins)
    require(actual == expected, "retained configure audit recovery output bytes differ")
    return {"configure_receipt": actual, "recovery_receipt": retained_evidence}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--recipe-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(execute(args.recipe, args.recipe_sha256)))


if __name__ == "__main__":
    main()
