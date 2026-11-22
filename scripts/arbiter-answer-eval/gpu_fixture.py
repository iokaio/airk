# SPDX-License-Identifier: Apache-2.0
"""CUDA lifetimes of the Answer-evaluation fixture (Windows, RTX A6000).

The CPU launcher ``fixture.py`` owns the evaluation installation. This module
reuses it unchanged and runs one lifetime on the CUDA selector
``cuda_answer::same_install::actual_private_arbiter_fixture_cuda_same_install``.
That selector reopens the same installation as the CPU selector (same control
root, catalog, CPU weights and indexes) and adds one CUDA Context profile,
Compact or Large, to the evaluation catalog only (or both with `--model pair`,
switched by `stage`; see cuda_pair.py). Its weights are read from
the pinned roster's files and never copied into the installation.

Commands: ``start`` (blocks for the lifetime), ``stage`` (CPU or CUDA
profile names), ``answer`` (one timed Answer with device evidence),
``compare`` (CPU against GPU evidence digest) and ``stop``. See
GPU_FIXTURE.md beside this file.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module  # dataclasses resolve their module through sys.modules
    spec.loader.exec_module(module)
    return module


FX = _load("answer_eval_fixture", HERE / "fixture.py")
EVIDENCE = _load("answer_eval_evidence", HERE / "evidence.py")

SELECTOR = ("service_runtime::evaluation_fixture_tests::cuda_answer::same_install::"
            "actual_private_arbiter_fixture_cuda_same_install")
GPU_TRACK = FX.EVALUATION / "gpu-track"
SYNTHETIC = "synthetic"
# One CUDA Context profile per lifetime: its single-entry roster and the
# reviewed final-graph placement result its scorer is opened against.
MODELS = {
    "compact": {
        "profile_name": "context-compact-cuda",
        "manifest": "840bd478675cbf03f2477c170401af75027d55de82b6ab69933f971a1104531d",
        "roster": GPU_TRACK / "inputs/cuda-compact-roster.json",
        "roster_sha256": "8b2af55e13e0e7109daed4ab7f06597f4291379157702508a45ec76698f56347",
        "graph": FX.CAMPAIGN / "fp32-paired-answer-r1/reviewed-gpu-graph.json",
        "graph_sha256": "2f4d3211e0bbf097c702acf90dcce046552a3e2d75dc49b2f5945d7988c76fee",
    },
    "large": {
        "profile_name": "context-large-cuda",
        "manifest": "7e4481caaf7034a70779702edad73a0eb08321418d1b785a4b7f4f891522bdac",
        "roster": GPU_TRACK / "inputs/cuda-large-roster.json",
        "roster_sha256": "932aab7b8ea0eb4190bc2a1d30561494aea59d748881106b1550928ae357483c",
        "graph": FX.CAMPAIGN / "large-fp32-batch-view-placement-r1/native-result.json",
        "graph_sha256": "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431",
    },
}
CUDA_PROFILES = {spec["profile_name"]: {"stage": "context", "device": "cuda", "manifest": spec["manifest"]}
                 for spec in MODELS.values()}
# Change set 1 with Context on CUDA: the change-set lifetime plus one CUDA
# calibration-only provisional-read variant of a reviewed base (MODELS[...]).
CS1_SELECTOR = ("service_runtime::evaluation_fixture_tests::cuda_answer::cs1::"
                "actual_private_arbiter_fixture_change_set_1_cuda")
CS1_BASES = {
    "compact": {"path": FX.CAMPAIGN / "fp32-paired-answer-r1/reviewed-gpu-profile.json",
                "sha256": "be0619b5db8e5e4c5414c9870e5a1da7533bdecfeac67d20ff4b6f4f7d90dcb8"},
    "large": {"path": FX.CAMPAIGN / "large-fp32-batch-view-metadata-r1/profile.json",
              "sha256": "81025bbb76529a58d91e1b02d4e2923a88bc283713478456d79fd9294f8cfa56"},
}


def cs1_environment(args):
    """Service variables of the change-set CUDA selector; every input is SHA-pinned."""
    base = CS1_BASES[args.model]
    change_set = FX.pin({"path": args.change_set_inputs, "sha256": args.change_set_inputs_sha256})
    variant = FX.pin({"path": args.cuda_variant_inputs, "sha256": args.cuda_variant_inputs_sha256})
    FX.pin({"path": str(base["path"]), "sha256": base["sha256"]})
    env = {"AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS": str(change_set),
           "AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS_SHA256": args.change_set_inputs_sha256,
           "AIRK_ARBITER_EVALUATION_ANSWER_LAYERS": args.layers,
           # The variant replaces the reviewed base's single-entry roster.
           "AIRK_ARBITER_EVALUATION_CUDA_INPUTS": str(variant),
           "AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256": args.cuda_variant_inputs_sha256,
           "AIRK_ARBITER_EVALUATION_CUDA_BASE_PROFILE": str(base["path"]),
           "AIRK_ARBITER_EVALUATION_CUDA_BASE_PROFILE_SHA256": base["sha256"]}
    if not args.restart_from:
        require(args.seed_cpu in SEEDS, "a fresh change-set lifetime names --seed-cpu")
        env["AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU"] = args.seed_cpu
    return env
CUDA_RUNTIME = "onnxruntime-cuda/1.30.0/device-execution/"
SEEDS = ("all", "focus", "none")

require, read_json, write_json, sha256, utc = FX.require, FX.read_json, FX.write_json, FX.sha256, FX.utc


def profiles():
    """CPU names from fixture.py plus the CUDA Context names, with their manifests."""
    merged = {name: dict(row) for name, row in FX.PROFILES.items() if row.get("manifest")}
    merged.update(CUDA_PROFILES)
    return merged


PAIR = "pair"
PAIR_SCHEMA = "airk.private.answer-eval-cuda-pair/1"


def read_pair(path, digest):
    """The pinned two-entry inputs written by cuda_pair.py pair (plan 12.5.2 item 4)."""
    pair = read_json(FX.pin({"path": str(path), "sha256": digest}))
    require(pair.get("schema") == PAIR_SCHEMA, "not a CUDA pair spec")
    FX.pin(pair["roster"])
    require(1 <= len(pair["reviews"]) <= 2 and all(set(r) == {"path", "sha256", "profile_sha256"}
                                                   for r in pair["reviews"]), "one review pin per CUDA entry")
    for review in pair["reviews"]:
        FX.pin(review)
    uuid = pair["adapter_uuid"]
    require(isinstance(uuid, list) and len(uuid) == 16 and any(uuid), "the pair names no adapter")
    for name, row in pair["profiles"].items():
        require(name not in FX.PROFILES and row["device"] == "cuda" and row["stage"] in ("focus", "context")
                and len(row["manifest"]) == 64, f"pair row {name} must be a new CUDA name with a manifest")
    return pair


def pair_environment(path, digest, seed=None):
    """Service variables of a two-entry lifetime: the roster, one review pin per entry, the adapter."""
    pair = read_pair(path, digest)
    env = {"AIRK_ARBITER_EVALUATION_CUDA_INPUTS": pair["roster"]["path"],
           "AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256": pair["roster"]["sha256"],
           "AIRK_ARBITER_EVALUATION_CUDA_REVIEWS": json.dumps(pair["reviews"]),
           "AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID": json.dumps(pair["adapter_uuid"])}
    if seed is not None:
        require(seed in SEEDS, "seed is all, focus or none")
        env["AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU"] = seed
    return env


def lifetime_rows(life):
    """The CUDA profile rows a lifetime was started with (gpu.json), so `stage` names them."""
    path = Path(life) / "gpu.json"
    if not path.is_file():
        return {}
    return {name: {key: row[key] for key in ("stage", "device", "manifest")}
            for name, row in (read_json(path).get("cuda_rows") or {}).items()}


def cuda_environment(model, seed=None):
    spec = MODELS[model]
    roster = FX.pin({"path": str(spec["roster"]), "sha256": spec["roster_sha256"]})
    graph = FX.pin({"path": str(spec["graph"]), "sha256": spec["graph_sha256"]})
    adapter = read_json(graph)["adapter_before"]["uuid"]
    require(isinstance(adapter, list) and len(adapter) == 16 and any(adapter), "reviewed adapter identity missing")
    env = {"AIRK_ARBITER_EVALUATION_CUDA_INPUTS": str(roster),
           "AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256": spec["roster_sha256"],
           "AIRK_ARBITER_EVALUATION_CUDA_GRAPH": str(graph),
           "AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256": spec["graph_sha256"],
           "AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID": json.dumps(adapter)}
    if seed is not None:
        require(seed in SEEDS, "seed is all, focus or none")
        env["AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU"] = seed
    return env


_FX_EMITTER_ENVIRONMENT = FX.emitter_environment


def emitter_environment(handle, scopes, stage, enabled, manifest, prefix, roots):
    """fixture.py's emitter environment; the read-only declaration names only roots outside
    artifacts (the Rust emitter refuses a declaration naming a root inside artifacts, such as
    the synthetic proving tree)."""
    env = _FX_EMITTER_ENVIRONMENT(handle, scopes, stage, enabled, manifest, prefix, roots)
    artifacts = (FX.ROOT / "artifacts").resolve()
    outside = [root for root in roots if not Path(root).resolve().is_relative_to(artifacts)]
    if outside:
        env["AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES"] = FX.external_declaration(outside)
    else:
        env.pop("AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES", None)
    return env


# --- lifetime -----------------------------------------------------------------------------------


def worker(args):
    """fixture.py's worker with the CUDA selector and its pinned inputs in the service environment."""
    # fixture.py's worker launches SELECTORS[args.selector] and adds the change-set
    # variables itself (selector_extras); this launcher adds the CUDA ones.
    args.search = None
    if args.change_set_inputs:
        FX.SELECTORS["cs1-cuda"] = CS1_SELECTOR
        args.selector, args.answer_layers = "cs1-cuda", args.layers
        cuda = cuda_environment(args.model)
        cuda.update({key: value for key, value in cs1_environment(args).items()
                     if key.startswith("AIRK_ARBITER_EVALUATION_CUDA_")})
        FX.ENVIRONMENT.update(cuda)
    else:
        FX.SELECTOR = FX.SELECTORS["software-v2"] = SELECTOR
        args.selector, args.answer_layers = "software-v2", None
        seed = None if args.restart_from else args.seed_cpu
        args.seed_cpu = None  # the same-install seed travels in its own variable below
        if args.model == PAIR:
            FX.ENVIRONMENT.update(pair_environment(args.pair, args.pair_sha256, seed))
        else:
            FX.ENVIRONMENT.update(cuda_environment(args.model, seed))
    if args.synthetic_root:
        FX.REPOSITORIES[SYNTHETIC] = Path(args.synthetic_root)
        FX.EXCLUSIONS[SYNTHETIC] = []
    FX.worker(args)


