# SPDX-License-Identifier: Apache-2.0
"""Run the reviewed Windows CUDA three-architecture compile candidate."""

import argparse
from collections import Counter
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import types

SELF = Path(__file__).resolve()
HERE = SELF.parent
SOURCE_SCHEMA = "airk.private.cuda-fresh-configure-recipe/1"
CANDIDATE_SCHEMA = "airk.private.cuda-fresh-architecture-recipe/1"
ARCHITECTURES = "86;89;120"
ARCH_ARG = "-DCMAKE_CUDA_ARCHITECTURES="
# ORT's pinned CMake source normalizes the selected Blackwell architecture
# to accelerated native code. The MSVC LLM target independently uses PTX.
CUBINS = {"86", "89", "120a"}
INSPECTOR = Path("C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin/cuobjdump.exe")
FROZEN = {
    "configure": ("fresh_configure.py", "0a45de8dc0aa23554e8fe17042b8b17853dea4356d138d52aa39470b8759fe51"),
    "compile": ("fresh_compile.py", "9baab4661425582367f636c46d2a02871d43f8567d1b6c0e68445197e99813da"),
    "finalize": ("fresh_finalize.py", "e0c1b39bd0306cbde3aa551db108180b5b64818c48d0af89bfff17b97873ea36"),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def exact_bytes(path, sha):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            "canonical reviewed architecture source or document required")
    with path.open("rb") as stream:
        raw = stream.read((4 << 20) + 1)
    require(0 < len(raw) <= 4 << 20 and hashlib.sha256(raw).hexdigest() == sha,
            "reviewed architecture source or document differs: " + str(path))
    return raw


def reviewed_recipe(raw, owner_pin):
    recipe = json.loads(raw)
    require(recipe["schema"] == CANDIDATE_SCHEMA
            and recipe["cuda_architectures"] == ARCHITECTURES
            and recipe["cuda_architecture_owner"] == owner_pin
            and Path(recipe["cuda_code_inspector"]["path"]) == INSPECTOR,
            "exact reviewed CUDA architecture candidate and pinned inspector required")
    return recipe


def command_with_architectures(original):
    indices = [i for i, item in enumerate(original) if item.startswith(ARCH_ARG)]
    require(len(indices) == 1 and original[indices[0]] == ARCH_ARG + "86",
            "single original CMake architecture argument required")
    result = list(original)
    result[indices[0]] = ARCH_ARG + ARCHITECTURES
    return result


def ninja_architectures(ninja):
    # Read only CUDA compile edges that contribute to the actual provider.
    target = re.compile(r": CUDA_COMPILER__(onnxruntime_providers_cuda[A-Za-z0-9_]*)_Release\s")
    real_pattern = re.compile(r"(?<![A-Za-z0-9_])sm_([0-9]+[af]?)(?![A-Za-z0-9_])")
    virtual_pattern = re.compile(r"code=\[?compute_([0-9]+[af]?)(?![A-Za-z0-9_])")
    generic = llm = 0
    specializations = {}
    for edge in ninja.split("\nbuild "):
        header, _, fields = edge.partition("\n")
        match = target.search(header)
        if match is None:
            continue
        name = match[1].removesuffix("_unscanned")
        flags = re.search(r"^  FLAGS = (.+)$", fields, re.MULTILINE)
        require(flags is not None, "actual generated CUDA provider edge has no flags")
        code = " ".join(flag for flag in flags[1].split() if "--generate-code=" in flag)
        real, virtual = set(real_pattern.findall(code)), set(virtual_pattern.findall(code))
        require(real <= CUBINS | {"90a"} and virtual <= {"120"}
                and bool(real or virtual), "unexpected CUDA architecture in generated provider edge")
        if name in {"onnxruntime_providers_cuda", "onnxruntime_providers_cuda_obj"}:
            require(real == CUBINS and not virtual, "general CUDA provider edge lacks exact 86/89/120a SASS")
            generic += 1
        elif name == "onnxruntime_providers_cuda_llm":
            require(real == {"86", "89"} and virtual == {"120"},
                    "Windows LLM CUDA edge lacks the expected Blackwell PTX specialization")
            llm += 1
        else:
            recorded = {"real": sorted(real), "virtual": sorted(virtual)}
            require(name not in specializations or specializations[name] == recorded,
                    "mixed architecture flags within a specialized CUDA target")
            specializations[name] = recorded
    require(generic > 0 and llm > 0, "general and Windows LLM CUDA compile edges required")
    return {"requested": ARCHITECTURES, "generic_edges": generic, "windows_llm_edges": llm,
            "generic_sass": sorted(CUBINS), "windows_llm_ptx": ["120"],
            "specialized_targets": specializations}


