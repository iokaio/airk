# SPDX-License-Identifier: Apache-2.0
"""Exact and simulated coverage on synthetic boundary nulls; reads only two pinned metadata files."""
import argparse
from hashlib import sha256
import json
from pathlib import Path
from random import Random
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from paired_confidence import half_width


def metadata(path, expected):
    raw = path.read_bytes()
    if sha256(raw).hexdigest() != expected:
        raise ValueError("original source metadata changed")
    return json.loads(raw)


def designs(root):
    group = metadata(root/"active-confirmation-source-groups-r1.private.json",
                     "2468b79029dd4fab82d92f5960503824841b1c9fa83f874f426151f10e909c92")["source_group_views"]
    tech = group["techqa_ordered_menu_content_sha256"]
    if [g for g in tech if "DEV_Q002" in g] != [["DEV_Q002"]]:
        raise ValueError("exposed singleton changed")
    common = [len(g) for g in tech if "DEV_Q002" not in g]
    common += [len(g) for g in group["ibm_published_positive_document_ids"]]
    common += [len(g) for g in group["contextbench_all_repository"]]
    # The original census includes 50 single-repository MultiLang requests lacking
    # complete independently judged evidence. These are planning scenarios only.
    common += [1] * 50
    out = {view: common + [len(g) for g in group[key]] for view,key in
           (("AWS guide","aws_guide_family"), ("AWS document","aws_answer_document_sha256"))}
    out["AWS collection"] = common + [41]
    out["six equal groups"] = [1]*6
    if [(len(v),sum(v)) for v in out.values()] != [(123,205),(141,205),(115,205),(6,6)]:
        raise ValueError("source group arithmetic changed")
    return out


def width_metadata(root):
    census = metadata(root/"compact-development-focus1024-prefix12-census-r1.private.json",
                      "a30258f4d43fa8d4822a0c56c93c9726448f7eec3ed4bdb72148ba436120bfe7")
    widths = []
    for row in census["rows"]:
        for record in row.get("admissions", []):
            members = record["compiler_census"]
            keys = {(c["input_tokens"],c["compiled_input_digest"]) for c in members}
            if len(members) != 2 or len(keys) != 1 or not all(c["fits_remaining_allowance"] for c in members):
                raise ValueError("compiler agreement/fit not observed")
            widths.append(members[0]["input_tokens"])
    margins = metadata(root/"compact-train-focus1024-reference-first13-margins-r1.private.json",
                       "851fcc274cfd37e347d7c61e4e1e00079f6ae1af29c380d9884c1c5d5bdb3863")
    if margins["prefix_sha256"] != "a30258f4d43fa8d4822a0c56c93c9726448f7eec3ed4bdb72148ba436120bfe7":
        raise ValueError("width metadata from another census")
    return {"states": census["counts"], "paired_task_outcomes": margins["paired_task_outcomes"],
            "admissions": len(widths), "minimum": min(widths),
            "maximum": max(widths), "at_8192": widths.count(8192), "quality_measured": census["task_quality_measured"]}


def distribution(sizes, probability, shared):
    n = sum(sizes)
    if shared:
        return {0: 1-probability, n: probability}
    mass = {0:1.0}
    for weight in sizes:
        nxt = {}
        for total, chance in mass.items():
            nxt[total] = nxt.get(total,0) + chance*(1-probability)
            nxt[total+weight] = nxt.get(total+weight,0) + chance*probability
        mass = nxt
    return mass


def coverage(sizes, low, high, probability, shared, repeats, rng):
    n = sum(sizes)
    truth = low + (high-low)*probability
    radius_cluster = half_width(sizes)
    radius_request = half_width([1]*n)
    def covered(weight, radius):
        mean = low + (high-low)*weight/n
        return max(-0.98, mean-radius) <= truth+1e-12
    mass = distribution(sizes, probability, shared)
    exact = sum(p for k,p in mass.items() if covered(k,radius_cluster))
    naive = sum(p for k,p in mass.items() if covered(k,radius_request))
    simulated = 0
    for _ in range(repeats):
        k = n*int(rng.random()<probability) if shared else sum(s for s in sizes if rng.random()<probability)
        simulated += covered(k,radius_cluster)
    return {"exact_cluster_coverage":exact,"simulated_cluster_coverage":simulated/repeats,
            "exact_request_independence_coverage":naive,
            "cluster_half_width":radius_cluster}


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--metadata",required=True,type=Path)
    ap.add_argument("--simulations",type=int,default=10000)
    args=ap.parse_args()
    if args.simulations < 1:
        ap.error("positive simulation count required")
    rng=Random(25793)
    table={}
    for name,sizes in designs(args.metadata).items():
        table[name]={"requests":sum(sizes),"groups":len(sizes),"largest_group":max(sizes),"scenarios":{}}
        for label,lo,hi,p,shared in (
            ("rare_regressions",-.98,.02,.98,False),
            ("balanced_discordance",-.98,1,49/99,False),
            ("all_joint_failure",0.,0.,0.,False),
            ("cross_group_common_shock",-.98,1,49/99,True)):
            table[name]["scenarios"][label]=coverage(sizes,lo,hi,p,shared,args.simulations,rng)
        table[name]["all_joint_success_at_rare_regression_boundary"]=.98**len(sizes)
    print(json.dumps({"seed":25793,"simulations_per_scenario":args.simulations,"hypothetical_only":True,
                      "exposed_train_widths":width_metadata(args.metadata),"designs":table},indent=2))


if __name__=="__main__":
    main()
