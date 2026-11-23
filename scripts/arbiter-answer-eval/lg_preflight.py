# SPDX-License-Identifier: Apache-2.0
"""Preflight of the LG gate run: every input the gate depends on, checked in one command.

Run it last, after the final rebase, the gate image build, the prompts refreeze and the lifetimes file, and
again before any resume. It starts nothing and changes nothing. Any mismatch is a refusal (exit 1): fix the
cause, never the expectation. The gate's CUDA lifetimes spend each root's one CUDA lifetime, so a run that
starts on a wrong input cannot simply be repeated.

``python -I -B lg_preflight.py --inputs <gate-inputs.json> --out <receipt.json>``

The inputs file (private; ``R\\answer-loop\\L5C\\lg-inputs\\gate-inputs.json``) states what the run expects::

    {"series": "06-gate-lg", "r_root": "<R>", "commit": "<the harness tree's HEAD the image was built from>",
     "image": "<image folder with images.json>", "scenario_freeze_sha256": "...",
     "prompts_freeze": "<R>/design/prompts-freeze.json", "lifetimes": "<lifetimes file>",
     "pairs": {"C": {"path": ..., "sha256": ...}, "L": {...}, "F": {...}},
     "manifests": {"context-compact-fp16-cuda": "...", "context-large-fp16-cuda": "...", "focus-1024-cuda": "..."},
     "bases": {"C": "06-gate-lg-C-base", "L": "06-gate-lg-L-base2", "F": "06-gate-lg-F-base"},
     "roots": {}, "spares": {"C32": "06-gate-lg-C32-base", "L32": "06-gate-lg-L32-base"},
     "disk_floor_bytes": 214748364800, "disk_need_bytes": 21474836480, "device_need_gib": 35, "host_need_gib": 64}

``roots`` maps a judged arm to the arm whose (spare) root it runs in, as ``lg_lifetimes.py --root`` does.

Checks: the harness tree is clean and at ``commit``; the image names that commit and its files have the
recorded SHA-256; the default budget read from the Rust-emitted cases file; the scenario freeze; the prompts
freeze verifies against this tree (harness files, scenario freeze, budget); each pair, its roster and the
manifests it names; each judged arm's root has a closed, restartable base of exactly the pair's indexes and
has had no CUDA lifetime; the lifetimes file names those roots, bases and pairs; free disk above the floor;
free device and host memory; the CUDA math environment the launcher gives each CUDA service
(``ORT_CUDA_GEMM_OPTIONS=2``, no conflicting inherited ``ORT_*``); the plan runs the live smoke test
(``gate.py smoke``) on every arm before its replays and before any session; and the harness suite passes.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import lg_lifetimes  # noqa: E402

SCHEMA = "airk.private.answer-eval-lg-preflight/1"
JUDGED = ("C", "L", "F")


def git(*arguments: str) -> str:
    return subprocess.run(["git", "-C", str(common.REPO_ROOT), *arguments], capture_output=True, text=True,
                          check=True, stdin=subprocess.DEVNULL).stdout.strip()


def check_tree(inputs: dict) -> str:
    head, dirty = git("rev-parse", "HEAD"), git("status", "--porcelain")
    common.require(head == inputs["commit"], f"the harness tree is at {head}, not {inputs['commit']}")
    common.require(not dirty, f"the harness tree has uncommitted changes: {dirty.splitlines()[:5]}")
    return head


def check_image(inputs: dict) -> dict:
    image = Path(inputs["image"])
    images = common.read_json(image / "images.json")
    common.require(images.get("git_head") == inputs["commit"],
                   f"the image was built from {images.get('git_head')}, not {inputs['commit']}")
    found = {}
    for part in ("service", "cli"):
        row = images[part]["image"]
        common.require(Path(row["path"]).parent == image and common.sha256_file(row["path"]) == row["sha256"],
                       f"the image's {part} file differs from images.json")
        found[part] = row["sha256"]
    return found


def check_budget(inputs: dict) -> dict:
    source = common.answer_budget_source()
    expected = inputs.get("default_budget")
    common.require(expected is None or expected == source["default_budget"],
                   f"the cases file's default budget is {source['default_budget']}, not {expected}")
    return source


def check_scenarios(inputs: dict) -> str:
    path = Path(inputs["r_root"]) / "scenario-freeze.json"
    digest = common.sha256_file(path)
    common.require(digest == inputs["scenario_freeze_sha256"], f"{path} is {digest}, not the frozen scenarios")
    return digest


def check_prompts(inputs: dict) -> dict:
    import prompts_freeze
    return prompts_freeze.verify(Path(inputs["prompts_freeze"]))


def check_pairs(inputs: dict) -> dict:
    found = {}
    for arm in JUDGED:
        row = inputs["pairs"][arm]
        common.require(Path(row["path"]).is_file() and common.sha256_file(row["path"]) == row["sha256"],
                       f"pair of arm {arm} differs from its pin: {row['path']}")
        pair = common.read_json(Path(row["path"]))
        roster = pair["roster"]
        common.require(Path(roster["path"]).is_file() and common.sha256_file(roster["path"]) == roster["sha256"],
                       f"roster of arm {arm}'s pair differs from its pin")
        for name, profile in pair["profiles"].items():
            common.require(inputs["manifests"].get(name) == profile.get("manifest"),
                           f"arm {arm}: profile {name} has manifest {profile.get('manifest')}, "
                           f"not {inputs['manifests'].get(name)}")
        found[arm] = sorted(pair["profiles"])
    return found


def arm_root(inputs: dict, arm: str) -> Path:
    return Path(inputs["r_root"]) / f"fixture-{inputs['series']}-{(inputs.get('roots') or {}).get(arm, arm)}"


def check_roots(inputs: dict) -> dict:
    found, roots = {}, []
    for arm in JUDGED:
        root, base = arm_root(inputs, arm), inputs["bases"][arm]
        lg_lifetimes.check_base(root, base)
        used = lg_lifetimes.cuda_lifetimes(root)
        common.require(not used, f"arm {arm}: {root.name} has already spent its CUDA lifetime ({', '.join(used)})")
        roots.append(root)
        found[arm] = {"root": str(root), "base": base, "cuda_lifetimes": 0}
    common.require(len(set(roots)) == len(roots), "two judged arms name one root")
    spares = {}
    for arm, base in (inputs.get("spares") or {}).items():
        root = Path(inputs["r_root"]) / f"fixture-{inputs['series']}-{arm}"
        if root in roots:
            continue
        try:
            lg_lifetimes.check_base(root, base)
            spares[arm] = "free" if not lg_lifetimes.cuda_lifetimes(root) else "spent"
        except ValueError as error:
            spares[arm] = f"unusable: {error}"
    found["spares"] = spares
    return found


def check_lifetimes(inputs: dict) -> dict:
    rows = common.read_json(Path(inputs["lifetimes"]))
    for arm in JUDGED:
        row, pair = rows[arm], inputs["pairs"][arm]
        args = row["start_args"]
        common.require(Path(row["fixtures"]) == arm_root(inputs, arm), f"lifetimes row {arm} names {row['fixtures']}")
        common.require(row["lifetime"] == f"{inputs['series']}-{arm}", f"lifetimes row {arm} is named {row['lifetime']}")
        common.require(args[args.index("--image") + 1] == str(Path(inputs["image"])),
                       f"lifetimes row {arm} names another image")
        common.require(args[args.index("--pair-sha256") + 1] == pair["sha256"]
                       and Path(args[args.index("--pair") + 1]) == Path(pair["path"]),
                       f"lifetimes row {arm} names another pair")
        common.require(args[-2:] == ["--restart-from", inputs["bases"][arm]],
                       f"lifetimes row {arm} does not restart from {inputs['bases'][arm]}")
    return {"path": inputs["lifetimes"], "sha256": common.sha256_file(inputs["lifetimes"]), "arms": sorted(rows)}


def check_disk(inputs: dict) -> dict:
    free = shutil.disk_usage(inputs["r_root"]).free
    need = int(inputs["disk_floor_bytes"]) + int(inputs["disk_need_bytes"])
    common.require(free >= need, f"{free} bytes free on the campaign volume; the floor plus the run need {need}")
    return {"free_bytes": free, "floor_bytes": int(inputs["disk_floor_bytes"]), "need_bytes": int(inputs["disk_need_bytes"])}


def check_memory(inputs: dict) -> dict:
    import gate
    device = gate.nvidia_smi_memory()
    common.require(device is not None, "nvidia-smi gave no device memory")
    free_device = max(g["total_mib"] - g["used_mib"] for g in device.values()) / 1024
    common.require(free_device >= float(inputs["device_need_gib"]),
                   f"{free_device:.1f} GiB of device memory free; the three arms and the reserve need "
                   f"{inputs['device_need_gib']}")
    host = gate.host_available_bytes()
    common.require(host is None or host / (1 << 30) >= float(inputs["host_need_gib"]),
                   f"host memory free is below {inputs['host_need_gib']} GiB")
    return {"free_device_gib": round(free_device, 2), "free_host_gib": None if host is None else round(host / (1 << 30), 2)}


def check_math(_inputs: dict) -> dict:
    """The CUDA math the launcher will give each CUDA lifetime's service (GEMM2, no other ORT_* variable),
    refused here if this shell's environment conflicts. ``run_plan.py`` checks each handle again at readiness."""
    import gpu_fixture
    return {"service": gpu_fixture.math_environment(), "tolerated": gpu_fixture.MATH_TOLERATED}


