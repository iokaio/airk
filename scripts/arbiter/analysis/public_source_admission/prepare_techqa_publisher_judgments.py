# SPDX-License-Identifier: Apache-2.0
"""Join frozen extractive judgments to native source visibility without opening QA text or model output."""
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[4]
E = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
FULL = E / "techqa-normal-source-full-r1"
NOTICE = ROOT / ("artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1/"
                 "techqa-original-publisher-archive-r2/notices/3fc2eed6aa47f088844ef5fc446f5bf2acda5f2d193dc82f48d2873790c4128d.bytes")
LICENSE = ROOT / "third_party/research/TechQA-CDLA-Permissive-1.0.pdf"
PINS = {
    "freeze": ("techqa-original-dev-ordered-menu-freeze-r1.json", "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7"),
    "menus": ("techqa-full-menu-corpus-r1/manifest.private.json", "34109e91b1aa8acd13af56fc706ef3f73006502e3fdffed23843d13b263d1821"),
    "gold": ("techqa-exact-gold-source-corpus-r1/manifest.private.json", "a670e02fe6abdaab184a6950c33d74caf4ef4b972530a1aff300d68dd51b44c8"),
    "range": ("techqa-native-source-reads-r3/native-reads.private.json", "ffc6dfad39b3c809deb6c0434969df9afb19b08ab61228319106602be0a1eb73"),
    "groups": ("techqa-development-menu-overlap-r1.private.json", "7d05ef85f41a3038425611b0f96b9eb073c2330010fd63f5a3af698eaf8837c5"),
    "exposure": ("techqa-menu-train-exposure-r1.json", "2de707182f15be6e486a0e0fea004a52575e232a1db4caa4f3ad7d9ef992657c"),
    "native": ("techqa-normal-source-full-r1/census.private.json", "130d3becc8bdf1ae4bcf866bc349561aede90d2504bdfc3f370b6e8547e36ff9"),
    "owned": ("techqa-normal-source-full-r1/owned.json", "1e2db4b1ea6ba97e76cd05ae8e19c019fc03c09130d610582d90dc5c848c1fc2"),
}
README_SHA = "c5a58ed7f2780fb2c4f2e06f8933ec4afbd06b5f29f6797c7857f3812f824060"
LICENSE_SHA = "e12775d6b5bfdba0bb33fd691dab386f0c7573853fdc4785c40b39739f120d5f"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked(path, sha):
    require(path.resolve(strict=True) == path and all(not p.is_junction() and not p.is_symlink()
            for p in (path, *path.parents)), "noncanonical metadata path")
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == sha, "metadata identity differs: " + path.name)
    return raw


def bind_case(selector, annotated, docs, normal, gold, native_read):
    require(normal["case_id"] == selector == gold["selector"]["question_id"] == native_read["selector"]["question_id"],
            "publisher and native case identity differ")
    members = normal["ordered_members"]
    require(len(members) == len(set(annotated["ordered_document_ids"])) == 50 and
            [m["publisher_id"] for m in members] == annotated["ordered_document_ids"], "publisher menu order differs")
    for i, m in enumerate(members):
        d = docs[m["publisher_id"]]
        require(m["publisher_position_zero_based"] == i and m["path"] == m["publisher_id"] + ".txt" and m["visible"]
                and m["original_sha256"] == m["native_sha256"] == d["sha256"]
                and m["original_bytes"] == m["native_bytes"] == d["bytes"], "native menu member content differs")
    document = annotated["publisher_document_id"]
    member, = (m for m in members if m["publisher_id"] == document)
    span, citation = gold["source_span"], native_read["citation"]
    require(annotated["source_status"] == "exact_publisher_span" and document == gold["publisher_document_id"]
            and member["path"] == gold["document"]["path"] == citation["path"]
            and member["native_sha256"] == gold["document"]["sha256"] == citation["content_hash"]
            and gold["document"]["bytes"] == member["native_bytes"]
            and 0 <= annotated["publisher_start"] < annotated["publisher_end"]
            and 0 <= annotated["byte_start"] == span["byte_start"] < annotated["byte_end_exclusive"]
            == span["byte_end_exclusive"] <= member["native_bytes"]
            and span["span_sha256"] == annotated["span_sha256"] == native_read["span_sha256"]
            and span["line_start"] == citation["start_line"] and span["line_end_inclusive"] == citation["end_line"],
            "publisher span, native range read or normal-menu source differs")
    return {"publisher_id": document, "original_path": member["path"], "source_sha256": member["native_sha256"],
            "publisher_scalar_start": annotated["publisher_start"], "publisher_scalar_end_exclusive": annotated["publisher_end"],
            **span, "native_range_text_sha256": native_read["native_text_sha256"]}


