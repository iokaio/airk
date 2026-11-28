# SPDX-License-Identifier: Apache-2.0
"""Verify the live proposal from frozen deterministic candidates, scoring inputs and complete frames."""
import argparse
import hashlib
import json
from pathlib import Path
from unittest.mock import patch

import common
import finalist_envelope_replay as replay
import finalist_spare_context as spare
import range_choice_live as live
from agent_cost_batch import working_set
from arbiter_pair_probe import PREFIX, SUFFIX, pair_text


def verify(campaign, paired, baseline):
    inventory = {r["id"]: r for r in working_set(campaign)}
    raw = common.read_jsonl(paired)
    expected = common.read_json(baseline)["tasks"]
    rows = {}
    for item in raw[1:]:
        key = item["task"]
        repo = replay.repository(campaign, item, inventory)
        pool = item["pool"]
        control = item["variants"]["control"]
        def frozen_gather(repository, arguments, trace, *, top_files):
            assert repository is repo and top_files == 40 and arguments == item["arguments"]
            trace.update(ranked=[(p["file"], 0) for p in pool],
                         all_locations=[{"at": at} for p in pool for at in p["locations"]])
        def verify_pairs(instruction, pairs):
            inputs = [hashlib.sha256((PREFIX + pair_text(instruction,q,d) + SUFFIX).encode()).hexdigest()
                      for q,d in pairs]
            if inputs != [r["rendered_sha256"] for r in control["scores"]]:
                raise ValueError("Model card identity differs")
            return [r["score"] for r in control["scores"]]
        with patch.object(live.live_gather, "gather", frozen_gather):
            passages, count, audit = live.propose(repo, item["arguments"], verify_pairs)
        if [p.at for p in passages] != [p["at"] for p in control["passages"]]:
            raise ValueError("Selected source ranges changed")
        index, metrics, _ = replay.archived_identity(campaign,inventory[key]["repository"])
        wire, _ = spare.render(item["arguments"],repo,index,metrics,passages,count,1,"line",
                               attribution=audit["attribution"])
        sha = common.sha256_bytes(wire)
        if sha != expected[key]["outcomes"]["14000/default/bundled/spare16"]["wire_sha256"]:
            raise ValueError("Live wire bytes differ from frozen 14k/default incumbent")
        rows[key] = {"pair_count":len(pool),"wire_sha256":sha}
    if set(rows) != set(inventory) or len(rows) != 20:
        raise ValueError("Incomplete development panel")
    return {"policy":"bundled40-spare16","heldout_read":False,"paired_sha256":common.sha256_file(paired),
            "baseline_sha256":common.sha256_file(baseline),"tasks":rows,
            "source_sha256":{name:common.sha256_file(Path(__file__).with_name(name)) for name in (
                "range_choice_live.py","live_gather.py","qwen_file_trial.py","bundled_range_extension.py",
                "adaptive_passage_budget.py","finalist_gather_contract.py","finalist_spare_context.py")}}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for name in ("campaign","paired","baseline","output"):
        p.add_argument(name,type=Path)
    a=p.parse_args()
    receipt=verify(a.campaign,a.paired,a.baseline)
    with a.output.open("x",encoding="utf-8") as f:
        json.dump(receipt,f,indent=2)
