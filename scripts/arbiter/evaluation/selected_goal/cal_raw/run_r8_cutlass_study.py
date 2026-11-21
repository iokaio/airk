# SPDX-License-Identifier: Apache-2.0
"""Materialize the verified R8 input and execute the matched private Kernel study in one owned Job."""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import run_cutlass_study as study
from r8_cutlass_study_inputs import PLAN_SHA, pinned, prepare, read, require


def execute(descriptor, out):
    """Run the R5 launcher's image check, owned Job and closure on the R8 evidence builder."""
    study.prepare = prepare
    study.execute(descriptor, out)
    closure = json.loads((out / "execution.json").read_text("utf-8"))
    require(closure["complete"] is True and closure["report"]["plan"] == PLAN_SHA,
            "native study did not bind the R8 plan")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--descriptor", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    execute(read(pinned({"path": str(args.descriptor), "sha256": args.sha256}, "descriptor")), args.output)
