# SPDX-License-Identifier: Apache-2.0
"""Authenticate the complete R8 Compact CUDA panel for the unregistered native Compact Read study."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE / "compact_matching"), str(HERE)]
from r7_inputs import CAMPAIGN, IDENTITY, read, reference, require
from r5_chronological_inputs import check_outputs
from r8_capture import AUDIT_SHA, PLAN_SHA
from r8_compact_panel import panel
import r8_reference_inputs

SCHEMA = "airk.private.cutlass-study-verified-input/1"
NAME = "verified-study-input.json"


def pinned(value, label):
    require(isinstance(value, dict) and set(value) == {"path", "sha256"},
            f"exact {label} path and SHA-256 required")
    path = Path(value["path"])
    require(path.is_absolute() and path.is_relative_to(CAMPAIGN), f"{label} is outside the campaign")
    return reference(path, value["sha256"])


def same_file(left, right):
    return (Path(left["path"]) == Path(right["path"]) and left["sha256"] == right["sha256"]
            and left["bytes"] == right["bytes"])


def prepare(descriptor):
    require(isinstance(descriptor, dict) and set(descriptor) == {"panel", "certificate", "provider_sha256"},
            "R8 study evidence is the retained panel, its certificate and six phase receipt digests")
    panel_pin = pinned(descriptor["panel"], "panel")
    # The panel owner reopens all 13 native phase receipts. Every row it returns was
    # `raw_scores_only` on the exact graph, weights and compiled R8 input.
    rebuilt = panel(descriptor["provider_sha256"])
    require(read(panel_pin) == rebuilt and rebuilt["artifact"] == IDENTITY
            and rebuilt["calibration_fitted"] is False,
            "retained R8 panel does not reproduce from its native phase receipts")
    projection = read(rebuilt["projection"])
    certificate_pin = projection["reference_preflight"]
    require(same_file(certificate_pin, pinned(descriptor["certificate"], "certificate")),
            "certificate is not the one bound to the R8 projection")
    certificate = read(certificate_pin)
    require(certificate["complete"] is True and certificate["qualified_profiles"] == []
            and certificate["lifetime_plan_sha256"] == projection["plan_sha256"] == PLAN_SHA
            and certificate["launch_audit_sha256"] == projection["launch_audit"]["sha256"] == AUDIT_SHA
            and certificate["focus_tokens"] == 1024 and len(certificate["rows"]) == 100,
            "wrong R8 certificate, plan, launch audit or Focus width")
    receipts = [rebuilt[key] for key in ("projection", "compiler", "graph")]
    compiled = []
    for first in range(0, 100, 8):
        # The R8 reference selector checks the terminal chain, the native compiler
        # receipt and each captured request against this certificate.
        chosen, _ = r8_reference_inputs.selected(*receipts, first, min(8, 100 - first))
        compiled.extend(chosen)
    rows = []
    for scored, row, original in zip(rebuilt["rows"], compiled, certificate["rows"], strict=True):
        roster = original["request"]["roster"]
        require(scored["ordinal"] == row["ordinal"] == len(rows)
                and scored["id"] == row["id"] == original["id"]
                and scored["input_digest"] == row["compiled"]["input_digest"]
                and row["roster"] == roster and len(scored["scores"]) == len(roster["cards"])
                and original["split"] == "calibration",
                "R8 score, compiled input, roster or certificate row differs")
        rows.append({"id": scored["id"], "ordinal": scored["ordinal"], "status": "raw_scores_only",
                     "input_digest": scored["input_digest"], "scores": scored["scores"],
                     "split": original["split"], "source_group": original["source_group"], "roster": roster})
    # Independent CPU reference comparisons stay with their own receipts; the study does not read them.
    return {"schema": SCHEMA, "graph": IDENTITY["model_sha256"], "weights": IDENTITY["weight_sha256"],
            "recipe": IDENTITY["matching_recipe_sha256"], "plan": PLAN_SHA, "preflight": certificate,
            "provenance": {"descriptor": descriptor, "panel": panel_pin, "certificate": certificate_pin,
                           "projection": rebuilt["projection"], "compiler": rebuilt["compiler"],
                           "graph": rebuilt["graph"], "phases": rebuilt["phases"]},
            "comparisons": None,
            "raw": {"complete": True, "task_qualified": False, "release_qualified": False, "rows": rows}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--descriptor", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    out = args.output
    require(out.is_absolute() and out.resolve() == out and out.parent == CAMPAIGN and out.is_dir()
            and not any(out.iterdir()), "empty checked private output directory required")
    packet = prepare(read(pinned({"path": str(args.descriptor), "sha256": args.sha256}, "descriptor")))
    check_outputs(out, [NAME], created=False)
    with (out / NAME).open("x", encoding="utf-8") as stream:
        json.dump(packet, stream, allow_nan=False, separators=(",", ":"))
    check_outputs(out, [NAME], created=True)
    print(hashlib.sha256((out / NAME).read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
