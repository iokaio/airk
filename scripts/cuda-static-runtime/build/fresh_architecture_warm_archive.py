# SPDX-License-Identifier: Apache-2.0
"""Archive one cleared cu17 warm compilation without relabeling its evidence."""

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import time
import types

SELF = Path(__file__).resolve()
HERE = SELF.parent
REPO = HERE.parents[2]
WARM_SHA = "e0c30c6b0b3371c9aa03a9e6f32a0cc751d7a31de0b9a69313b59ed07843bd0c"
STOCK_SHA = "76266fa883786da4b1833f9316f5121eccaf7ac60440cc105544df8e351260a7"
COFF_SHA = "5db96e7b0fe1853f7753e7aa3c15e576b87d2c29b465b8a2b446b4f611bb2a89"
RECIPE = "airk.private.cuda-architecture-warm-aggregate-recipe/1"
RESULT = "airk.private.cuda-architecture-warm-aggregate/1"
PLAN = "airk.private.cuda-architecture-warm-aggregate-plan/1"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def exact_module(path, sha):
    require(path.resolve(strict=True) == path and path.is_file(), "canonical maintained source required")
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == sha,
            "reviewed source bytes differ: " + str(path))
    owner = types.ModuleType("cu17_warm_archive_" + path.stem)
    owner.__file__ = str(path)
    exec(compile(raw, str(path), "exec", dont_inherit=True), owner.__dict__)
    return owner


def check_stage(warm, cpu, row, *, stage, argv, root, output, deadline):
    expected_log = output / (stage + ".log")
    owned = row["actual_owned"]
    require(row["stage"] == stage and row["argv"] == argv and row["cwd"] == str(root)
            and row["deadline_monotonic"] == deadline and Path(row["log"]["path"]) == expected_log
            and owned["argv"] == argv and owned["log"] == str(expected_log)
            and warm.empty_owned_job(owned) and cpu.successful(owned),
            "retained direct warm build stage did not naturally settle with exact argv")
    return row["log"]


def replay_warm(value, warm_pin, root, chain, warm, arch, compiler, pins, guard, original, deadline):
    configured, config, _, owners = chain
    cpu = owners["cpu"]
    emitted = Path(warm_pin["path"]).parent
    build_root = root / "b/Release"
    require(value["schema"] == warm.SCHEMA and value["status"] == "WARM_COMPILED_UNQUALIFIED"
            and value["qualification"] is False and value["error"] is None
            and value["configure_recipe"] == configured["recipe"]
            and value["architecture_owner"] == config["cuda_architecture_owner"]
            and value["recovery_owner"] == pins.pin(HERE / "fresh_architecture_warm.py", deadline)
            and value["recovery_owner"]["sha256"] == WARM_SHA
            and value["original_deadline_monotonic"] == original
            and compiler.valid_seconds(value["effective_deadline_monotonic"])
            and 0 < value["effective_deadline_monotonic"] <= original
            and len(value["stages"]) == 4, "completed exact original-window cu17 warm result required")

    failed_pin = value["failed_session_stage"]
    fail_path = root / "logs/core-session-build.receipt.json"
    require(Path(failed_pin["path"]) == fail_path and failed_pin["sha256"] == warm.FAILED_STAGE_SHA,
            "warm continuation used different original failure evidence")
    guard([fail_path], True, deadline)
    prior = compiler.read_pinned(pins, failed_pin, deadline)
    failed = warm.failed_session(prior, root, config, compiler, original, guard, pins, deadline)

    ninja = pins.check_pin(config["tools"]["ninja"], deadline)
    jobs = compiler.parallel_workers(config)
    inspector = pins.check_pin(config["cuda_code_inspector"], deadline)
    require(inspector == arch.INSPECTOR and value["code_inspector"] == config["cuda_code_inspector"],
            "code inventory inspector differs from reviewed configure recipe")
    providers = [entry for entry in value["providers"]
                 if Path(entry["path"]) == build_root / "onnxruntime_providers_cuda.dll"]
    require(len(providers) == 1 and value["code_provider"] == providers[0], "warm code provider differs")
    provider = pins.check_pin(providers[0], deadline)
    expected = (
        ("warm-session", warm.ninja_command(ninja, build_root, jobs, ("onnxruntime_session",))),
        ("warm-provider", warm.ninja_command(ninja, build_root, jobs, ("all", "re2"))),
        ("cuda-cubin-census", [str(inspector), "--all-fatbin", "--list-elf", str(provider)]),
        ("cuda-ptx-census", [str(inspector), "--all-fatbin", "--list-ptx", str(provider)]),
    )
    logs = []
    last = compiler.utc(configured["finished_utc"])
    for row, (stage, argv) in zip(value["stages"], expected):
        log = check_stage(warm, cpu, row, stage=stage, argv=argv, root=root, output=emitted,
                          deadline=value["effective_deadline_monotonic"])
        start = compiler.utc(row["started_utc"])
        finish = compiler.utc(row["finished_utc"])
        require(last <= start <= finish, "direct warm stage chronology differs")
        last = finish
        logs.append(log)
    cutoff, _ = compiler.configure_window(configured, config)
    require(last <= compiler.utc(value["finished_utc"]) < cutoff, "warm receipt exceeded original build cutoff")
    guard([Path(log["path"]) for log in logs], True, deadline)
    for log in logs:
        pins.check_pin(log, deadline)
    reports = []
    for log in logs[-2:]:
        raw = arch.exact_bytes(Path(log["path"]), log["sha256"])
        require(len(raw) == log["bytes"], "observed direct code census length differs")
        reports.append(raw.decode("utf-8"))
    require(arch.code_inventory(*reports) == value["code_inventory"], "actual warm SASS/PTX census differs")

    ninja_pins = compiler.verify_inputs(pins, chain, deadline)
    require(ninja_pins == value["ninja_graph"], "original configured Ninja graph drifted")
    audited = compiler.build_files(root, guard, deadline)
    libraries, provider_roster = compiler.pin_outputs(pins, root, audited, deadline)
    require(libraries == value["libraries"] and provider_roster == value["providers"],
            "warm build output roster drifted")
    stock_view = {"ninja": ninja_pins, "configured_graph": configured["configured_graph"], "libraries": libraries}
    return stock_view, [warm_pin, value["configure"], value["configure_recipe"], value["architecture_owner"],
                       value["recovery_owner"], failed_pin, config["cuda_code_inspector"],
                       *ninja_pins, *failed, *logs, *provider_roster]


