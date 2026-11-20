# SPDX-License-Identifier: Apache-2.0
"""Reconcile publisher commit-link metadata against earlier screens without reading patches or labels."""
import argparse
from collections import Counter
from hashlib import sha256
import json
from pathlib import Path
import re
import sys
from urllib.parse import urlsplit

import pyarrow.parquet as pq

sys.path.insert(0, str(Path(__file__).resolve().parent))
from audit_repository_premerge_topology import PINS


TOPOLOGY = "69dd8e32d8d2a6a034f1db870617eb52942abfbab19daf8095dc951f9144ad65"
SCHEMA = "7ae992b7385245c4871ecd7c7cfd38f3526e0f84754f0669ce80c3f81b41c512"
COLUMNS = ("instance_id", "repo", "base_commit", "commit_url", "commit_urls")


def receipt(directory, name, digest):
    raw = (directory / name).read_bytes()
    if sha256(raw).hexdigest() != digest:
        raise ValueError("changed original metadata: " + name)
    return json.loads(raw)


def commit_id(url, repo, locator):
    if not isinstance(url, str):
        return None
    parsed = urlsplit(url)
    parts = parsed.path.strip("/").split("/")
    if parsed.scheme != "https" or parsed.netloc.lower() != "github.com" or parsed.query or parsed.fragment:
        return None
    if len(parts) != 4 or "/".join(parts[:2]).casefold() != repo.casefold() or parts[2] != locator:
        return None
    return parts[3] if re.fullmatch("[0-9a-f]{40}", parts[3]) else None


def census(directory):
    initial, first, second = [receipt(directory, name, pin) for name, pin in PINS.items()]
    original = [r for r in initial["rows"] if r["base_object_resolved"] and r["postmodel_issue_and_ordered_merge"]
                and r["base_tied_to_pull_ref_or_first_merge_parent"] and not r["known_contextbench_overlap"]
                and r["license_at_base"].get("permissive_classifier") is True]
    passing = [r for r in first["rows"]+second["rows"] if not r["screen_failures"]]
    roster = {r["instance_id"]: (r, "selected" if i<50 else "reserve")
              for i,r in enumerate(original+passing)}
    if len(original)!=39 or len(passing)!=20 or len(roster)!=59:
        raise ValueError("original selection changed")
    topology = receipt(directory,"premerge-topology-r1.private.json",TOPOLOGY)
    merges = {r["instance_id"]:r["observation"]["pull"]["mergeCommit"]["oid"] for r in topology["rows"]}
    shards = receipt(directory,"source-label-schema-r1.private.json",SCHEMA)["multilang"]["shards"]
    found = {}
    counts = {"selected":Counter(), "reserve":Counter()}
    for shard in shards:
        path = directory / (shard["split"]+"-00000-of-00001.parquet")
        with path.open("rb") as handle:
            if path.stat().st_size!=shard["bytes"] or sha256(handle.read()).hexdigest()!=shard["sha256"]:
                raise ValueError("changed publisher shard")
            handle.seek(0)
            reader = pq.ParquetFile(handle, pre_buffer=False)
            ids = reader.read(columns=["instance_id"],use_threads=False)["instance_id"].to_pylist()
            indices = [i for i,key in enumerate(ids) if key in roster]
            if not indices:
                continue
            table = reader.read(columns=list(COLUMNS),use_threads=False).take(indices)
            for row in table.to_pylist():
                key = row["instance_id"]
                prior, cohort = roster[key]
                if key in found or row["base_commit"]!=prior["base_commit"] or row["repo"].casefold()!=prior["requested_repo"].casefold():
                    raise ValueError("duplicate or changed preselected publisher row")
                primary = commit_id(row["commit_url"], row["repo"], "tree")
                listed = [commit_id(v, row["repo"], "commit") for v in row["commit_urls"] or []]
                case = {"cohort":cohort,"base_no_later_than_issue":prior["base_date_no_later_than_original_issue"],
                        "primary_url_same_repo_tree":primary is not None, "listed_urls":len(listed),
                        "all_listed_commits_resolved":bool(listed) and all(listed),
                        "listed_same_repo_commits":sum(x is not None for x in listed),
                        "primary_oid":primary, "primary_is_base":primary==row["base_commit"] if primary else None,
                        "primary_in_list":primary in listed if primary else None,
                        "primary_is_observed_merge":primary==merges[key] if primary and key in merges else None,
                        "observed_merge_in_list":merges[key] in listed if key in merges else None}
                found[key]=case
                counts[cohort].update({k:int(v) for k,v in case.items() if type(v) is bool})
    if found.keys()!=roster.keys() or len(merges)!=35:
        raise ValueError("incomplete publisher join")
    return {"columns":COLUMNS,"rows":found,"counts":counts,"observation_only":True,
            "independent_evidence_judgments":0,"source_validation_receipt_sha256":SCHEMA}


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory",type=Path,required=True)
    print(json.dumps(census(parser.parse_args().directory),indent=2))
