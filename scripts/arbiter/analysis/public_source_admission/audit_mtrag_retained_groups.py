# SPDX-License-Identifier: Apache-2.0
"""Make private source-disjoint groups from the two frozen IBM metadata receipts."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys
from urllib.parse import urlsplit

sys.path.insert(0,str(Path(__file__).resolve().parent))
from mtrag_cloud_original_sources import META, REV, SCREEN, original_receipts, require


def components(keys):
    """Connect distinct cases via any shared original source or conversation."""
    groups = [{i} for i in range(len(keys))]
    for i, row in enumerate(keys):
        for j in range(i):
            if row & keys[j] and groups[i] is not groups[j]:
                merged = groups[i] | groups[j]
                for k in merged:
                    groups[k] = merged
    return sorted(set(tuple(sorted(group)) for group in groups))


def audit():
    screen, meta = original_receipts()
    originals = screen["screening_candidates"]
    bound = {row["task_id"]:row for row in meta["cases"]}
    bindings = meta["published_passage_bindings"]
    ids = [row["task_id"] for row in originals]
    require(len(ids)==len(set(ids))==len(bound)==25 and set(bound)==set(ids),"incomplete source roster")
    keys, roster, doc_ids, all_passages = [], [], set(), set()
    for ordinal, frozen in enumerate(originals):
        row = bound[frozen["task_id"]]
        pointers = frozen["publisher_positive_passage_pointers"]
        require(len(pointers)==len(set(pointers)) and pointers and set(pointers)==set(row["published_positive_passage_pointers"])
                and row["release"]==frozen["release"] and row["conversation_id"]==frozen["conversation_id"]
                and hashlib.sha256("\n".join(sorted(pointers)).encode("ascii")).hexdigest()==frozen["positive_passage_roster_sha256"],
                "foreign case or passage grouping")
        keys_for_case = {("conversation",row["conversation_id"]),("question_token",frozen["question_json_token_sha256"])}
        docs = set()
        for pointer in pointers:
            binding = bindings[pointer]
            doc = binding["document_id"]
            require(doc == pointer.rsplit("-",2)[0],"publisher document join differs")
            url = urlsplit(binding["url"])
            require(url.scheme in ("http","https") and url.hostname and not url.username,"invalid source reference")
            keys_for_case.update({("document",doc),("passage",pointer),("url",url.netloc.lower(),url.path or "/",url.query)})
            docs.add(doc)
        doc_ids.update(docs)
        all_passages.update(pointers)
        keys.append(keys_for_case)
        roster.append({"ordinal":ordinal,"task":row["task_id"],"release":row["release"],"conversation":row["conversation_id"],
                       "source_documents":sorted(docs),"positive_passages":pointers,
                       "authored":row["human_question_created_at"]})
    require(len(all_passages)==meta["bound_passages"]==60 and len(doc_ids)==meta["distinct_literal_cloud_documents"]==49,
            "positive passage or source document coverage changed")
    groups = components(keys)
    return {"schema":"airk.private.ibm-cloud-retained-source-groups/1","revision":REV,"screen_sha256":SCREEN,
            "binding_sha256":META,"source":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "source_reference_files":meta["publisher_reference_pins"],"case_count":len(ids),
            "positive_pointer_occurrences":sum(len(row["positive_passages"]) for row in roster),
            "distinct_positive_passages":len(all_passages),"distinct_source_documents":len(doc_ids),
            "cases":roster,"components":groups,"component_size_counts":dict(Counter(map(len,groups))),
            "rights":meta["scope"]["repository_license_scope"],"complete_human_support_sets_verified":0,
            "verified_native_ranges":0,"independently_admissible_for_paired_measurement":0,
            "evidence_limit":"Passage-level positive relevance only. Native byte/line locations, support-set sufficiency, aliases, and model exposure require independent evidence."}


if __name__ == "__main__":
    print(json.dumps(audit(),separators=(",",":"),allow_nan=False))
