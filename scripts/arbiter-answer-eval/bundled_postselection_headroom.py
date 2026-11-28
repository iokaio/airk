# SPDX-License-Identifier: Apache-2.0
"""Postselection development sensitivity. Keep the frozen reply and try skipped whole source candidates at 20k."""
import argparse
from collections import Counter
import json
from pathlib import Path

import finalist_envelope_replay as replay
import finalist_gather_contract as contract
from agent_cost_batch import working_set
import prompt_need_audit as audit


PINNED = {"replay/qwen-paired-cards-r1/paired-complete.jsonl":"d3121a27a1f5981ddb185119829e04884b7512f3b9bbe3b5725aa8f935628180",
          "replay/finalist-range-default-r1/outcomes.json":"2e12f94f2fb07f514c92ded1b43697a3e47f970aba951bc7fabf669136081246",
          "replay/finalist-range-envelope-r1/outcomes.json":"458ba10dc56782a6c2882c54391c07dfd9817f4ef3c36f22b949e6be615d9104",
          "replay/qwen-need-reference-r2.json":"d3b787bb0c01a1914008a64fad7bd0ed119ec38f51fb3b5e0b47249452d6a699"}


def compare(campaign):
    paths = [campaign/name for name in PINNED]
    if any(audit.digest(p.read_bytes())!=sha for p,sha in zip(paths,PINNED.values(),strict=True)):
        raise ValueError("Development input or selected baseline changed")
    _,*items = map(json.loads,paths[0].read_bytes().splitlines())
    large,small,refs = [audit.read_json(p)["tasks"] for p in paths[1:]]
    inventory={r["id"]:r for r in working_set(campaign)}
    if sorted(r["task"] for r in items)!=sorted(inventory) or len(items)!=20:
        raise ValueError("Wrong development panel")
    results={}
    for item in items:
        key=item["task"]; meta=inventory[key]; source=Path(item["source"])
        repo=replay.repository(campaign,item,inventory)
        index,metrics,proof=replay.archived_identity(campaign,meta["repository"])
        pool=replay.extension.bundled(item,source)
        task=audit.read_json(meta["task"])
        if audit.digest(meta["task"].read_bytes())!=refs[key]["scenario_sha256"]:
            raise ValueError("Development reference identity differs")
        notes={r["id"]:r for r in task["notes"]}
        results[key]={}
        for limit in ("default",8192,14000,20000):
            request=replay.frame_args(item["arguments"],limit)
            def frame(candidate):
                return contract.render(request,repo,index,metrics,candidate,len(pool),1,"line",attribution="bundled_file")
            recorded=(large if limit=="default" else small)[key]["outcomes"][f"14000/{limit}/bundled/spare16"]
            selected=[]; spent=0; covered=set();files=set()
            for row in recorded["passages"]:
                path,a,b=audit.parse_location(row["at"])
                text="\n".join(repo.texts[path][a-1:b])+"\n"
                if audit.digest(text.encode())!=row["sha256"] or row["needs"]:
                    raise ValueError("Selected source passage or attribution changed")
                selected.append(replay.Candidate(row["at"],text,[])); spent+=len(text.encode());files.add(path)
                covered.update((path,n) for n in range(a,b+1))
            base,kept=frame(selected)
            if kept!=selected or audit.digest(base)!=recorded["wire_sha256"]:
                raise ValueError("Selected baseline wire mismatch")
            blob=base; reasons=Counter(); added=[]
            for passage in pool:
                coords={(passage.path,n) for n in range(passage.first,passage.last+1)}
                if coords<=covered:
                    reasons["already_delivered"]+=1
                    continue
                if passage.path not in files and len(files)==6:
                    reasons["file_limit"]+=1
                    continue
                if spent+passage.size>20000:
                    reasons["extra_passage_limit"]+=1
                    continue
                candidate=replay.Candidate(passage.at,passage.text,[])
                proposed=[*selected,candidate]
                new,kept=frame(proposed)
                if kept!=proposed:
                    reasons["wire_limit"]+=1
                    continue
                blob,selected=new,proposed
                spent+=passage.size; covered.update(coords);files.add(passage.path);added.append(passage.at)
            body=json.loads(json.loads(blob)["result"]["content"][0]["text"])
            count=len(recorded["passages"])
            original=json.loads(json.loads(base)["result"]["content"][0]["text"])
            if body["passages"][:count]!=original["passages"] or body["needs"]!=original["needs"]:
                raise ValueError("Changed selected source rows or need status")
            judged=replay.analyze(body,blob,base,source,notes,refs[key]["need_notes"],20000,0)
            change={}
            for field in ("whole_notes","whole_needs","whole_essential","attributed_whole_needs"):
                before,after=set(recorded[field]),set(judged[field])
                if not before<=after:
                    raise ValueError("Displaced original evidence")
                change[field]={"before":len(before),"after":len(after),"gained":sorted(after-before)}
            results[key][str(limit)]={"change":change,"added_ranges":added,"omissions":dict(reasons),"set":meta["set"],
                "base_wire_bytes":len(base),"final_wire_bytes":len(blob),"base_passage_bytes":recorded["passage_bytes"],
                "final_passage_bytes":spent,"base_hash":audit.digest(base),"final_hash":audit.digest(blob),"scope_proof":proof}
    return {"basis":"Postselection development-only sensitivity. Frozen 14k bundled+spare remains the selected policy.",
            "inputs":PINNED,"script_sha256":audit.digest(Path(__file__).read_bytes()),"tasks":results}


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("campaign",type=Path);p.add_argument("output",type=Path)
    args=p.parse_args()
    result=compare(args.campaign)
    with args.output.open("x",encoding="utf-8") as fp:
        json.dump(result,fp,separators=(",",":"));fp.write("\n")