def check_plan_smoke(inputs: dict) -> dict:
    """The plan runs the live smoke test (``gate.py smoke``: one Answer per arm and repository with a search
    record without a failure, a Focus call, and model decisions where a Context model is staged) after each
    launch and before any replay or session of those arms. The live test itself needs the lifetimes, so it is a
    plan step; this check makes sure the plan has it."""
    path = Path(inputs["r_root"]) / "passes" / inputs["series"] / "plan.json"
    plan = common.read_json(path)
    smoked: set[str] = set()
    for index, step in enumerate(plan):
        gates = [g["gate"] for g in step.get("parallel") or [] if "gate" in g] + (
            [step["gate"]] if "gate" in step else [])
        for body in gates:
            if body.get("command") == "smoke":
                smoked.add(body["arm"])
            elif body.get("command") == "replays":
                common.require(body["arm"] in smoked, f"plan step {index + 1} replays arm {body['arm']} before its smoke test")
        if "sessions" in step:
            common.require(set(JUDGED) <= smoked, f"plan step {index + 1} runs sessions before every judged arm's smoke test")
    common.require(set(JUDGED) <= smoked, f"the plan has no smoke test for arms {sorted(set(JUDGED) - smoked)}")
    return {"plan": str(path), "sha256": common.sha256_file(path), "smoked": sorted(smoked)}


