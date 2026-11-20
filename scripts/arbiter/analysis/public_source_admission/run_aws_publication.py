# SPDX-License-Identifier: Apache-2.0
"""Run a bounded AWS offline source publication, then reopen its settled index for a census."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
SETUP_SECONDS = 4 * 3600
CENSUS_SECONDS = 1200


def window(phase, end, now):
    if end.tzinfo is None or end.utcoffset() is None:
        raise ValueError("An absolute timezone-aware campaign cutoff is required")
    remaining = int((end - now).total_seconds())
    allowed = SETUP_SECONDS if phase == "setup" else CENSUS_SECONDS
    if remaining < 240:
        raise ValueError("Insufficient campaign window for source, command and clean shutdown")
    return min(remaining, allowed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--phase", required=True, choices=("setup", "census"))
    parser.add_argument("--not-after", required=True, type=datetime.fromisoformat)
    parser.add_argument("--checkpoint", type=Path)
    parser.add_argument("--checkpoint-sha256")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if (args.phase == "census") != (args.checkpoint is not None and args.checkpoint_sha256 is not None):
        parser.error("Only census requires the settled setup checkpoint path and SHA-256")
    if args.phase == "setup" and (args.checkpoint or args.checkpoint_sha256):
        parser.error("A fresh source registration has no prior checkpoint")
    menu = runpy.run_path(str(HERE / "verify_aws_natural_menu.py"))
    stage = runpy.run_path(str(HERE / "aws_publication_setup.py"))
    n = menu["native"]
    n["checked"](n["WRAPPER"], n["WRAPPER_SHA"])
    n["checked"](n["ROOT"] / "scripts/provision-static-onnx-runtime.py", n["OWNER_SHA"])
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    w["require"](output == args.output and output.parent == n["CAMPAIGN"]
                 and output.name.startswith(f"aws-publication-{args.phase}-"), "Expected fresh private phase path")
    duration = window(args.phase, args.not_after, datetime.now(timezone.utc))
    if args.worker:
        w["require"](os.environ.get("AIRK_AWS_PUBLICATION_OWNED") == str(output), "Owned worker required")
        # Parent's one wall deadline includes interpreter startup and full source authentication.
        outer_expiry = float(os.environ["AIRK_AWS_PUBLICATION_DEADLINE"])
        deadline = min(time.monotonic() + duration, outer_expiry)
        with w["capped_worker_job"](output):
            stage["execute"](w, owner, menu, args, output, deadline)
        return
    w["require"](not output.exists(), "Fresh private phase output required")
    output.mkdir()
    source_paths = [Path(__file__), HERE / "aws_publication_setup.py", HERE / "verify_aws_natural_menu.py",
                    HERE / "native_cohort_source_reads.py", n["WRAPPER"],
                    n["ROOT"] / "scripts/provision-static-onnx-runtime.py"]
    sources = {path.name: owner.digest(path) for path in source_paths}
    plan_path = n["CAMPAIGN"] / "fp32-paired-answer-r1/cpu-launch-manifest-bound.json"
    n["checked"](plan_path, n["PLAN"])
    plan = w["read_json"](plan_path)
    receipt = {"phase": args.phase, "source_receipt": menu["INVENTORY_SHA"],
               "source_proof": menu["PACK_SHA"], "rights_proof": menu["RIGHTS_SHA"], "sources": sources,
               "native_pins": {k: plan[k]["sha256"] for k in ("service", "cli", "inputs")},
               "outside_job_seconds": duration, "command_seconds": 120, "campaign_not_after": args.not_after.isoformat(),
               "maximum_process_commit_bytes": 32 << 30, "maximum_tree_commit_bytes": 32 << 30,
               "setup_source_root": "aws-docs-full-r1", "model_scoring": False}
    w["write_json"](output / "launch.private.json", receipt)
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"],
                                          n["ROOT"], output / "outer.log", duration,
                                          os.environ | {"AIRK_AWS_PUBLICATION_OWNED": str(output),
                                                        "AIRK_AWS_PUBLICATION_DEADLINE": str(time.monotonic() + duration)})
    w["write_json"](output / "owned.json", result)
    stage["require_settlement"](w, result)
    w["require"](sources == {p.name: owner.digest(p) for p in source_paths}, "Offline sources changed during phase")
    memory = w["read_json"](output / "worker-memory.json")
    w["require"](memory["process_limit_bytes"] == memory["job_limit_bytes"] == 32 << 30
                 and memory["peak_job_bytes"] <= 32 << 30, "Offline memory cap proof differs")
    receipt.update(data=stage["link"](output / "phase.private.json"), owned=stage["link"](output / "owned.json"),
                   memory=stage["link"](output / "worker-memory.json"))
    checkpoint = output / "checkpoint.private.json"
    w["write_json"](checkpoint, receipt)
    print(stage["link"](checkpoint)["sha256"])


if __name__ == "__main__":
    main()
