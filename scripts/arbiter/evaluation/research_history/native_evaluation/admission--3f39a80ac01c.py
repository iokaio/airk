# SPDX-License-Identifier: Apache-2.0
"""Metadata-only reservation; no raw corpus, questions, gold or models opened."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import threading
import time
import types

HERE = Path(__file__).resolve().parent
DOMAIN = b"airk.selected-context-generic-read.r1/component-split\0"
ALLOWED = {"EXPOSED", "DEV", "CAL_SEALED_UNOPENED", "FINAL_SEALED_UNOPENED"}

def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def need(value, reason):
    if not value:
        raise ValueError(reason)

def reserve(metadata):
    need(metadata["schema"] == "airk.private-a1-blind-metadata-audit/1", "metadata schema")
    questions, articles, components = set(), set(), set()
    output = {"CAL": [], "CONFIRMATION": [], "EXCLUDED": []}
    counts = {name: 0 for name in ALLOWED}
    for row in metadata["components"]:
        c, qs, aa = row["component_digest"], row["question_ids"], row["article_digests"]
        need(isinstance(c, str) and len(c) == 64 and c not in components, "component identity")
        bytes.fromhex(c)
        need(qs and aa and len(set(qs)) == len(qs) and len(set(aa)) == len(aa), "component membership")
        need(not questions.intersection(qs) and not articles.intersection(aa), "connected family overlap")
        need(row["disposition"] in ALLOWED, "unknown disposition")
        need(all(isinstance(q, str) and q for q in qs), "question identity")
        need(all(isinstance(a, str) and len(a) == 64 for a in aa), "article identity")
        for a in aa:
            bytes.fromhex(a)
        questions.update(qs); articles.update(aa); components.add(c)
        counts[row["disposition"]] += len(qs)
        if row["disposition"] != "CAL_SEALED_UNOPENED":
            destination = "EXCLUDED"
        else:
            need(row.get("exclusion") is None, "excluded CAL component")
            destination = "CAL" if hashlib.sha256(DOMAIN + bytes.fromhex(c)).digest()[0] & 1 == 0 else "CONFIRMATION"
        output[destination].append({"component_digest": c, "question_ids": qs, "article_digests": aa,
                                    "original_disposition": row["disposition"]})
    need(counts == {k:v["questions"] for k,v in metadata["counts_before_unperformed_rights_alignment_checks"].items()}, "population drift")
    for rows in output.values():
        rows.sort(key=lambda row: row["component_digest"])
    return output

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--protocol-sha256", required=True)
    parser.add_argument("--root-decision", type=Path, required=True)
    parser.add_argument("--execute-reviewed-metadata-only", action="store_true", required=True)
    args = parser.parse_args()
    started = time.monotonic()
    need(sha(args.protocol) == args.protocol_sha256, "protocol drift")
    protocol = json.loads(args.protocol.read_bytes())
    decision = json.loads(args.root_decision.read_bytes())
    need(decision["schema"] == "airk.private-selected-Context-cohort-admission-decision/1"
         and decision["approved"] is True and decision["protocol_sha256"] == args.protocol_sha256
         and decision["metadata_only"] is True and decision["cal_scope_explicitly_approved"] is True
         and decision["current_exposure_inventory_complete_for_declared_producers"] is True
         and decision["entire_old_FINAL_bucket_and_all_QASPER_excluded"] is True,
         "Root metadata scope/exposure decision missing")
    before = {row["path"]:sha(row["path"]) for row in protocol["inputs"].values()}
    need(all(before[row["path"]] == row["sha256"] for row in protocol["inputs"].values()), "sealed input drift")
    resource = protocol["inputs"]["resource_owner"]
    module = types.ModuleType("sealed_metadata_resource_owner"); module.__file__ = resource["path"]
    raw = Path(resource["path"]).read_bytes()
    need(hashlib.sha256(raw).hexdigest() == resource["sha256"], "resource source changed")
    exec(compile(raw, resource["path"], "exec"), module.__dict__)
    module.JOB_STATE = module.enforce_budget()
    done = threading.Event()
    def expire():
        if not done.wait(60):
            os._exit(124)
    threading.Thread(target=expire, daemon=True).start()
    adapter_pin = protocol["inputs"]["retained_adapter"]
    adapter = types.ModuleType("sealed_anchor_owner"); adapter.__file__ = adapter_pin["path"]
    adapter_raw = Path(adapter_pin["path"]).read_bytes()
    need(hashlib.sha256(adapter_raw).hexdigest() == adapter_pin["sha256"], "adapter drift")
    exec(compile(adapter_raw, adapter_pin["path"], "exec"), adapter.__dict__)
    observer_raw = Path(protocol["inputs"]["anchor_observer"]["path"]).read_bytes()
    anchor = Path(protocol["output_anchor"])
    target = anchor / "materialized"
    observe = lambda path: adapter.anchor_observation(path, observer_raw)
    proof, identity = adapter.guard_output(anchor, target, protocol["protected_input_roots"], observe)
    metadata = json.loads(Path(protocol["inputs"]["metadata"]["path"]).read_bytes())
    reservations = reserve(metadata)
    report = {"schema":"airk.private-selected-Context-opaque-reservation/1",
              "protocol_sha256":args.protocol_sha256, "root_decision_sha256":sha(args.root_decision),
              "rights_alignment_eligibility":"NOT_PERFORMED; candidates reserved only",
              "partitioning_before_any_new_question_or_gold_decode":True,
              "reservations":reservations,
              "counts":{k:{"questions":sum(len(x["question_ids"]) for x in v),
                            "documents":sum(len(x["article_digests"]) for x in v),
                            "components":len(v)} for k,v in reservations.items()},
              "all_QASPER_excluded":True,"entire_previous_FINAL_bucket_excluded":True,
              "inputs_unchanged":all(sha(p)==h for p,h in before.items()),
              "elapsed_seconds":time.monotonic()-started,"peak_committed_bytes":module.peak_memory(),
              "anchor_proof":proof,"native_or_neural_execution":False,"complete":False}
    need(report["inputs_unchanged"] and report["elapsed_seconds"] < 60 and report["peak_committed_bytes"] <= 268435456, "resource/drift refusal")
    for part in ("CAL", "CONFIRMATION"):
        need(report["counts"][part]["questions"] >= 100 and report["counts"][part]["documents"] >= 30, "metadata population insufficient")
    # Fixed fresh output; never overwrite/merge previous reservations.
    need((anchor.stat().st_dev, anchor.stat().st_ino) == identity, "anchor identity changed")
    need(observe(anchor) == proof, "anchor authority changed")
    target.mkdir()
    output = target / "opaque-reservation.json"
    report["complete"] = True
    with output.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, separators=(",", ":"));stream.write("\n");stream.flush();os.fsync(stream.fileno())
    done.set()
    print(json.dumps({"complete":True,"counts":report["counts"],"sha256":sha(output),
                      "rights_alignment_eligibility":report["rights_alignment_eligibility"]}))

if __name__ == "__main__":
    main()