def start(args):
    owner = runpy.run_path(str(FX.OWNER))
    fixtures = Path(args.fixtures)
    life = FX.lifetime_dir(args.lifetime, fixtures)
    require(fixtures.is_dir() and not life.exists(), "a fresh lifetime folder under an existing fixtures root is required")
    require(0 < args.seconds <= FX.MAX_SECONDS, f"the lifetime is at most {FX.MAX_SECONDS} s")
    require(bool(args.restart_from) != bool(args.seed_cpu), "a fresh lifetime names --seed-cpu; a restart never seeds")
    for repo in args.repositories:
        require(repo in FX.REPOSITORIES or (repo == SYNTHETIC and args.synthetic_root), "unknown repository " + repo)
    images = FX.image_pins(Path(args.image))
    # Pins checked before launch.
    pair = None
    if args.model == PAIR:
        require(bool(args.pair and args.pair_sha256) and not args.change_set_inputs,
                "--model pair needs --pair and --pair-sha256, without change-set inputs")
        pair = read_pair(args.pair, args.pair_sha256)
        pair_environment(args.pair, args.pair_sha256, args.seed_cpu)
        cs1 = None
    elif args.change_set_inputs:
        cuda_environment(args.model)
        cs1 = cs1_environment(args)
    else:
        cuda_environment(args.model, args.seed_cpu)
        cs1 = None
    life.mkdir()
    write_json(life / "gpu.json", {
        "schema": "airk.private.answer-eval-gpu-lifetime/1", "at": utc(), "model": args.model,
        "selector": CS1_SELECTOR if cs1 else SELECTOR, "change_set": cs1,
        "image": images, "seed_cpu": args.seed_cpu, "restart_from": args.restart_from,
        "cuda": ({"pair": str(args.pair), "pair_sha256": args.pair_sha256} if pair
                 else {k: str(v) for k, v in MODELS[args.model].items()}),
        "cuda_rows": pair["profiles"] if pair else CUDA_PROFILES,
        "helpers": {"gpu_fixture.py": sha256(Path(__file__)), "fixture.py": sha256(HERE / "fixture.py"),
                    "evidence.py": sha256(HERE / "evidence.py")}})
    argv = [sys.executable, "-I", "-B", "-S", str(Path(__file__).resolve()), "--fixtures", str(fixtures),
            "worker", "--lifetime-path", str(life), "--image", str(Path(args.image).resolve()),
            "--inputs", str(args.inputs), "--inputs-sha256", args.inputs_sha256, "--seconds", str(args.seconds),
            "--repositories", ",".join(args.repositories), "--model", args.model]
    if pair:
        argv += ["--pair", str(Path(args.pair).resolve()), "--pair-sha256", args.pair_sha256]
    if args.restart_from:
        argv += ["--restart-from", args.restart_from]
    if args.seed_cpu:
        argv += ["--seed-cpu", args.seed_cpu]
    if args.synthetic_root:
        argv += ["--synthetic-root", str(Path(args.synthetic_root).resolve())]
    if args.change_set_inputs:
        argv += ["--change-set-inputs", args.change_set_inputs, "--change-set-inputs-sha256",
                 args.change_set_inputs_sha256, "--cuda-variant-inputs", args.cuda_variant_inputs,
                 "--cuda-variant-inputs-sha256", args.cuda_variant_inputs_sha256, "--layers", args.layers]
    if not args.dispatch_observer:
        argv.append("--no-dispatch-observer")
    before =(sha256(Path(__file__)), sha256(HERE / "fixture.py"))
    result = owner["run_evaluation_fixture"](argv, FX.ROOT, life / "outer.log", args.seconds + 600,
                                             os.environ | {FX.OWNER_VARIABLE: str(life)})
    result["scripts_sha256_unchanged"] = (sha256(Path(__file__)), sha256(HERE / "fixture.py")) == before
    write_json(life / "owned.json", result)
    handle = read_json(life / "handle.json") if (life / "handle.json").is_file() else {}
    ok = (owner["successful"](result) and result["owned_job_final_active_processes"] == 0
          and result["owned_job_final_member_pids"] == [] and handle.get("state") == "closed")
    print(json.dumps({"lifetime": life.name, "state": handle.get("state"), "settled": ok,
                      "stop_reason": handle.get("stop_reason")}))
    require(ok, "the GPU fixture lifetime did not settle; see " + str(life))


