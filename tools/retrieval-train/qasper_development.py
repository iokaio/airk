# SPDX-License-Identifier: Apache-2.0
"""Materialize one explicitly approved development-only QASPER rights view.

Original human records are preserved separately as private archival evidence.
The model-consumable view excludes quote-bearing passages and figures/tables;
unresolved/disagreeing alternatives are not converted into invented gold.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ARCHIVE_BYTES = 10835856
ARCHIVE_SHA256 = "a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def prepare(paper, decision):
    observed = digest(json.dumps(paper["full_text"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode())
    if observed != decision["source_text_sha256"] or paper["title"] != decision["title"]:
        raise ValueError("approved source work/hash differs")
    allowed, excluded, locations = [], [], {}
    for section_index, section in enumerate(paper["full_text"]):
        for paragraph_index, text in enumerate(section["paragraphs"]):
            identifier = f"{decision['paper_id']}:{section_index}:{paragraph_index}"
            item = {"id": identifier, "section": section["section_name"], "paragraph_sha256": digest(text.encode())}
            # Conservative, explicit derived-view exclusion. Do not relabel
            # excluded third-party quotations as the paper authors' own work.
            if any(character in text for character in '\"“”«»„'):
                excluded.append({**item, "reason": "quote-bearing paragraph; third-party permissions not separately established"})
            else:
                allowed.append({**item, "text": text})
                locations.setdefault(text, []).append(identifier)
    questions = []
    for question in paper["qas"]:
        alternatives = []
        for annotation in question["answers"]:
            answer = annotation["answer"]
            evidence = [{"source_evidence_sha256": digest(text.encode()), "allowed_locations": locations.get(text, [])}
                        for text in answer["evidence"]]
            alternatives.append({"human_annotation_id": annotation["annotation_id"],
                "original_record_sha256": digest(json.dumps(annotation, sort_keys=True, ensure_ascii=False).encode()),
                "unanswerable": answer["unanswerable"], "original_evidence_count": len(answer["evidence"]),
                "evidence": evidence,
                "alignment": "unanswerable_human_alternative" if answer["unanswerable"] else
                    "all_original_evidence_maps_to_allowed_text" if evidence and all(row["allowed_locations"] for row in evidence)
                    else "unsupported_or_excluded_evidence_not_admitted"})
        questions.append({"question_id": question["question_id"], "question": question["question"],
                          "human_alternatives": alternatives,
                          "disagreement": len({bool(row["unanswerable"]) for row in alternatives}) > 1})
    if sum(len(row["human_alternatives"]) for row in questions) != decision.get("expected_human_alternatives", 5):
        raise ValueError("approved human alternatives were not preserved")
    return {"schema": "airk.development-qasper-rights-view/1", "decision": decision,
            "publisher_split": decision["publisher_split"], "passages": allowed, "excluded_passages": excluded,
            "questions": questions, "nontext_evidence": "All figures/tables excluded; no visual extraction qualification.",
            "changes": ["QASPER/S2ORC extraction retained under the approved exact source hash",
                        "Paragraph identity/hash added; quote-bearing paragraphs excluded from consumable view",
                        "Original human alternatives preserved separately; evidence alignment is mechanical, not new adjudication"],
            "scope": "Development-only diagnostic. No training, final gold, sufficiency, statistical quality or native transport qualification."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-root", type=Path, required=True)
    parser.add_argument("--arxiv-metadata", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / "artifacts") or output.exists():
        parser.error("output must be a fresh private artifact directory")
    decisions_path = HERE / "development-adoptions.json"
    source_bytes = Path(__file__).read_bytes()
    decisions_bytes = decisions_path.read_bytes()
    decision = json.loads(decisions_bytes)["decisions"][0]
    archive = args.raw_root / "qasper-train-dev-v0.3.tgz"
    raw = archive.read_bytes()
    if len(raw) != ARCHIVE_BYTES or digest(raw) != ARCHIVE_SHA256:
        raise ValueError("QASPER archive differs")
    metadata_bytes = args.arxiv_metadata.read_bytes()
    if digest(metadata_bytes) != decision["source_metadata_sha256"]:
        raise ValueError("primary source-rights metadata differs")
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as source:
        member = source.getmember("qasper-dev-v0.3.json")
        if not member.isfile() or member.size > 33554432:
            raise ValueError("unexpected approved member")
        member_bytes = source.extractfile(member).read(member.size + 1)
        if len(member_bytes) != member.size:
            raise ValueError("member size changed")
        paper = json.loads(member_bytes)[decision["paper_id"]]
    result = prepare(paper, decision)
    for path, retained in ((Path(__file__), source_bytes), (decisions_path, decisions_bytes),
                           (archive, raw), (args.arxiv_metadata, metadata_bytes)):
        if path.read_bytes() != retained:
            raise ValueError("development corpus input changed during materialization: " + str(path))
    output.mkdir(parents=True)
    original = json.dumps(paper["qas"], ensure_ascii=False, indent=2) + "\n"
    (output / "original-human-annotations.private.json").write_text(original, encoding="utf-8")
    result["original_human_records_sha256"] = digest(original.encode())
    result["input_seals"] = {"archive_sha256": ARCHIVE_SHA256, "metadata_sha256": digest(metadata_bytes),
        "rights_decisions_sha256": digest(decisions_bytes), "source_sha256": digest(source_bytes)}
    (output / "materializer-source.py").write_bytes(source_bytes)
    (output / "development-view.private.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({"questions": len(result["questions"]), "human_alternatives": sum(len(q["human_alternatives"]) for q in result["questions"]),
        "allowed_passages": len(result["passages"]), "excluded_passages": len(result["excluded_passages"]),
        "disagreement_questions": sum(q["disagreement"] for q in result["questions"])}))


if __name__ == "__main__":
    main()