def check_suite(_inputs: dict) -> dict:
    done = subprocess.run([sys.executable, "-I", "-B", "-m", "unittest", "discover", "-s", str(HERE)],
                          capture_output=True, text=True, stdin=subprocess.DEVNULL, cwd=str(common.REPO_ROOT))
    tail = (done.stderr or done.stdout).strip().splitlines()[-3:]
    common.require(done.returncode == 0, f"the harness suite failed: {tail}")
    return {"result": tail}


CHECKS = (("harness tree", check_tree), ("image", check_image), ("default budget", check_budget),
          ("scenario freeze", check_scenarios), ("prompts freeze", check_prompts), ("pairs", check_pairs),
          ("roots", check_roots), ("lifetimes file", check_lifetimes), ("disk", check_disk),
          ("memory", check_memory), ("CUDA math environment", check_math), ("plan smoke test", check_plan_smoke),
          ("harness suite", check_suite))


def run(inputs: dict, checks=CHECKS) -> dict:
    """Every check, each to its end: a refusal in one does not hide another."""
    rows = []
    for name, check in checks:
        try:
            rows.append({"check": name, "ok": True, "found": check(inputs)})
        except (common.HarnessError, ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
            rows.append({"check": name, "ok": False, "problem": f"{type(error).__name__}: {error}"})
    return {"schema": SCHEMA, "at": common.now_iso(), "series": inputs.get("series"),
            "passed": all(row["ok"] for row in rows), "checks": rows}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="the receipt (replaced on every run)")
    args = parser.parse_args(argv)
    receipt = run(common.read_json(args.inputs))
    receipt["inputs"] = {"path": str(args.inputs), "sha256": common.sha256_file(args.inputs)}
    common.write_json(args.out, receipt)
    for row in receipt["checks"]:
        print(("ok      " if row["ok"] else "REFUSED ") + row["check"] + ("" if row["ok"] else ": " + row["problem"]))
    print("PREFLIGHT PASSED" if receipt["passed"] else "PREFLIGHT REFUSED: do not start the gate")
    return 0 if receipt["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