# --- stages and Answer --------------------------------------------------------------------------


def set_stage(life, handle, profile_name, enabled, repositories, seconds=900, row=None):
    """fixture.set_stage for CPU and CUDA profile names (one CUDA Context profile per lifetime).
    `row` names a profile by manifest ({stage, device, manifest}), such as a change-set
    catalog-only profile or a CUDA provisional-read variant listed in this lifetime's Ready."""
    table = profiles()
    # The lifetime's own CUDA rows (a pair's Compact, Large and any CUDA Focus row).
    table.update(lifetime_rows(life))
    if row is not None:
        require(row["stage"] in ("focus", "context") and row["device"] in ("cpu", "cuda")
                and len(row["manifest"]) == 64, "a manifest row names stage, device and manifest")
        table[profile_name] = row
    require(profile_name in table, "unknown profile " + profile_name)
    profile = table[profile_name]
    saved = FX.PROFILES.get(profile_name)
    # fixture.set_stage runs only rows marked "cpu" (its CUDA rows name what was missing);
    # in this launcher a CUDA row is served by the CUDA selector, so it passes as runnable.
    FX.PROFILES[profile_name] = dict(profile, device="cpu")
    FX.emitter_environment = emitter_environment
    try:
        record = FX.set_stage(life, handle, profile_name, enabled, repositories, seconds)
    finally:
        FX.emitter_environment = _FX_EMITTER_ENVIRONMENT
        if saved is None:
            FX.PROFILES.pop(profile_name, None)
        else:
            FX.PROFILES[profile_name] = saved
    record["device"] = profile["device"]
    return record


