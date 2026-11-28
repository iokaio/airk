# SPDX-License-Identifier: Apache-2.0
"""Predeclared range80 plus spare challenger on the exact prior transfer file/card pool."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
from time import perf_counter

import arbiter_judge_service
import common
from finalist_joint_evidence import Evidence
import finalist_spare_context
import live_gather
from prospective_transfer import digest, load_fixture
import qwen_file_trial
from qwen_need_trial import DIRECT
import range_choice

SELECTOR = "d9e442c0fdbde89683eb880e8d5f0974caaed10c620679c4f32fec9bedbded24"


def reconstruct(repo, args, baseline):
    trace = {}
    live_gather.gather(repo, args, trace, top_files=qwen_file_trial.WIDTH)
    paths = [p for p, _ in trace["ranked"][:qwen_file_trial.WIDTH]]
    locations = defaultdict(list)
    for r in trace["all_locations"]:
        locations[r["at"].rsplit(":",1)[0]].append(r["at"])
    query = "\n".join([args["task"], *args["needs"], *args["anchors"]])
    pairs = [(query,qwen_file_trial.document(repo,p,locations[p])) for p in paths]
    expected = baseline["variants"]["bundled"]["batches"]
    if (len(expected) != 1 or len(pairs) != expected[0]["pairs"] or
            digest(common.canonical_json([qwen_file_trial.INSTRUCTION,pairs])) != expected[0]["input_sha256"]):
        raise ValueError("The file/range input differs from the paired actor")
    return [{"file":p,"locations":locations[p],"source_sha256":repo.digests[p].hex()} for p in paths]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("panel","baseline","server","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("--freeze-sha256",required=True)
    parser.add_argument("--baseline-sha256",required=True)
    args=parser.parse_args()
    if digest(Path(range_choice.__file__).read_bytes())!=SELECTOR or digest(args.baseline.read_bytes())!=args.baseline_sha256:
        raise ValueError("Frozen input or selection code changed")
    repo,cases,manifest=load_fixture(args.panel,args.freeze_sha256)
    header,*prior=[json.loads(r) for r in args.baseline.read_bytes().splitlines()]
    if [c["id"] for c in cases] != [r["id"] for r in prior] or digest(args.server.read_bytes())!=header["server_sha256"]:
        raise ValueError("Paired cases or scorer identity changed")
    score=arbiter_judge_service.verified_client(args.server)
    identity=manifest["source_manifest_sha256"]
    with args.output.open("x",encoding="utf-8") as output:
        def emit(row):
            print(json.dumps(row,ensure_ascii=True),file=output,flush=True)
        emit({"type":"run","freeze_sha256":args.freeze_sha256,"baseline_sha256":args.baseline_sha256,"selector_sha256":SELECTOR,
              "runner_sha256":digest(Path(__file__).read_bytes()),"server_sha256":header["server_sha256"]})
        for case,original in zip(cases,prior,strict=True):
            request=case["arguments"]; pool=reconstruct(repo,request,original)
            nominations=range_choice.nominate(repo,pool,request,80)
            pairs=[(n.query,n.document) for n in nominations]
            if not 0<len(pairs)<=80:
                raise ValueError("Unexpected candidate count")
            start=perf_counter()
            scores=score(DIRECT,pairs)
            elapsed=perf_counter()-start
            selected=range_choice.assemble(request["needs"],nominations,scores)
            supports=defaultdict(list)
            for s in selected["scored_support"]:
                supports[s["returned_at"]].append({"need":s["need"],"scored_at":s["scored_at"]})
            seeds=[Evidence(x["at"],x["text"],tuple(supports[x["at"]])) for x in selected["passages"]]
            wire,kept=finalist_spare_context.render(request,repo,"offline-transfer",
                {"publication":"offline-fixture-sha256:"+identity,"scope_fingerprint":identity},
                seeds,len({n.source.at for n in nominations}),1,"line",passage_limit=14000,attribution="need_ranges")
            emit({"type":"case","id":case["id"],"request_sha256":original["request_sha256"],"logical_judgments":len(pairs),
                  "score_seconds":elapsed,"wire":wire.decode(),"wire_bytes":len(wire),"wire_sha256":digest(wire),
                  "source_bytes":sum(len(p.text.encode()) for p in kept),"pool_input_sha256":original["variants"]["bundled"]["batches"][0]["input_sha256"],
                  "inputs_sha256":digest(common.canonical_json([DIRECT,pairs])),"scores_sha256":digest(common.canonical_json(scores))})


if __name__=="__main__":
    main()
