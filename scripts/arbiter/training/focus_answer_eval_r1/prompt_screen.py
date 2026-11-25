# SPDX-License-Identifier: Apache-2.0
"""Prepare or run the owned, original-weight Focus prompt screen."""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from importlib.metadata import version
import json
import math
import os
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import answer_worker as worker
import focus_text as text
import make_plan as owned
import worker_math as metrics


SCHEMA = "airk.private.focus-answer-eval-prompt-screen/1"
ARMS = ("publisher", "software_evidence_v1")
REPOS = ("bookbuilder", "munarium")
LOCAL_SETS = ("heldout_id", "heldout_path")
REPLAY_SETS = ("replay_dev", "replay_general_heldout")
WIDTH = 1024
BATCH = 4
GIB = 1024 ** 3


def paired_cohort(rows, per_cell):
    """Select identical whole held-out groups and one unchanged replay roster."""
    if not 1 <= per_cell <= 32:
        raise ValueError("groups per repository/split must be from 1 to 32")
    by_cell = defaultdict(dict)
    replay = []
    seen = set()
    for row in rows:
        split, repo = row["set"], row["repo"]
        if split == "train_sample":
            continue
        if row["id"] in seen:
            raise ValueError("duplicate screen row identity")
        seen.add(row["id"])
        for name in ("query", "document"):
            if text.sha256_text(row[name]) != row[name + "_sha256"]:
                raise ValueError("screen query or window differs from its retained hash")
        metrics.target(row["label"])
        if split in LOCAL_SETS and repo in REPOS and row["instruction"] == "publisher":
            by_cell[(repo, split)].setdefault(row["pair_id"], []).append(row)
        elif repo == "replay" and split in REPLAY_SETS and row["instruction"] in ARMS:
            replay.append(row)
        else:
            raise ValueError("unexpected prompt screen source or instruction")
    groups = []
    for repo in REPOS:
        for split in LOCAL_SETS:
            cell = by_cell[(repo, split)]
            ranked = []
            for pair_id, group in cell.items():
                if len({row["query_sha256"] for row in group}) != 1:
                    raise ValueError("a ranking group has different questions")
                if {row["label"] for row in group} == {0, 1}:
                    ranked.append((text.sha256_text("prompt-screen/1:" + pair_id), group))
            if not ranked:
                raise ValueError(f"empty comparable held-out cell: {repo}/{split}")
            groups.extend(group for _, group in sorted(ranked, key=lambda item: item[0])[:per_cell])
    if {row["set"] for row in replay} != set(REPLAY_SETS):
        raise ValueError("both independently retained replay sets are required")
    return groups, replay


def compile_one(tokenizer, row, arm):
    ids = text.compile_live_ids(tokenizer, text.INSTRUCTIONS[arm], row["query"], row["document"], WIDTH)
    if arm == row["instruction"] and ids != row["live_ids"]:
        raise ValueError("local compiler differs from the retained input tokens")
    return None if ids is None else {**row, "live_ids": ids}


def compile_matched(tokenizer, groups, replay):
    local = {arm: [] for arm in ARMS}
    counts = defaultdict(Counter)
    for group in groups:
        cell = counts[f"{group[0]['repo']}/{group[0]['set']}"]
        cell["offered_groups"] += 1
        cell["offered_rows"] += len(group)
        compiled = {arm: [compile_one(tokenizer, row, arm) for row in group] for arm in ARMS}
        for arm in ARMS:
            cell[arm + "_refusals"] += compiled[arm].count(None)
        if all(None not in compiled[arm] for arm in ARMS):
            for arm in ARMS:
                local[arm].extend(compiled[arm])
            cell["paired_groups"] += 1
            cell["paired_rows"] += len(group)
    unchanged = [compile_one(tokenizer, row, row["instruction"]) for row in replay]
    if None in unchanged:
        raise ValueError("unchanged replay contains a refused row")
    return local, unchanged, {name: dict(cell) for name, cell in sorted(counts.items())}


def score_pairs(torch, model, rows, time_left):
    result = []
    with torch.inference_mode():
        head = model.lm_head.weight[[metrics.NO_ID, metrics.YES_ID]]
        for chunk in metrics.batches(rows, BATCH):
            time_left()
            ids, mask = zip(*(metrics.left_pad(row["live_ids"], WIDTH) for row in chunk))
            tensor = lambda v: torch.tensor(v, dtype=torch.int64, device="cuda")
            hidden = model.model(
                input_ids=tensor(ids), attention_mask=tensor(mask),
                position_ids=tensor([list(range(WIDTH))] * len(chunk)),
                use_cache=False, return_dict=True,
            ).last_hidden_state[:, WIDTH - 1, :]
            logits = torch.nn.functional.linear(hidden, head).float().cpu().tolist()
            for row, (no, yes) in zip(chunk, logits, strict=True):
                if not math.isfinite(no) or not math.isfinite(yes):
                    raise ValueError("nonfinite original-model score")
                result.append([row["id"], no, yes])
    return result


