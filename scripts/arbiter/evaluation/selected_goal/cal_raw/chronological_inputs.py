# SPDX-License-Identifier: Apache-2.0
"""Project closed CAL phases into original per-model scorer inputs, without loading a model."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from phase_reference import AUDIT, MODELS, PLAN, phase_rows, require


def tensor_digest(value):
    encoded = json.dumps(value, ensure_ascii=False, allow_nan=False,
                         sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def chronology(phases, model, ordinal=None, reader=phase_rows, *, batch=False):
    require(not (batch and ordinal is not None), "choose a batch or a single original ordinal")
    require(0 < len(phases) <= 13, "one through thirteen consecutive closed phases required")
    rows = []
    seen_ids = set()
    for phase_index, phase in enumerate(phases):
        observed = reader(phase, model=model, retain_budget=True)
        start = 8 * phase_index
        end = min(start + 8, 100)
        require([row["ordinal"] for row in observed] == list(range(start, end)),
                "missing, duplicated or reordered original phase")
        for row in observed:
            require(row["id"] not in seen_ids, "repeated CAL request")
            seen_ids.add(row["id"])
            rows.append(row)
    counts = Counter(row["status"] for row in rows)
    eligible = [row for row in rows if row["status"] == "eligible"]
    result = {"model": MODELS[model][0], "original_manifest": MODELS[model][1],
              "launch_audit_sha256": AUDIT, "phase_plan_sha256": PLAN, "phases": phases,
              "original_requests": len(rows), "statuses": dict(counts),
              "first_eligible_ordinal": eligible[0]["ordinal"] if eligible else None,
              "NN_invocations": 0}
    if batch:
        result["rows"] = rows
    if ordinal is not None:
        require(0 <= ordinal < len(rows), "ordinal outside closed chronological prefix")
        row = rows[ordinal]
        selected = dict(row)
        if row["status"] == "eligible":
            selected["compiled_sha256"] = tensor_digest(row["compiled"])
            selected["roster_sha256"] = tensor_digest(row["roster"])
        result["selected"] = selected
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", nargs=2, action="append", required=True, metavar=("PATH", "SHA256"))
    parser.add_argument("--model", choices=MODELS, required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--ordinal", type=int)
    mode.add_argument("--batch", action="store_true")
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode, "use Python -I -B -S")
    refs = [{"path": path, "sha256": digest} for path, digest in args.phase]
    print(json.dumps(chronology(refs, args.model, args.ordinal, batch=args.batch), allow_nan=False, separators=(",", ":")))
