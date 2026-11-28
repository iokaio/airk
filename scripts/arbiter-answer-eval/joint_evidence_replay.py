# SPDX-License-Identifier: Apache-2.0
"""Compare matched bundled and corroborated need evidence on complete native-shaped MCP frames."""
import argparse
import json
from pathlib import Path

import bundled_range_extension as extension
import common
import finalist_envelope_replay as env
import finalist_gather_contract as contract
import finalist_joint_evidence as joint
import finalist_spare_context as spare
from agent_cost_batch import working_set


def joint_render(arguments, repository, index, metrics, seed, base_count, all_at, records, request_id, framing, *, strict):
    def fit(rows, total, mode):
        return contract.render(arguments,repository,index,metrics,rows,total,request_id,framing,attribution=mode)
    base_wire, core = fit(seed, base_count, "bundled_file")
    widened_wire, widened = spare.widen(arguments,repository,index,metrics,base_count,request_id,framing,
                                        base_wire,core,passage_limit=14000,attribution="bundled_file")
    union_count = len(set(all_at) | {r["returned_at"] for r in records})
    start = widened if strict else core
    result = joint.attach(repository, start, records, len(arguments.get("needs") or [arguments["task"]]),
                          lambda rows: fit(rows, union_count, "need_ranges"),
                          passage_limit=14000,preserve_file_score=False)
    if result is None or not result[2]:
        return widened_wire, widened, [], "bundled_file"
    wire, kept, accepted = result
    if not strict:
        wire, kept = spare.widen(arguments,repository,index,metrics,union_count,request_id,framing,
                                 wire,kept,passage_limit=14000,attribution="need_ranges")
    if len(kept) < len(start) or any(not joint.contains(p.at, old.at) for p,old in zip(kept,start)):
        raise ValueError("Protected baseline range displaced")
    return wire, kept, accepted, "need_ranges"


def run(campaign, paired_path, range_path, baseline_path, output, *, native_default=False):
    study = "finalist-range-default-r1" if native_default else "finalist-range-envelope-r1"
    expected_freeze = "5342af801a03a6089bf4303b2fe450e5b7b59f6770cf1bc4554bc027a18a100d" if native_default else env.FREEZE
    freeze = campaign/"replay"/study/"freeze.json"
    if common.sha256_file(freeze) != expected_freeze:
        raise ValueError("Bundled envelope identity changed")
    identities = common.read_json(freeze)
    if identities["bundled"] != common.sha256_file(paired_path) or identities["range"] != common.sha256_file(range_path):
        raise ValueError("Unmatched original score receipts")
    base = common.read_json(baseline_path)
    if (base["freeze_sha256"] != expected_freeze or base["contract_sha256"] != common.sha256_file(Path(contract.__file__))
            or base["spare_sha256"] != common.sha256_file(Path(spare.__file__))):
        raise ValueError("Wire renderer or widening behavior changed")
    caps = ("default",) if native_default else (8192, 14000, 20000)
    inventory = {r["id"]:r for r in working_set(campaign)}
    paired = {r["task"]:r for r in common.read_jsonl(paired_path) if r.get("type") == "task"}
    head,*raw = common.read_jsonl(range_path)
    ranges = {r["task"]:r for r in raw}
    if not inventory.keys() == paired.keys() == ranges.keys() == base["tasks"].keys():
        raise ValueError("Incomplete study input")
    if head["paired_sha256"] != identities["bundled"]:
        raise ValueError("Unmatched input to range candidate scoring")
    with output.open("x", encoding="utf-8") as fp:
        def emit(row):
            print(json.dumps(row,ensure_ascii=False,separators=(",",":")),file=fp,flush=True)
        emit({"type":"run", "paired_sha256":identities["bundled"], "ranges_sha256":identities["range"],
              "baseline_sha256":common.sha256_file(baseline_path),"caps":caps,
              "logical_pairs":80,"executed_new_pairs":0,"attribution_threshold":"binary yes preferred only",
              "spare_sha256":base["spare_sha256"],"contract_sha256":base["contract_sha256"],
              "selector_sha256":common.sha256_file(Path(joint.__file__)),"tasks":sorted(inventory)})
        for key in sorted(inventory):
            item, scored = paired[key], ranges[key]["variants"]["range40"]["scores"]
            if len(scored)!=40:
                raise ValueError("Need 40 matched subsidiary scores")
            repo = env.repository(campaign,item,inventory)
            index,metrics,_ = env.archived_identity(campaign,inventory[key]["repository"])
            stream = extension.bundled(item,Path(item["source"]))
            seeds = [env.Candidate(p.at,p.text,[]) for p in extension.select(stream,14000)]
            outcomes = {}
            for cap in caps:
                args = env.frame_args(item["arguments"],cap)
                wire,kept = spare.render(args,repo,index,metrics,seeds,len(stream),1,"line",
                                         passage_limit=14000,attribution="bundled_file")
                truth = base["tasks"][key]["outcomes"][f"14000/{cap}/bundled/spare16"]
                if common.sha256_bytes(wire) != truth["wire_sha256"]:
                    raise ValueError("Could not reproduce bundled incumbent frame")
                for label,strict in (("strict",True),("reserve",False)):
                    blob,delivered,support,mode = joint_render(args,repo,index,metrics,seeds,len(stream),
                                                               [p.at for p in stream],scored,1,"line",strict=strict)
                    if len(blob) > contract.reply_cap(args):
                        raise ValueError("Actual wire cap exceeded")
                    outcomes[f"{cap}/{label}"] = {"wire_utf8":blob.decode(), "wire_sha256":common.sha256_bytes(blob),
                        "baseline_sha256":common.sha256_bytes(wire),"wire_bytes":len(blob),
                        "attribution":mode,"accepted":support}
                outcomes[f"{cap}/baseline"] = {"wire_utf8":wire.decode(),"wire_sha256":common.sha256_bytes(wire),
                                               "wire_bytes":len(wire),"attribution":"bundled_file","accepted":[]}
            emit({"type":"task","task":key,"index":index,**metrics,"source":item["source"],"outcomes":outcomes})


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for arg in ("campaign","paired","range_scores","wire_baseline","output"):
        p.add_argument(arg,type=Path)
    p.add_argument("--native-default", action="store_true", help="Replay the original no-override request against its matching baseline file.")
    opts=p.parse_args()
    run(opts.campaign,opts.paired,opts.range_scores,opts.wire_baseline,opts.output,native_default=opts.native_default)