def comparison(local, replay, score, refusals):
    fixed = ("id", "pair_id", "query", "document", "label", "repo", "set", "kind")
    identities = {arm: [tuple(row[key] for key in fixed) for row in local[arm]] for arm in ARMS}
    if identities["publisher"] != identities["software_evidence_v1"]:
        raise ValueError("prompt comparison rows are not matched")
    scores = {arm: score(local[arm]) if local[arm] else [] for arm in ARMS}
    arms = {arm: metrics.summarize(local[arm], scores[arm]) if local[arm] else {} for arm in ARMS}
    general = metrics.summarize(replay, score(replay))
    groups = defaultdict(lambda: defaultdict(list))
    for arm in ARMS:
        for row, (row_id, no, yes) in zip(local[arm], scores[arm], strict=True):
            if row_id != row["id"]:
                raise ValueError("original-model score identity or order differs")
            groups[(row["repo"], row["set"], row["pair_id"])][arm].append((row["label"], yes - no))
    changes = defaultdict(Counter)
    for (repo, split, _), group in groups.items():
        publisher, software = (metrics.ranking({0: group[arm]})["mrr"] for arm in ARMS)
        if publisher is None or software is None:
            raise ValueError("matched ranking group lacks a positive or a negative")
        change = "improved" if software > publisher else "worse" if software < publisher else "tied"
        changes[f"{repo}/{split}"][change] += 1
    delta = {}
    for key, control in arms["publisher"].items():
        candidate = arms["software_evidence_v1"][key]
        delta[key] = {metric: candidate[metric] - control[metric] if (
            candidate[metric] is not None and control[metric] is not None) else None
            for metric in ("auroc", "hit_at_1", "mrr", "mean_positive_margin")}
    return {"arms": arms, "software_minus_publisher": delta, "unchanged_replay": general,
            "paired_rank_changes": {cell: dict(counts) for cell, counts in sorted(changes.items())},
            "admission_by_cell": refusals, "replay_runs": 1}


