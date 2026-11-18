# SPDX-License-Identifier: Apache-2.0
"""Aggregate one reviewed CUDA architecture build and retain its private proof."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import types

SELF = Path(__file__).resolve()
HERE = SELF.parent
AGGREGATE_SCHEMA = "airk.private.cuda-fresh-architecture-aggregate/1"
RECIPE_SCHEMA = "airk.private.cuda-fresh-architecture-aggregate-recipe/1"
BUILD_SCHEMA = "airk.private.cuda-fresh-architecture-build-proof/1"
CODE_SCHEMA = "airk.private.cuda-fresh-architecture-code/1"
ARCH_SHA = "865f88d629127f8931b3763e68096c0908c01f1eaaac3acfdc6abc2dbc4288bb"
AGGREGATE_SHA = "76266fa883786da4b1833f9316f5121eccaf7ac60440cc105544df8e351260a7"
PROJECTION_SHA = "d26eea4f195c91654dbec3f7583bc64236ebbaa9f70ea7e1f9c34370a094292b"
CODE_FIELDS = {
    "schema", "qualification", "compile", "architecture_owner", "inspector", "provider",
    "configured_architectures", "stage_receipts", "cubin_objects", "ptx_objects",
}
CENSUS_STAGES = (("cuda-cubin-census", "--list-elf"), ("cuda-ptx-census", "--list-ptx"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def frozen(leaf, sha, replacements=()):
    path = HERE / leaf
    require(path.resolve(strict=True) == path and path.is_file(), "canonical maintained owner required")
    with path.open("rb") as stream:
        raw = stream.read((2 << 20) + 1)
    require(0 < len(raw) <= 2 << 20 and hashlib.sha256(raw).hexdigest() == sha,
            "frozen maintained owner differs: " + leaf)
    body = raw.decode("utf-8")
    for old, new in replacements:
        require(body.count(old) == 1, "frozen aggregate adaptation anchor differs: " + leaf)
        body = body.replace(old, new)
    owner = types.ModuleType("airk_architecture_archive_" + path.stem)
    owner.__file__ = str(path)
    exec(compile(body, str(path), "exec", dont_inherit=True), owner.__dict__)
    return owner, {"path": str(path), "bytes": len(raw), "sha256": sha}


def check_codes(code, arch, arch_pin, recipe, compile_pin, compiled):
    graph = compiled["configured_graph"]["cuda_architecture_evidence"]
    providers = [row for row in compiled["providers"]
                 if Path(row["path"]).name == "onnxruntime_providers_cuda.dll"]
    require(set(code) == CODE_FIELDS and code["schema"] == CODE_SCHEMA and code["qualification"] is False
            and len(providers) == 1 and code["provider"] == providers[0]
            and code["compile"] == compile_pin and code["architecture_owner"] == arch_pin
            and code["inspector"] == recipe["cuda_code_inspector"]
            and code["configured_architectures"] == graph and graph["requested"] == arch.ARCHITECTURES
            and len(code["stage_receipts"]) == len(CENSUS_STAGES),
            "CUDA code census differs from the exact compiled provider or requested architecture graph")


def verify_codes(arch, arch_pin, compiler, pins, aggregate_recipe, compile_pin, compiled, recipe, deadline):
    reviewed_schema = recipe["schema"]
    require(reviewed_schema == arch.CANDIDATE_SCHEMA and recipe["cuda_architecture_owner"] == arch_pin
            and recipe["cuda_architectures"] == arch.ARCHITECTURES,
            "reviewed configuration does not bind the exact architecture owner")
    code_pin = aggregate_recipe["cuda_code_receipt"]
    root = Path(recipe["fresh_output"])
    require(Path(code_pin["path"]) == root / "fresh-cuda-code.json",
            "code receipt must be the exact same-build retained code inventory")
    component = compiler.load(compiler.OWNERS["guards"], "architecture_archive_guards")
    component.guard(recipe, deadline=deadline)
    guard = compiler.output_guard(component, recipe)
    stage_paths = [root / "logs" / (name + ".receipt.json") for name, _ in CENSUS_STAGES]
    guard([Path(code_pin["path"]), *stage_paths], True, deadline)
    code = compiler.read_pinned(pins, code_pin, deadline)
    check_codes(code, arch, arch_pin, recipe, compile_pin, compiled)
    tool = pins.check_pin(code["inspector"], deadline)
    provider = pins.check_pin(code["provider"], deadline)
    require(tool == arch.INSPECTOR and provider == root / "b/Release/onnxruntime_providers_cuda.dll",
            "CUDA inspector or provider outside the approved same-build paths")
    for row in (arch_pin, code_pin):
        pins.check_pin(row, deadline)
    cpu = compiler.load(compiler.OWNERS["cpu"], "architecture_archive_cpu")
    reports = []
    verified = [code_pin, arch_pin, code["provider"], code["inspector"]]
    last = compiler.utc(compiled["finished_utc"])
    for (stage, flag), stage_pin in zip(CENSUS_STAGES, code["stage_receipts"]):
        expected = root / "logs" / (stage + ".receipt.json")
        require(Path(stage_pin["path"]) == expected, "CUDA code stage outside its exact original receipt")
        proof = compiler.read_pinned(pins, stage_pin, deadline)
        # Guard only the exact stage files, whose names are verified again by
        # the retained command owner before any original bytes are consumed.
        stage_outputs = [expected.with_suffix("").with_suffix(suffix) for suffix in compiler.STAGE_SUFFIXES.values()]
        guard(stage_outputs, True, deadline)
        argv = [str(tool), "--all-fatbin", flag, str(provider)]
        actual = compiler.verify_stage(pins, cpu, root, proof, stage, argv,
                                       compiled["deadline_monotonic"], deadline)
        process_tree = proof["actual_tree_result"]
        start = compiler.utc(process_tree["start_utc"])
        finished = compiler.utc(process_tree["finished_utc"])
        require(actual == stage_pin and last <= start <= finished,
                "CUDA code census did not follow settled compile in the original order")
        last = finished
        stdout = [item for item in proof["files"] if Path(item["path"]).name == stage + ".stdout.log"]
        require(len(stdout) == 1, "exact original CUDA inspector stdout required")
        raw = arch.exact_bytes(Path(stdout[0]["path"]), stdout[0]["sha256"])
        require(len(raw) == stdout[0]["bytes"], "CUDA inspector stdout length differs")
        reports.append(raw.decode("utf-8"))
        verified.extend([stage_pin, *proof["files"]])
    observed = arch.code_inventory(*reports)
    require(all(code[key] == value for key, value in observed.items()),
            "retained CUDA code inventory disagrees with actual inspector stdout")
    return verified


def aggregate_owner():
    arch, arch_pin = frozen("fresh_architectures.py", ARCH_SHA)
    _, compiler, _ = arch.modules()
    projection, _ = frozen("fresh_projection.py", PROJECTION_SHA, [
        ('"airk.private.cuda-fresh-configure-recipe/1"', '"' + arch.CANDIDATE_SCHEMA + '"'),
    ])
    projection.BUILD_SCHEMA = BUILD_SCHEMA
    aggregate, _ = frozen("fresh_aggregate.py", AGGREGATE_SHA, [
        ('and set(recipe) == {"schema", "repository_root", "compile_receipt", "owners", "archiver",',
         'and set(recipe) == {"schema", "repository_root", "compile_receipt", "cuda_code_receipt", "owners", "archiver",'),
        ('    verification_finished_utc = dt.datetime.now(dt.timezone.utc)',
         '    code_pins = verify_codes(compile_owner, pins, recipe, compile_pin, compiled, configure_recipe, parent_deadline)\n'
         '    verification_finished_utc = dt.datetime.now(dt.timezone.utc)'),
        ('    build = projection.project_build(compile_pin, compiled, configured, configure_recipe, canonical=pins.canonical)',
         '    build = projection.project_build(compile_pin, compiled, configured, configure_recipe, canonical=pins.canonical)\n'
         '    build.update(architecture_owner=configure_recipe["cuda_architecture_owner"],\n'
         '                 cuda_code_receipt=recipe["cuda_code_receipt"])'),
        ('        "plan_sha256": pins.canonical(plan),',
         '        "plan_sha256": pins.canonical(plan),\n'
         '        "architecture_build_proof": build,'),
        ('                    response_pin, output_pin, *compiled["ninja"], *selected, *recipe["owners"].values()]:',
         '                    response_pin, output_pin, *compiled["ninja"], *selected, *code_pins, *recipe["owners"].values()]:'),
    ])
    aggregate.__file__ = str(SELF)
    aggregate.RECIPE_SCHEMA, aggregate.RECEIPT_SCHEMA = RECIPE_SCHEMA, AGGREGATE_SCHEMA
    stock_load = aggregate.load
    adapted = {HERE / "fresh_compile.py": compiler, HERE / "fresh_projection.py": projection}

    def load(path, name):
        candidate = adapted.get(Path(path))
        return candidate if candidate is not None else stock_load(path, name)

    aggregate.load = load
    aggregate.verify_codes = lambda *args: verify_codes(arch, arch_pin, *args)
    return aggregate


def main():
    require(os.name == "nt" and sys.flags.no_site and sys.flags.dont_write_bytecode,
            "verified Windows human interpreter with -B -S required")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", type=Path, required=True)
    parser.add_argument("--recipe-sha256", required=True)
    args = parser.parse_args()
    receipt = aggregate_owner().execute(args.recipe, args.recipe_sha256)
    print(json.dumps(receipt, allow_nan=False))


if __name__ == "__main__":
    main()