def stage(args):
    life, handle = FX.load_handle(args)
    repositories = args.repositories or list(handle["indexes"])
    row = None
    if args.manifest:
        row = {"stage": args.stage_name, "device": args.device, "manifest": args.manifest}
    record = set_stage(life, handle, args.profile, args.enable, repositories, row=row)
    with (life / "stages.jsonl").open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(record, sort_keys=True) + "\n")
    print(json.dumps(record))


def gpu_memory():
    """Read-only nvidia-smi observation of the adapter (used and total MiB)."""
    try:
        line = subprocess.run(["nvidia-smi", "--query-gpu=uuid,memory.used,memory.total", "--format=csv,noheader,nounits"],
                              capture_output=True, text=True, timeout=30, check=True).stdout.strip().splitlines()[0]
        uuid, used, total = (part.strip() for part in line.split(","))
        return {"uuid": uuid, "used_mib": int(used), "total_mib": int(total), "at": utc()}
    except (OSError, subprocess.SubprocessError, ValueError, IndexError) as error:
        return {"error": f"{type(error).__name__}: {error}"}


def observer_rows(work, after_sequence):
    path = Path(work) / "composed-observer.jsonl"
    if not path.is_file():
        return []
    rows = [json.loads(line) for line in path.read_bytes().splitlines() if line.strip()]
    return [row for row in rows if row["sequence"] > after_sequence]