def execute(recipe_path, sha):
    warm = exact_module(HERE / "fresh_architecture_warm.py", WARM_SHA)
    arch = warm.load_architecture()
    _, compiler, _ = arch.modules()
    pins = compiler.load(compiler.OWNERS["pin_reader"], "warm_archive_pins")
    stock = exact_module(HERE / "fresh_aggregate.py", STOCK_SHA)
    directory = compiler.ANCHOR / "cu17"
    configuration = directory / "fresh-configure.json"
    configured_pin = pins.pin(configuration)
    require(configured_pin["sha256"] == warm.CONFIGURED_SHA, "original cu17 configure receipt differs")
    configured = compiler.read_pinned(pins, configured_pin, None)
    config = compiler.read_pinned(pins, configured["recipe"], None)
    original_cutoff = compiler.build_deadline(configured, config)
    _, original = compiler.configure_window(configured, config)
    chain = compiler.load_chain(pins, configured_pin, original_cutoff)
    configured, config, root, owners = chain
    require(root == directory and Path(sys.executable).resolve() == Path(config["tools"]["python"]["path"]),
            "cu17 warm archive path or original interpreter differs")
    component, cpu = owners["guards"], owners["cpu"]
    component.guard(config, deadline=original_cutoff)
    guard = compiler.output_guard(component, config)
    recipe_pin = pins.pin(recipe_path, original_cutoff)
    require(recipe_pin["sha256"] == sha, "reviewed warm archive recipe differs")
    recipe = compiler.read_pinned(pins, recipe_pin, original_cutoff)
    require(set(recipe) == {"schema", "driver", "warm_compile", "archiver", "output", "wall_seconds",
                            "execution_authorized", "qualification"} and recipe["schema"] == RECIPE
            and recipe["execution_authorized"] is True and recipe["qualification"] is False
            and type(recipe["wall_seconds"]) is int and 0 < recipe["wall_seconds"] <= 300
            and recipe["driver"] == pins.pin(SELF, original_cutoff),
            "exact reviewed guarded warm aggregate recipe required")
    warm_pin = recipe["warm_compile"]
    warm_path = Path(warm_pin["path"])
    output = Path(recipe["output"])
    require(warm_path.name == "warm-compile.json" and warm_path.parent.parent == root
            and re.fullmatch(r"warm-continuation-[a-z0-9-]{1,32}", warm_path.parent.name)
            and output.parent == root and re.fullmatch(r"warm-aggregate-[a-z0-9-]{1,32}", output.name)
            and not output.exists(), "exact retained warm or fresh archive path required")
    guard([warm_path], True, original_cutoff)
    value = compiler.read_pinned(pins, warm_pin, original_cutoff)
    require(value["configure"] == configured_pin, "warm configure receipt identity differs")
    verified, inputs = replay_warm(value, warm_pin, root, chain, warm, arch, compiler, pins, guard, original, original_cutoff)
    archiver = pins.check_pin(recipe["archiver"], original_cutoff)
    require(archiver == Path(config["tools"]["compiler"]["path"]).with_name("lib.exe")
            and [row for row in config["compiler_tools"] if Path(row["path"]) == archiver] == [recipe["archiver"]],
            "exact original configured archiver required")
    stock_pin = pins.pin(HERE / "fresh_aggregate.py", original_cutoff)
    coff_pin = pins.pin(REPO / "scripts/cuda-static-archive.py", original_cutoff)
    require(stock_pin["sha256"] == STOCK_SHA and coff_pin["sha256"] == COFF_SHA,
            "original stock archive verification owners differ")

    started = time.monotonic()
    deadline = min(original_cutoff, started + recipe["wall_seconds"])
    pins.deadline_check(deadline)
    coff = pins.bound_module(exact_module(Path(coff_pin["path"]), COFF_SHA), deadline)
    handoff = {"warm_compile": warm_pin, "configure": configured_pin, "code_provider": value["code_provider"],
               "code_inventory": value["code_inventory"]}
    plan = stock.select_plan(root / "b/Release", verified, warm_pin, cpu, coff, pins, deadline,
                             producer_proof_sha256=pins.canonical(handoff))
    # These stock pure functions only consume archive closure and FilePins. The
    # retained candidate plan has its own identity and no fake cold compile.
    plan["schema"] = PLAN
    plan["warm_compile_receipt"] = plan.pop("compile_receipt")
    chosen = plan["inputs"] + [row["pin"] for row in plan["excluded_provider_imports"]]
    observed_inputs = stock.inspect_inputs(plan, cpu, coff, pins, deadline)
    component.mkdir(config, output, deadline=deadline)
    temporary = output / "tmp"
    component.mkdir(config, temporary, deadline=deadline)
    response, binary, receipt = (output / name for name in ("inputs.rsp", "onnxruntime.lib", "warm-aggregate.json"))
    guard([response, binary, receipt], False, deadline)
    proof = {"schema": RESULT, "status": "FAILED_PRESERVED_NO_RETRY", "qualification": False,
             "recipe": recipe_pin, "private_handoff": handoff, "plan": plan, "plan_sha256": pins.canonical(plan),
             "stock_archive_owner": stock_pin, "stock_coff_owner": coff_pin, "archiver": recipe["archiver"],
             "original_lineage_deadline_monotonic": original, "started_monotonic": started,
             "effective_deadline_monotonic": deadline, "stages": [], "error": None}
    try:
        content = stock.response_bytes(plan)
        with response.open("xb") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        guard([response], True, deadline)
        require(response.read_bytes() == content, "warm archive response bytes changed")
        proof["response"] = pins.pin(response, deadline)
        env = dict(config["environment"])
        env.update({name: str(temporary) for name in ("TEMP", "TMP", "TMPDIR")})
        command = [str(archiver), "/NOLOGO", "/OUT:" + str(binary), "@" + str(response)]
        warm.run_direct(cpu, pins, guard, root, output, "warm-lib", command, env, deadline, proof)
        guard(list(compiler.tree_paths(output, deadline)), True, deadline)
        proof["archive"] = pins.pin(binary, deadline)
        require(65536 < proof["archive"]["bytes"] <= coff.MAX_ARCHIVE, "warm archive output size differs")
        proof["coff"] = stock.inspect_output(binary, observed_inputs, coff)
        for pin in [*inputs, *chosen, recipe_pin, recipe["driver"], recipe["archiver"], stock_pin, coff_pin,
                    proof["response"], proof["archive"], proof["stages"][0]["log"]]:
            pins.check_pin(pin, deadline)
        pins.deadline_check(deadline)
        proof["status"] = "WARM_AGGREGATED_UNQUALIFIED"
    except BaseException as error:
        proof["error"] = {"type": type(error).__name__, "message": str(error)}
        raise
    finally:
        active = sys.exc_info()[1]
        proof["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        proof["elapsed_seconds"] = time.monotonic() - started
        try:
            retained = stock.retain_receipt(receipt, proof, guard, pins, deadline)
        except BaseException as error:
            if active is None:
                raise
            active.add_note("warm archive receipt retention: " + str(error))
    pins.deadline_check(deadline)
    return retained


def main():
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", type=Path, required=True)
    parser.add_argument("--recipe-sha256", required=True)
    selected = parser.parse_args()
    print(json.dumps(execute(selected.recipe, selected.recipe_sha256), allow_nan=False))


if __name__ == "__main__":
    main()
