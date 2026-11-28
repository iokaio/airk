# SPDX-License-Identifier: Apache-2.0
"""Compare emitted mutable-run citations with both authenticated source images at identical coordinates."""
import argparse
import hashlib
import json
from pathlib import Path

import common
from single_stage_live_probe import MARKER, SOURCE


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    evidence = common.read_json(args.run / "receipt.json")
    source = (args.baseline / SOURCE).read_bytes()
    original_hash = hashlib.sha256(source).hexdigest()
    changed = source.replace(b"def _retryable(", MARKER + b"def _retryable(", 1)
    changed_hash = hashlib.sha256(changed).hexdigest()
    common.require((original_hash, changed_hash) == (evidence["baseline_sha256"], evidence["changed_sha256"]),
                   "Cannot authenticate both source states")
    rows = []
    for i, label in enumerate(("baseline", "edited", "restored"), 3):
        actual, other = (changed, source) if label == "edited" else (source, changed)
        payload = (args.run / "gather" / f"call-{i:03}.wire").read_bytes()
        common.require(hashlib.sha256(payload).hexdigest() == evidence["phases"][label]["wire_sha256"], "Changed wire")
        body = json.loads(json.loads(payload)["result"]["content"][0]["text"])
        common.require(body["eligibility"]["publication"] == evidence["stable"][label]["read"], "Publication differs")
        hits = []
        for entry in body["passages"]:
            path, _, extent = entry["at"].rpartition(":")
            if path != SOURCE:
                continue
            first, last = map(int, extent.split("-"))
            expected = "\n".join(actual.decode().splitlines()[first-1:last]) + "\n"
            stale = "\n".join(other.decode().splitlines()[first-1:last]) + "\n"
            hits.append({"at": entry["at"], "matches_actual": entry["text"] == expected,
                         "differs_from_other_at_identical_coordinates": entry["text"] != stale,
                         "digest_matches_actual": entry["workspace_sha256"] == hashlib.sha256(actual).hexdigest() and
                         entry["text_sha256"] == hashlib.sha256(expected.encode()).hexdigest()})
        common.require(hits and all(x["matches_actual"] and x["digest_matches_actual"] for x in hits),
                       "Emitted citation differs from actual changed/restored source")
        rows.append({"phase": label, "hits": hits})
    distinguishes = all(any(h["differs_from_other_at_identical_coordinates"] for h in row["hits"]) for row in rows)
    with args.output.open("x", encoding="utf-8") as out:
        json.dump({"rows": rows, "distinguishes_all_three": distinguishes, "edited_marker_shown":
                   evidence["phases"]["edited"]["marker_seen"]}, out, indent=2)
    print([(r["phase"], len(r["hits"]), sum(h["differs_from_other_at_identical_coordinates"] for h in r["hits"])) for r in rows])
    return int(not distinguishes)


if __name__ == "__main__":
    raise SystemExit(main())