def code_inventory(elf, ptx):
    real = Counter(re.findall(r"(?m)^\s*ELF file\s+\d+: .*\.sm_([0-9]+[af]?)\.cubin\s*$", elf))
    virtual = Counter(re.findall(r"(?m)^\s*PTX file\s+\d+: .*\.(?:sm|compute)_([0-9]+[af]?)\.ptx\s*$", ptx))
    require(len(re.findall(r"(?m)^\s*ELF file\s+\d+:", elf)) == sum(real.values())
            and len(re.findall(r"(?m)^\s*PTX file\s+\d+:", ptx)) == sum(virtual.values()),
            "unrecognized code object in actual CUDA provider inventory")
    require(CUBINS <= real.keys() <= CUBINS | {"90a"} and virtual.keys() == {"120"},
            "actual CUDA provider is missing required real or virtual architecture code")
    return {"cubin_objects": dict(sorted(real.items())), "ptx_objects": dict(sorted(virtual.items()))}


def frozen(name):
    leaf, sha = FROZEN[name]
    path = HERE / leaf
    body = exact_bytes(path, sha).decode("utf-8")
    token = '"' + SOURCE_SCHEMA + '"'
    require(body.count(token) == 1, "frozen build recipe schema anchor differs")
    body = body.replace(token, '"' + CANDIDATE_SCHEMA + '"')
    if name == "finalize":
        historical_argv = (
            '"-B", "-S", CONFIGURE_DRIVER, "--recipe", original_pin["path"], '
            '"--recipe-sha256", original_pin["sha256"],'
        )
        candidate_argv = (
            '"-B", "-S", CONFIGURE_DRIVER, "configure", "--input", original_pin["path"], '
            '"--input-sha256", original_pin["sha256"], "--approved-script-sha256", '
            'original["cuda_architecture_owner"]["sha256"],'
        )
        require(body.count(historical_argv) == 1, "frozen configure recovery argv anchor differs")
        body = body.replace(historical_argv, candidate_argv)
    owner = types.ModuleType("airk_cuda_architecture_" + name)
    owner.__file__ = str(path)
    exec(compile(body, str(path), "exec", dont_inherit=True), owner.__dict__)
    return owner


def modules():
    cfg = frozen("configure")
    compiler = frozen("compile")
    recovery = frozen("finalize")
    original_command, original_graph = cfg.configure_command, cfg.configured_inputs
    cfg.configure_command = lambda cpu, r8, root, tools, frontend: command_with_architectures(
        original_command(cpu, r8, root, tools, frontend))

    def configured_inputs(cpu, r8, root, command, lock, collector, **options):
        facts = original_graph(cpu, r8, root, command, lock, collector, **options)
        graph = (root / "b/Release/build.ninja").read_text(encoding="utf-8")
        return dict(facts, cuda_architecture_evidence=ninja_architectures(graph))

    def use_architectures(loader):
        replacements = {HERE / FROZEN["configure"][0]: cfg, HERE / FROZEN["compile"][0]: compiler}

        def load(path, name):
            if Path(path) in replacements:
                return replacements[Path(path)]
            return loader(path, name)

        return load

    cfg.configured_inputs = configured_inputs
    compiler.load = use_architectures(compiler.load)
    recovery.load = use_architectures(recovery.load)
    recovery.CONFIGURE_DRIVER = "scripts/cuda-static-runtime/build/fresh_architectures.py"
    # The former campaign calendar cutoff is historical. Keep the original
    # wall allowance and process-derived clocks in this separate invocation.
    cfg.CAMPAIGN_END = compiler.CAMPAIGN_END = dt.datetime.max.replace(tzinfo=dt.timezone.utc)
    return cfg, compiler, recovery