def prepare(args):
    output, log = args.output.resolve(), args.log_dir.resolve()
    if output.exists() or log.exists() or not all(
        path.is_relative_to(owned.ROOT / "artifacts/v190/final-plan") for path in (output, log)
    ):
        raise ValueError("screen requires fresh checked outputs under the private campaign")
    if not args.base_python.is_absolute() or not args.base_python.is_file():
        raise ValueError("absolute pinned base Python required")
    if not 0 < args.wall_seconds <= 1800 or not 1 <= args.groups_per_cell <= 32:
        raise ValueError("screen requires bounded wall time and groups per cell")
    base = owned.pin(args.base_python)
    if base["sha256"] != args.base_python_sha256:
        raise ValueError("base Python hash differs")
    examples = args.examples.resolve(strict=True)
    manifest = owned.pin(examples / "manifest.json")
    if manifest["sha256"] != args.examples_manifest_sha256:
        raise ValueError("example manifest hash differs")
    declared = json.loads(worker.pinned(manifest).read_bytes())
    stats, evaluation = (owned.pin(examples / name) for name in ("stats.json", "eval.jsonl"))
    for pin in (stats, evaluation):
        if declared["outputs"].get(Path(pin["path"]).name) != pin["sha256"]:
            raise ValueError("example file differs from its manifest")
    summary = json.loads(worker.pinned(stats).read_bytes())
    if summary.get("instruction") != "publisher" or summary.get("width") != WIDTH:
        raise ValueError("screen requires examples built with the incumbent 1024 publisher prompt")
    inputs = declared.get("inputs") or {}
    if not any(Path(path).name == "corpus_admission.py" for path in inputs):
        raise ValueError("screen requires the protected L7 example build")
    paired_cohort(worker.read_rows(evaluation), args.groups_per_cell)

    launcher = owned.load(owned.PILOT / "launch_owned.py", "owned_prompt_screen_launcher")
    owner = owned.load(owned.OWNER, "owned_prompt_screen_gpu")
    nvidia_smi = str(Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32/nvidia-smi.exe")
    sources = [Path(__file__), owned.HERE / "answer_worker.py", owned.HERE / "make_plan.py",
               owned.HERE / "focus_text.py", owned.HERE / "worker_math.py", owned.OWNER,
               owned.PILOT / "support/candidate_overlay.py", *owned.PILOT.glob("*.py"), *owned.REFERENCE.glob("*.py")]
    pins = [owned.pin(path) for path in sources] + [stats, evaluation, manifest]
    sealed = {pin["path"]: pin for pin in pins}
    sha = {path: pin["sha256"] for path, pin in sealed.items()}
    env = {"PYTHONPYCACHEPREFIX": launcher.FRESH_PYCACHE, "PYTHONDONTWRITEBYTECODE": "1",
           "PYTHONIOENCODING": "utf-8", "PYTHONPATH": os.pathsep.join(
               map(str, (owned.HERE, owned.PILOT, owned.REFERENCE, owned.SITE))),
           "CUBLAS_WORKSPACE_CONFIG": ":4096:8", "CUDA_MODULE_LOADING": "LAZY",
           "HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1", "HF_HUB_DISABLE_TELEMETRY": "1",
           "TORCH_HOME": str(log / "cache/torch"), "CUDA_CACHE_PATH": str(log / "cache/cuda"),
           "TOKENIZERS_PARALLELISM": "false", "CUDA_VISIBLE_DEVICES": "0"}
    plan = {"schema": SCHEMA, "mode": "score_only", "human_sid": owned.HUMAN,
            "repository_root": str(owned.ROOT), "native_census": stats, "native_owned_receipt": None,
            "cohort": evaluation, "replay": manifest, "general_replay_instruction": "as_frozen",
            "gpu": owner.gpu_observation(nvidia_smi), "nvidia_smi": nvidia_smi, "versions": owned.VERSIONS,
            "base_python": base, "reference_module": sealed[str((owned.REFERENCE / "run_focus_export.py").resolve())],
            "reference_worker": None, "ownership_module": sealed[str(owned.OWNER)],
            "human_identity_helper": sealed[str(owned.PILOT / "support/candidate_overlay.py")],
            "worker": sealed[str(Path(__file__).resolve())], "optimizer": None,
            "execution_sources": sealed, "input_seals": sha, "immutable_input_seals": sha,
            "environment": env, "log_dir": str(log), "worker_output": str(log / "worker"),
            "groups_per_cell": args.groups_per_cell,
            "limits": {"wall_seconds": args.wall_seconds, "host_committed_bytes": 12 * GIB,
                       "cuda_allocator_bytes": 12 * GIB, "minimum_cuda_free_bytes": 14 * GIB}}
    plan["continuity_key"] = launcher.continuity_key(plan)
    with output.open("x", encoding="utf-8", newline="\n") as destination:
        destination.write(json.dumps(plan, indent=2) + "\n")
    print(json.dumps(owned.pin(output)))


def run(plan_path, plan_sha):
    if worker.digest(plan_path) != plan_sha or os.environ.get("AIRK_F2_START_GATE") != "1":
        raise ValueError("exact owned prompt screen required")
    plan = json.loads(plan_path.read_bytes())
    if plan.get("schema") != SCHEMA or plan.get("mode") != "score_only" or plan.get("optimizer") is not None:
        raise ValueError("only original-weight score mode is supported")
    print("AIRK_F2_GATE_READY:" + str(os.getpid()), flush=True)
    message = sys.stdin.readline().strip().split()
    if len(message) != 3 or message[:2] != ["GO", plan_sha]:
        raise ValueError("owned GO/source binding absent")
    deadline = float(message[2])

    def time_left():
        if time.monotonic() >= deadline:
            raise TimeoutError("owned prompt screen deadline expired")

    if (not math.isfinite(deadline) or plan["human_sid"] != owned.HUMAN
            or not 0 < plan["limits"]["wall_seconds"] <= 1800
            or plan["limits"]["host_committed_bytes"] != 12 * GIB
            or plan["limits"]["cuda_allocator_bytes"] != 12 * GIB
            or plan["limits"]["minimum_cuda_free_bytes"] != 14 * GIB):
        raise ValueError("prompt screen limits differ")
    for name, value in plan["environment"].items():
        if os.environ.get(name) != value:
            raise ValueError("sealed environment differs: " + name)
    for name, expected in plan["versions"].items():
        if version(name) != expected:
            raise ValueError("runtime version differs: " + name)
    for pin in plan["execution_sources"].values():
        worker.pinned(pin)
    time_left()

    def audit(event, _args):
        if event in ("subprocess.Popen", "os.system", "os.posix_spawn", "os.fork", "socket.connect"):
            raise RuntimeError("prompt screen cannot spawn a process or use the network")

    sys.addaudithook(audit)
    output = Path(plan["worker_output"])
    if output != Path(plan["log_dir"]) / "worker" or output.exists():
        raise ValueError("fresh owned worker output required")
    output.mkdir()
    report = output / "result.json"
    result = {"schema": SCHEMA, "mode": "score_only", "plan_sha256": plan_sha, "complete": False,
              "gradient_steps": 0, "qualification": False, "promotion": False,
              "scope": "Original FP32 publisher weights on Torch CUDA, local compiler, fixed admitted examples. "
                       "No native AIRK CUDA profile/placement, served ranking, or training conclusion."}
    started = time.monotonic()
    torch = None
    try:
        groups, replay = paired_cohort(worker.read_rows(plan["cohort"]), plan["groups_per_cell"])
        builder_stats = json.loads(worker.pinned(plan["native_census"]).read_bytes())
        result["input_builder_refusals"] = builder_stats.get("refusals", {})
        source = worker.load_module(plan["reference_module"], "original_focus_prompt_screen")
        result["original_asset_sha256"] = source.verify_assets()
        tokenizer = source.local_tokenizer()
        if (tokenizer.pad_token_id != metrics.PAD_ID or tokenizer.encode("no") != [metrics.NO_ID]
                or tokenizer.encode("yes") != [metrics.YES_ID]):
            raise ValueError("publisher scoring tokenizer ABI differs")
        local, unchanged, refusals = compile_matched(tokenizer, groups, replay)
        time_left()

        import torch
        torch.manual_seed(1908)
        torch.set_num_threads(4)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        torch.backends.cuda.matmul.allow_tf32 = False
        torch.backends.cudnn.allow_tf32 = False
        if not torch.cuda.is_available() or torch.cuda.device_count() != 1:
            raise ValueError("exact reviewed CUDA device required")
        device = torch.cuda.get_device_properties(0)
        free, total = torch.cuda.mem_get_info()
        device_uuid = str(device.uuid)
        if (device.name != plan["gpu"]["name"] or free < plan["limits"]["minimum_cuda_free_bytes"]
                or device_uuid.lower().removeprefix("gpu-") != plan["gpu"]["uuid"].lower().removeprefix("gpu-")):
            raise ValueError("reviewed CUDA device or free reserve differs")
        torch.cuda.set_per_process_memory_fraction(plan["limits"]["cuda_allocator_bytes"] / total, 0)
        torch.cuda.reset_peak_memory_stats()
        model = source.load_official(torch, "cuda").eval()
        result["gpu_uuid"] = device_uuid
        result["comparison"] = comparison(local, unchanged, lambda rows: score_pairs(torch, model, rows, time_left), refusals)
        torch.cuda.synchronize()
        if torch.cuda.max_memory_reserved() > plan["limits"]["cuda_allocator_bytes"]:
            raise ValueError("CUDA peak exceeded allocator cap")
        time_left()
        for pin in plan["execution_sources"].values():
            worker.pinned(pin)
        time_left()
        result["complete"] = True
    except BaseException as error:
        result["failure"] = {"type": type(error).__name__, "message": str(error)[:2048]}
        raise
    finally:
        result["elapsed_seconds_after_GO"] = time.monotonic() - started
        if torch is not None and torch.cuda.is_initialized():
            result["peak_cuda_allocated_bytes"] = torch.cuda.max_memory_allocated()
            result["peak_cuda_reserved_bytes"] = torch.cuda.max_memory_reserved()
        with report.open("x", encoding="utf-8", newline="\n") as destination:
            destination.write(json.dumps(result, indent=2, allow_nan=False) + "\n")


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    parser = argparse.ArgumentParser(description=__doc__)
    if args and args[0] == "prepare":
        parser.add_argument("--examples", required=True, type=Path)
        parser.add_argument("--examples-manifest-sha256", required=True)
        parser.add_argument("--base-python", required=True, type=Path)
        parser.add_argument("--base-python-sha256", required=True)
        parser.add_argument("--log-dir", required=True, type=Path)
        parser.add_argument("--output", required=True, type=Path)
        parser.add_argument("--groups-per-cell", default=12, type=int)
        parser.add_argument("--wall-seconds", default=900, type=int)
        return prepare(parser.parse_args(args[1:]))
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--plan-sha256", required=True)
    parsed = parser.parse_args(args)
    return run(parsed.plan, parsed.plan_sha256)


if __name__ == "__main__":
    main()