def device_evidence(body, rows):
    """What ran where: Context advice runtimes and profiles, and the native dispatch census."""
    advice = []
    for event in body.get("assessment_history") or []:
        result = event.get("result") or {}
        item = result.get("advice") or {}
        advice.append({"status": result.get("status"), "profile": item.get("profile"),
                       "runtime": item.get("runtime"), "observed_input_tokens": item.get("observed_input_tokens")})
    dispatch = {}
    for row in rows:
        group = dispatch.setdefault(row["dispatch_identity"], {"profile": row["profile_manifest"],
                                                               "task": row["task"], "kinds": []})
        group["kinds"].append(row["kind"])
        if row["kind"] == "score_returned":
            group.update(outcome=row.get("outcome"), finite=row.get("finite_raw_scores"),
                         raw_scores=row.get("raw_scores"), work=row.get("work"))
    return {"advice": advice, "dispatch": list(dispatch.values()),
            "cuda_advice": sum(1 for a in advice if str(a.get("runtime") or "").startswith(CUDA_RUNTIME)),
            "cpu_advice": sum(1 for a in advice if str(a.get("runtime") or "").startswith("onnxruntime-cpu/"))}


def answer(args):
    life, handle = FX.load_handle(args)
    work = Path(handle["work"])
    repo = args.repository
    index_id = handle["indexes"][repo]["index_id"]
    commands = FX.client_commands(life, handle, "gpu-answer-commands", 900)
    request = commands.folder / f"{commands.prefix}-{args.label}.args.json"
    arguments = FX.answer_arguments(work, args.question)
    if args.answer_defaults:
        # Merged like the harness's answer_defaults (change set 1 sends its own budget per call).
        defaults = json.loads(args.answer_defaults)
        require(isinstance(defaults, dict) and set(defaults) <= {"budget"}, "answer defaults name only budget")
        arguments = arguments | defaults
    write_json(request, arguments)
    status_before = commands.json(f"{args.label}-status-before", "arbiter", "status-legacy")
    rows_before = observer_rows(work, 0)
    sequence = rows_before[-1]["sequence"] if rows_before else 0
    memory_before = gpu_memory()
    out, elapsed = commands.run(f"{args.label}-answer", "tools", "call", "airk_answer", "--index", index_id,
                                "--args-file", str(request))
    memory_after = gpu_memory()
    envelope = read_json(out)
    is_error = envelope.get("isError") is True
    body = None if is_error else FX.tool_body(envelope)
    # The observer writes settlement rows after the reply; wait briefly for this call's rows to settle.
    end = time.monotonic() + 30
    rows = observer_rows(work, sequence)
    while time.monotonic() < end and any(k == "dispatch_admitted" for k in (r["kind"] for r in rows)) \
            and sum(r["kind"] == "charge_released" for r in rows) < sum(r["kind"] == "dispatch_admitted" for r in rows):
        time.sleep(0.5)
        rows = observer_rows(work, sequence)
    record = {
        "schema": "airk.private.answer-eval-gpu-answer/1", "at": utc(), "lifetime": life.name,
        "label": args.label, "repository": repo, "index_id": index_id,
        "request": {"path": str(request), "sha256": sha256(request)},
        "result": {"path": str(out), "sha256": sha256(out), "bytes": out.stat().st_size},
        "elapsed_ms": round(elapsed / 1e6, 1), "is_error": is_error,
        "stages": {s: {k: status_before[s].get(k) for k in ("enabled", "availability", "active_profile",
                                                             "selected_profile")} for s in ("focus", "context")},
        "gpu_memory": {"before": memory_before, "after": memory_after},
    }
    if body is not None:
        record.update(stop=body.get("stop"), records=len(body.get("records") or []),
                      evidence_digest=EVIDENCE.evidence_digest(body), device=device_evidence(body, rows))
    else:
        record["error"] = json.dumps(envelope)[:4000]
    pinned = write_json(Path(args.output), record)
    print(json.dumps({"label": args.label, "elapsed_ms": record["elapsed_ms"], "is_error": is_error,
                      "stop": record.get("stop"), "evidence_digest": record.get("evidence_digest"), **pinned}))