def observe_code(cfg, compiler, compiled_pin, recipe, owner_pin):
    pins = compiler.load(compiler.OWNERS["pin_reader"], "fresh_cuda_architecture_pins")
    body = compiler.read_pinned(pins, compiled_pin, None)
    deadline = body["deadline_monotonic"]
    pins.deadline_check(deadline)
    root = Path(recipe["fresh_output"])
    graph = body["configured_graph"]["cuda_architecture_evidence"]
    require(graph["requested"] == ARCHITECTURES, "compiled receipt lacks requested CUDA architecture evidence")
    provider = [row for row in body["providers"] if Path(row["path"]).name == "onnxruntime_providers_cuda.dll"]
    require(len(provider) == 1, "one actual compiled CUDA provider required")
    binary = pins.check_pin(provider[0], deadline)
    tool = pins.check_pin(recipe["cuda_code_inspector"], deadline)
    processes = cfg.load(compiler.OWNERS["process_stage"], "fresh_cuda_architecture_process")
    component = cfg.load(compiler.OWNERS["guards"], "fresh_cuda_architecture_guard")
    cpu = cfg.load(compiler.OWNERS["cpu"], "fresh_cuda_architecture_cpu")
    component.guard(recipe, deadline=deadline)
    guard = compiler.output_guard(component, recipe)
    receipt_path = root / "fresh-cuda-code.json"
    guard([receipt_path], False, deadline)
    require(not receipt_path.exists(), "existing CUDA code receipt must be preserved")
    environment = dict(recipe["environment"])
    environment.update(TEMP=str(root / "tmp"), TMP=str(root / "tmp"), TMPDIR=str(root / "tmp"),
                       CUDA_CACHE_PATH=str(root / "cuda-cache"), PYTHONNOUSERSITE="1")
    stages, reports = [], []
    for stage, flag in (("cuda-cubin-census", "--list-elf"), ("cuda-ptx-census", "--list-ptx")):
        argv = [str(tool), "--all-fatbin", flag, str(binary)]
        proof = processes.run_stage(argv=argv, cwd=root, root=root, stage=stage, environment=environment,
                                    cpu=cpu, guard_paths=guard, power_shell=recipe["powershell"],
                                    tree_waiter=recipe["owners"]["tree_waiter"], deadline=deadline)
        stage_pin = compiler.verify_stage(pins, cpu, root, proof, stage, argv, deadline, deadline)
        stdout = [item for item in proof["files"] if Path(item["path"]).name == stage + ".stdout.log"]
        require(len(stdout) == 1 and 0 < stdout[0]["bytes"] <= 16 << 20, "bounded CUDA code census required")
        output = exact_bytes(Path(stdout[0]["path"]), stdout[0]["sha256"]).decode("utf-8")
        stages.append(stage_pin)
        reports.append(output)
    census = code_inventory(*reports)
    for pin in (compiled_pin, provider[0], recipe["cuda_code_inspector"], owner_pin):
        pins.check_pin(pin, deadline)
    component.write_new(recipe, receipt_path, {
        "schema": "airk.private.cuda-fresh-architecture-code/1", "qualification": False,
        "compile": compiled_pin, "architecture_owner": owner_pin, "inspector": recipe["cuda_code_inspector"],
        "provider": provider[0], "configured_architectures": graph, "stage_receipts": stages, **census,
    })
    pins.deadline_check(deadline)
    return pins.pin(receipt_path, deadline)


def main():
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("configure", "compile", "finalize"))
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--input-sha256", required=True)
    parser.add_argument("--approved-script-sha256", required=True)
    args = parser.parse_args()
    own = exact_bytes(SELF, args.approved_script_sha256)
    owner_pin = {"path": str(SELF), "bytes": len(own), "sha256": args.approved_script_sha256}
    cfg, compiler, recovery = modules()
    raw = exact_bytes(args.input, args.input_sha256)
    if args.phase == "configure":
        reviewed_recipe(raw, owner_pin)
        result = cfg.execute(args.input, args.input_sha256)
    else:
        input_value = json.loads(raw)
        recipe_pin = input_value["recipe" if args.phase == "compile" else "configure_recipe"]
        recipe = reviewed_recipe(exact_bytes(Path(recipe_pin["path"]), recipe_pin["sha256"]), owner_pin)
        if args.phase == "compile":
            result = compiler.execute(args.input, args.input_sha256, FROZEN["compile"][1])
            result = {"compile": result, "code": observe_code(cfg, compiler, result, recipe, owner_pin)}
        else:
            result = recovery.execute(args.input, args.input_sha256)
    print(json.dumps(result, allow_nan=False))


if __name__ == "__main__":
    main()