def prepare():
    data = {k: json.loads(checked(E / path, sha)) for k, (path, sha) in PINS.items()}
    readme = checked(NOTICE, README_SHA)
    checked(LICENSE, LICENSE_SHA)
    require(b"human annotators found an answer" in readme and b"START_OFFSET" in readme and b"END_OFFSET" in readme,
            "original publisher extractive judgment semantics differ")
    f, menu, gold, ranges = (data[k] for k in ("freeze", "menus", "gold", "range"))
    grouping, native, owner = (data[k] for k in ("groups", "native", "owned"))
    groups = grouping["remaining_components"]
    membership = {case: members for members in groups for case in members}
    require(grouping["direct_train_matches"] == [] and grouping["exposed_source_components"] == [["DEV_Q002"]]
            and len(groups) == 32 and sum(map(len, groups)) == len(membership) == 75
            and "DEV_Q002" not in membership and grouping["prior_exposure_sha256"] == PINS["exposure"][1]
            and grouping["full_menu_sha256"] == PINS["menus"][1], "original withholding or component fence differs")
    require(all(x["ordered_menu_freeze_sha256"] == PINS["freeze"][1] for x in (menu, gold, ranges))
            and ranges["corpus_manifest_sha256"] == PINS["gold"][1] and not ranges["answer_scored"]
            and native["publisher_manifest_sha256"] == PINS["menus"][1] and native["menu_freeze_sha256"] == PINS["freeze"][1]
            and native["answer_requests"] == native["ranker_requests"] == 0 and owner["exit_code"] == 0
            and owner["owned_descendants_settled"] and owner["owned_job_final_active_processes"] == 0
            and owner["owned_job_final_member_pids"] == [] and not owner["owned_job_forced_termination"]
            and not owner["timed_out"], "native source epochs or process settlement differ")
    a = f["menu_freeze"]
    gold_cases = {r["selector"]["question_id"]: r for r in gold["cases"]}
    native_ranges = {r["selector"]["question_id"]: r for r in ranges["cases"]}
    require(len(a["cases"]) == 77 and len(gold_cases) == 76 and set(gold_cases) == set(native_ranges)
            == set(menu["ordered_menus"]) == set(membership) | {"DEV_Q002"}, "exact publisher case population differs")
    withheld = [case for case, v in a["cases"].items() if v["source_status"] == "withheld_publisher_span_mismatch"]
    require(len(withheld) == 1 and withheld[0] not in membership and len(native["cases"]) == native["count"] == 75
            and {row["case_id"] for row in native["cases"]} == membership.keys(), "normal source case population differs")
    rows = []
    for record in native["cases"]:
        case = record["case_id"]
        obs = json.loads(checked(FULL / "observations" / (case + ".json"), record["receipt_sha256"]))
        require(record["index_id"] == obs["index_id"] and record["publication"] == obs["publication"]["token"]
                and record["visible"] == obs["visible"] == 50 and record["omitted"] == obs["omitted"] == 0
                and a["cases"][case]["ordered_document_ids"] == menu["ordered_menus"][case], "scope or source coverage differs")
        accepted = bind_case(case, a["cases"][case], menu["documents"], obs, gold_cases[case], native_ranges[case])
        exposure = data["exposure"]["cases"][case]
        rows.append({"case": case, "shared_document_component": membership[case], "train_product_metadata": exposure,
                     "source_family_independence": "unresolved", "original_menu": menu["ordered_menus"][case],
                     "normal_index": obs["index_id"], "normal_publication": obs["publication"],
                     "normal_scope": obs["scope"], "normal_observation_sha256": record["receipt_sha256"],
                     "publisher_acceptable_evidence_sets": [[accepted]]})
    counts = {field: sum(bool(row["train_product_metadata"]["menu_matches"].get(field)) for row in rows)
              for field in ("literal_product_id", "literal_product_name")}
    clean_proxies = [row["case"] for row in rows if not row["train_product_metadata"]["menu_missing_fields"]
                     and not any(row["train_product_metadata"]["menu_matches"].get(key) for key in counts)]
    return {"role": "prospective original TechQA extractive publisher judgments", "inputs": PINS,
            "documented_positive_sets": len(rows), "literal_product_overlap_counts": counts,
            "no_literal_product_overlap_with_complete_metadata": clean_proxies,
            "prior_source_fence": f["source_fence"], "prior_mirror_exposure": f["prior_exposure_caveat"],
            "public_model_pretraining_exposure": "unknown", "frozen_train_menu_sha256": grouping["train_source_sha256"],
            "publisher_qa_member": "TechQA/training_and_dev/dev_Q_A.json",
            "publisher_qa_member_sha256": f["members"]["TechQA/training_and_dev/dev_Q_A.json"],
            "publisher_archive_sha256": f["archive_sha256"], "rights": {"terms": "CDLA-Permissive-1.0",
            "license_path": str(LICENSE), "license_sha256": LICENSE_SHA, "publisher_readme_sha256": README_SHA},
            "gold_range_read_publication": ranges["publication"], "source_identity_join": "same bytes, separate native publications",
            "withheld_offset_mismatch": withheld, "excluded_exposed_singleton": "DEV_Q002",
            "source_components": groups, "known_direct_train_document_matches": [], "rows": rows,
            "new_judgments_needed_for_documented_positive_sets": 0, "unannotated_alternatives": "unknown",
            "final_reservation": None, "paired_outcomes": 0, "8192_token_stratum": None}


if __name__ == "__main__":
    print(json.dumps(prepare(), sort_keys=True, allow_nan=False))
