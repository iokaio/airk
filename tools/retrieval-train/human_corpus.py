# SPDX-License-Identifier: Apache-2.0
"""Materialize reviewed human-annotation fixtures; never synthesize gold labels."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile
from urllib.parse import quote

import qasper_development as qasper

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def hash_bytes(data):
    return hashlib.sha256(data).hexdigest()


def append_hotpot_rows(hotpot_rows, selected_set, documents, tasks, hotpot_exclusions):
    """Append exact neutral Hotpot conversion; caller owns sealed input selection."""
    for row in hotpot_rows:
        if row["id"] not in selected_set:
            continue
        mapping = {}
        for title, sentences in zip(row["context"]["title"], row["context"]["sentences"]):
            text = "".join(sentences)
            identifier = "hotpot:" + hash_bytes((title + "\n" + text).encode())
            if any(character in text for character in '"“”«»„'):
                hotpot_exclusions.append({"task_id": row["id"], "source_id": identifier, "reason": "quote-bearing context excluded"})
                continue
            documents[identifier] = {"id": identifier, "text": text, "title": title, "source_group": "wikipedia:" + title,
                "url": "https://en.wikipedia.org/wiki/" + quote(title.replace(" ", "_")), "paragraph_sha256": hash_bytes(text.encode()),
                "license": "CC-BY-SA-4.0", "dataset": "HotpotQA", "attribution": "Wikipedia contributors, " + title + "; HotpotQA (Yang, Qi, Zhang et al.,2018), CC BY-SA4.0; publisher normalized paragraph, no text modification"}
            for sentence_index, sentence in enumerate(sentences):
                mapping[(title, sentence_index)] = (identifier, sentence)
        evidence = []
        for title, sentence_index in zip(row["supporting_facts"]["title"], row["supporting_facts"]["sent_id"]):
            location = mapping.get((title, sentence_index))
            evidence.append({"source_title": title, "sentence_index": sentence_index,
                "source_evidence_sha256": hash_bytes(location[1].encode()) if location else None,
                "allowed_locations": [location[0]] if location else []})
        tasks.append({"id": "hotpot:" + row["id"], "question_id": row["id"], "question": row["question"], "dataset": "HotpotQA", "split": "test",
            "publisher_split": "validation", "source_group": "hotpot-question:" + row["id"], "disagreement": False,
            "human_alternatives": [{"human_annotation_id": "hotpot-original:" + row["id"], "unanswerable": False,
                "original_human_answer": row["answer"], "original_evidence_count": len(evidence), "evidence": evidence,
                "alignment": "all_original_evidence_maps_to_allowed_text" if evidence and all(e["allowed_locations"] for e in evidence) else "unsupported_or_excluded_evidence_not_admitted"}]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rights", type=Path, required=True)
    parser.add_argument("--raw-root", type=Path, required=True)
    parser.add_argument("--hotpot", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    destination = args.output.resolve()
    if destination.exists() or not destination.is_relative_to(ROOT / "artifacts"):
        parser.error("fresh private destination required")
    retained = {p: p.read_bytes() for p in (Path(__file__), Path(qasper.__file__), args.rights, HERE / "a0-freeze.json", args.hotpot)}
    rights = json.loads(retained[args.rights])
    assert hash_bytes(retained[args.hotpot]) == "c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6"
    original_papers = {}
    for filename, sha, members in [
        ("qasper-train-dev-v0.3.tgz", "a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a", ["qasper-train-v0.3.json", "qasper-dev-v0.3.json"]),
        ("qasper-test-and-evaluator-v0.3.tgz", "72a52a41193e2838b8074f80ac074b94f956b84886c36a61c58a7df4171bdd72", ["qasper-test-v0.3.json"])]:
        path = args.raw_root / filename
        retained[path] = path.read_bytes()
        assert hash_bytes(retained[path]) == sha
        with tarfile.open(fileobj=io.BytesIO(retained[path]), mode="r:gz") as archive:
            for member in members:
                original_papers.update(json.loads(archive.extractfile(member).read()))
    documents, tasks, decisions = {}, [], []
    for row in rights["records"]:
        if not row["eligible"]:
            continue
        identifier = row["paper_id"]
        metadata = args.raw_root / f"arxiv-{identifier}.html"
        retained[metadata] = metadata.read_bytes()
        assert hash_bytes(retained[metadata]) == row["metadata_sha256"]
        decision = {"title": row["title"], "paper_id": identifier, "source_text_sha256": row["source_text_sha256"],
            "publisher_split": row["publisher_split"], "expected_human_alternatives": row["human_alternatives"],
            "source_id": row["source_id"], "license": "CC-BY-4.0", "license_url": "https://creativecommons.org/licenses/by/4.0/",
            "authors": row["authors"], "url": "https://arxiv.org/abs/" + identifier + "v1",
            "attribution": "; ".join(row["authors"]) + ", " + row["title"] + ", arXiv:" + identifier + "v1, CC BY4.0; QASPER/S2ORC normalized extraction; quote-bearing paragraphs and figures/tables excluded.",
            "decision": "Approved delegated single-version CC BY4 exact-title/abstract criterion; private materialized closure requires primary review before training"}
        paper = original_papers[identifier]
        view = qasper.prepare(paper, decision)
        decisions.append(decision)
        split = row["publisher_split"]
        if split == "validation":
            split = "calibration" if int(hash_bytes(("airk.qasper.calibration.v1:" + identifier).encode()), 16) % 2 else "development"
        for passage in view["passages"]:
            documents[passage["id"]] = {**passage, "source_group": "qasper:" + identifier, "title": row["title"], "url": decision["url"], "license": "CC-BY-4.0", "dataset": "QASPER"}
        original_by_id = {q["question_id"]: q for q in paper["qas"]}
        for question in view["questions"]:
            original = original_by_id[question["question_id"]]
            for alternative, annotation in zip(question["human_alternatives"], original["answers"]):
                alternative["original_human_answer"] = {k: annotation["answer"][k] for k in ("unanswerable", "extractive_spans", "yes_no", "free_form_answer")}
            tasks.append({**question, "id": "qasper:" + question["question_id"], "dataset": "QASPER", "split": split,
                "publisher_split": row["publisher_split"], "source_group": "qasper:" + identifier})
    import pyarrow.parquet as parquet
    hotpot_rows = parquet.read_table(io.BytesIO(retained[args.hotpot])).to_pylist()
    selected = sorted((row["id"] for row in hotpot_rows), key=lambda identifier: hash_bytes(("airk.hotpot.final.v1:" + identifier).encode()))[:512]
    selected_set = set(selected)
    hotpot_exclusions = []
    append_hotpot_rows(hotpot_rows, selected_set, documents, tasks, hotpot_exclusions)
    for path, data in retained.items():
        if path.read_bytes() != data:
            raise ValueError("human corpus source drift: " + str(path))
    destination.mkdir(parents=True)
    result = {"schema": "airk.private-human-evidence-corpus/1", "documents": list(documents.values()), "tasks": tasks,
        "qasper_rights_decisions": decisions, "hotpot_holdout_ids_frozen": selected, "hotpot_exclusions": hotpot_exclusions,
        "hotpot_license": "CC-BY-SA-4.0; derived Hotpot fixture remains CC BY-SA4.0, private evaluation/development only, prohibited distributable-weight training absent separate rights closure",
        "hotpot_attribution": "Zhilin Yang, Peng Qi, Saizheng Zhang, Yoshua Bengio, William W. Cohen, Ruslan Salakhutdinov and Christopher D. Manning (2018), HotpotQA: A Dataset for Diverse, Explainable Multi-hop Question Answering, EMNLP; Wikipedia contributors under each retained article URL",
        "qasper_attribution": "Dasigi, Lo, Beltagy, Cohan, Smith and Gardner (2021), A Dataset of Information-Seeking Questions and Answers Anchored in Research Papers; CC BY4 annotations",
        "changes": ["Original questions/answers/alternatives preserved; deterministic publisher-dev final holdout IDs frozen", "Stable source identities/hashes and original article URLs added; quote-bearing contexts and nontext evidence excluded", "Mechanical evidence-to-source alignment only; no synthetic teacher labels or new human adjudication"],
        "limitations": ["Known public benchmarks may occur in pretrained/fine-tuned model data; no uncontaminated-pretraining claim", "Question-only memorization earns no source/citation evidence credit", "Paper-only QASPER train data does not establish code/config/test/document-format generalization", "Primary must review exact materialized closure before training"],
        "input_seals": {str(p): hash_bytes(data) for p, data in retained.items()}}
    payload = json.dumps(result, indent=2, ensure_ascii=False).encode()
    (destination / "corpus.private.json").write_bytes(payload)
    print(json.dumps({"output": str(destination), "sha256": hash_bytes(payload), "documents": len(documents), "tasks": len(tasks),
        "counts": {s: sum(t["split"] == s for t in tasks) for s in ("train", "development", "calibration", "test")},
        "test_fully_aligned": sum(t["split"] == "test" and any(a["alignment"] == "all_original_evidence_maps_to_allowed_text" for a in t["human_alternatives"]) for t in tasks)}))


if __name__ == "__main__":
    main()
