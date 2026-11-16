# SPDX-License-Identifier: Apache-2.0
"""Audit research freeze/source metadata without reading task text or running models.

This development-only metadata format is not a native request/profile/calibration
schema. Passing it cannot authorize training or replace human/native acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path

ORIGINALS = frozenset({"a0-freeze.json", "encoder-baseline-freeze.json",
    "goal-baseline-selection-freeze.json", "resource-evaluation-freeze.json"})
PARTITIONS = frozenset({"TRAIN", "DEV", "CAL", "policy_confirmation", "FINAL"})
REQUIRED_BINDINGS = frozenset({"corrected_matched_runtime_and_native_policy",
    "current_baseline_and_native_input_menu_census",
    "accepted_software_corpus_and_exact_rights_split_visible_examples",
    "repository_cluster_power_sample_rationale", "per_experiment_checkpoint_and_input_hashes",
    "measured_executable_probe_caps"})


def need(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(value):
    need(isinstance(value, str) and len(value) == 64 and
        all(c in "0123456789abcdef" for c in value), "exact lowercase SHA-256 required")
    return value


def validate_source_metadata(metadata):
    """Check identity/rights/split exclusions before any consumer may decode text.

    Research records carry opaque source hashes/family IDs, not native source
    locations, requests, prompts, token vectors or label values. A record is one
    original source closure. FINAL closure and clone families are never TRAIN.
    """
    need(metadata.get("schema") == "airk.development-software-source-metadata/1",
         "unsupported research source metadata")
    rows = metadata.get("sources")
    need(isinstance(rows, list) and rows, "nonempty source metadata required")
    reserved = {digest(value) for value in metadata.get("reserved_final_source_sha256", [])}
    need(reserved, "explicit nonempty FINAL source reservation required")
    owners, hashes, annotation_ids = {}, {}, set()
    counts = {partition: 0 for partition in PARTITIONS}
    for row in rows:
        partition = row.get("partition")
        need(partition in PARTITIONS, "unknown source partition")
        counts[partition] += 1
        source_hash = digest(row.get("source_sha256"))
        need(bool(row.get("repository_family")) and bool(row.get("revision")),
             "exact repository family/revision required")
        for field in ("repository_family", "issue_symbol_family", "clone_family"):
            family = row.get(field)
            need(isinstance(family, str) and family, "complete split family metadata required")
            key = (field, family)
            need(owners.setdefault(key, partition) == partition,
                 "repository/issue/clone family crosses partitions")
        need(hashes.setdefault(source_hash, partition) == partition,
             "source content crosses partitions")
        need(not (partition == "TRAIN" and source_hash in reserved),
             "reserved FINAL source cannot enter training or mining")
        need(not (partition == "TRAIN" and row.get("dataset") == "HotpotQA"),
             "Hotpot remains evaluation-only")
        rights = row.get("rights", {})
        digest(rights.get("decision_sha256"))
        digest(rights.get("license_artifact_sha256"))
        need(isinstance(rights.get("notice_artifact_sha256"), list)
             and rights["notice_artifact_sha256"], "retained notice/license closure required")
        for item in rights["notice_artifact_sha256"]:
            digest(item)
        need(partition in rights.get("permitted_partitions", []),
             "source rights do not cover selected use")
        need(row.get("final_exposure") in {"unexposed", "historically_exposed"},
             "explicit exposure disposition required")
        need(not (partition == "FINAL" and row["final_exposure"] != "unexposed"),
             "consumed FINAL is not untouched")
        if partition == "FINAL":
            need(source_hash in reserved, "FINAL source is absent from reservation")
        review = row.get("annotation_review", {})
        need(review.get("authority") == "existing_human_judgment",
             "agent/mechanical judgments are not accepted human review")
        review_hash = digest(review.get("artifact_sha256"))
        identifier = review.get("id")
        need(isinstance(identifier, str) and identifier and
             (identifier, review_hash) not in annotation_ids,
             "duplicate/missing annotation review identity")
        annotation_ids.add((identifier, review_hash))
    need(all(counts.values()), "all five partitions require source metadata")
    return counts


def audit(protocol, protocol_directory, metadata=None):
    need(protocol.get("schema") == "airk.development-software-v2-freeze/1",
         "unsupported additive protocol")
    bindings = protocol.get("original_protocols", [])
    need({item.get("path") for item in bindings} == ORIGINALS
         and len(bindings) == len(ORIGINALS), "exact original protocol roster required")
    for item in bindings:
        path = protocol_directory / item["path"]
        need(hashlib.sha256(path.read_bytes()).hexdigest() == digest(item.get("sha256")),
             "historical protocol changed")
    result = {"historical_protocols_verified": len(bindings),
              "source_metadata_audited": False, "training_authorized": False}
    if metadata is not None:
        result["partition_source_counts"] = validate_source_metadata(metadata)
        result["source_metadata_audited"] = True
    if protocol.get("executable") is True:
        need(not protocol.get("pending_bindings"), "executable protocol still has pending bindings")
        need(metadata is not None, "executable protocol requires accepted source metadata")
        accepted = protocol.get("accepted_bindings", {})
        need(REQUIRED_BINDINGS <= accepted.keys(), "executable protocol lacks accepted data/native/probe bindings")
        for key in REQUIRED_BINDINGS:
            digest(accepted[key])
        # This auditor never releases training: exact native visibility, human
        # annotation adequacy and execution caps require their owning review.
        result["status"] = "metadata_consistent_owner_acceptance_still_required"
    else:
        need(protocol.get("executable") is False, "explicit execution disposition required")
        result["status"] = "criteria_only_inputs_not_admitted"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, default=Path(__file__).with_name("software-v2-freeze.json"))
    parser.add_argument("--source-metadata", type=Path)
    args = parser.parse_args()
    try:
        protocol = json.loads(args.protocol.read_bytes())
        metadata = json.loads(args.source_metadata.read_bytes()) if args.source_metadata else None
        print(json.dumps(audit(protocol, args.protocol.parent, metadata), sort_keys=True))
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(1, "research audit refused: " + str(error) + "\n")


if __name__ == "__main__":
    main()