def compare(args):
    """Equal evidence for the same request on CPU and on GPU (formats: evidence.evidence_digest)."""
    rows = [read_json(path) for path in args.records]
    require(len(rows) >= 2 and all(not row["is_error"] for row in rows), "two successful Answer records required")
    requests = {read_json(row["request"]["path"])["question"] for row in rows}
    require(len(requests) == 1, "the records answer different questions")
    digests = {row["label"]: row["evidence_digest"] for row in rows}
    result = {"schema": "airk.private.answer-eval-gpu-comparison/1", "at": utc(),
              "question": next(iter(requests)), "digests": digests,
              "equal_evidence": len(set(digests.values())) == 1,
              "elapsed_ms": {row["label"]: row["elapsed_ms"] for row in rows},
              "records": [{"path": str(path), "sha256": sha256(path)} for path in args.records]}
    pinned = write_json(Path(args.output), result)
    print(json.dumps({"equal_evidence": result["equal_evidence"], **pinned}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", default=str(FX.FIXTURES))
    commands = parser.add_subparsers(dest="command", required=True)

    def lifetime_options(sub):
        sub.add_argument("--model", choices=sorted([*MODELS, PAIR]), required=True)
        # --model pair: the pinned two-entry inputs of cuda_pair.py pair.
        sub.add_argument("--pair")
        sub.add_argument("--pair-sha256")
        sub.add_argument("--inputs", default=str(FX.DEFAULT_INPUTS))
        sub.add_argument("--inputs-sha256", default=FX.DEFAULT_INPUTS_SHA256)
        sub.add_argument("--seconds", type=int, default=FX.MAX_SECONDS)
        sub.add_argument("--repositories", type=lambda v: [r for r in v.split(",") if r], default=["airk", "bookbuilder"])
        sub.add_argument("--restart-from")
        sub.add_argument("--seed-cpu", choices=SEEDS)
        sub.add_argument("--synthetic-root")
        sub.add_argument("--no-dispatch-observer", dest="dispatch_observer", action="store_false")
        sub.add_argument("--image", required=True)
        # Change set 1 with a CUDA provisional-read variant (all four or none).
        sub.add_argument("--change-set-inputs")
        sub.add_argument("--change-set-inputs-sha256")
        sub.add_argument("--cuda-variant-inputs")
        sub.add_argument("--cuda-variant-inputs-sha256")
        sub.add_argument("--layers", default="all")

    sub = commands.add_parser("start")
    sub.add_argument("--lifetime", required=True)
    lifetime_options(sub)
    sub = commands.add_parser("worker")
    sub.add_argument("--lifetime-path", required=True)
    lifetime_options(sub)
    sub = commands.add_parser("stage")
    sub.add_argument("--lifetime", required=True)
    sub.add_argument("--profile", required=True)
    group = sub.add_mutually_exclusive_group(required=True)
    group.add_argument("--enable", dest="enable", action="store_true")
    group.add_argument("--disable", dest="enable", action="store_false")
    sub.add_argument("--repositories", type=lambda v: [r for r in v.split(",") if r])
    sub.add_argument("--manifest", help="stage a profile of this lifetime's Ready by manifest")
    sub.add_argument("--stage-name", choices=("focus", "context"))
    sub.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    sub = commands.add_parser("answer")
    sub.add_argument("--lifetime", required=True)
    sub.add_argument("--repository", required=True)
    sub.add_argument("--question", required=True)
    sub.add_argument("--label", required=True)
    sub.add_argument("--output", required=True)
    sub.add_argument("--answer-defaults", help="JSON merged into the Answer arguments, for example a change set budget")
    sub = commands.add_parser("compare")
    sub.add_argument("--records", nargs="+", required=True)
    sub.add_argument("--output", required=True)
    sub = commands.add_parser("stop")
    sub.add_argument("--lifetime", required=True)
    args = parser.parse_args()
    {"start": start, "worker": worker, "stage": stage, "answer": answer, "compare": compare,
     "stop": FX.stop}[args.command](args)


if __name__ == "__main__":
    main()
