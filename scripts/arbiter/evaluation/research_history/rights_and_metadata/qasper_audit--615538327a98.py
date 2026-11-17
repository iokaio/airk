# SPDX-License-Identifier: Apache-2.0
"""Acquire pinned QASPER archives for rights/schema audit only, never execute them."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
DESTINATION = ROOT / "qasper-rights-audit-v03"
DESTINATION.mkdir(exist_ok=True)
ARCHIVES = [
    ("qasper-train-dev-v0.3.tgz", 10835856, "a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a"),
    ("qasper-test-and-evaluator-v0.3.tgz", 3865061, "72a52a41193e2838b8074f80ac074b94f956b84886c36a61c58a7df4171bdd72"),
]
records, papers, totals = [], [], {}
started = time.monotonic()
for name, size, expected in ARCHIVES:
    path = DESTINATION / name
    url = "https://qasper-dataset.s3.us-west-2.amazonaws.com/" + name
    if not path.exists():
        temporary = DESTINATION / (name + ".download")
        with urllib.request.urlopen(url, timeout=30) as source, temporary.open("xb") as output:
            count = 0
            while block := source.read(1024 * 1024):
                count += len(block)
                if count > size:
                    raise ValueError("QASPER archive exceeded pinned byte size")
                output.write(block)
        if count != size or hashlib.sha256(temporary.read_bytes()).hexdigest() != expected:
            raise ValueError("QASPER archive differs from pinned release identity")
        temporary.rename(path)
    data = path.read_bytes()
    if len(data) != size or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("retained QASPER archive changed")
    record = {"url": url, "file": name, "bytes": size, "sha256": expected, "members": []}
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        if len(members) > 32:
            raise ValueError("unexpected archive member count")
        total = 0
        for member in members:
            components = PurePosixPath(member.name)
            if components.is_absolute() or ".." in components.parts or "\\" in member.name or member.issym() or member.islnk():
                raise ValueError("unsafe archive member")
            if member.isdir():
                continue
            if not member.isfile() or member.size > 67108864:
                raise ValueError("unexpected nonregular/oversized member")
            total += member.size
            if total > 134217728:
                raise ValueError("archive expansion exceeds 128 MiB")
            row = {"name": member.name, "bytes": member.size, "consumed": "inventory only"}
            record["members"].append(row)
            if not member.name.endswith(".json"):
                # Evaluator and other supplied code are neither extracted nor executed.
                continue
            raw = archive.extractfile(member).read(member.size + 1)
            if len(raw) != member.size:
                raise ValueError("archive JSON size differs")
            row.update(sha256=hashlib.sha256(raw).hexdigest(), consumed="rights/schema/identity audit only; no model evaluation or gold adoption")
            corpus = json.loads(raw)
            split = "test" if "test" in member.name else "validation" if "dev" in member.name else "train"
            counts = {"papers": len(corpus), "questions": 0, "answers": 0, "unanswerable": 0}
            for paper_id, paper in sorted(corpus.items()):
                counts["questions"] += len(paper["qas"])
                question_ids, annotation_ids = [], []
                for question in paper["qas"]:
                    question_ids.append(question["question_id"])
                    for answer in question["answers"]:
                        annotation_ids.append(answer["annotation_id"])
                        counts["answers"] += 1
                        counts["unanswerable"] += bool(answer["answer"].get("unanswerable"))
                papers.append({"paper_id": paper_id, "publisher_split": split, "archive": name,
                               "member": member.name, "title": paper["title"],
                               "source_text_sha256": hashlib.sha256(json.dumps(paper["full_text"], sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest(),
                               "paper_schema_fields": sorted(paper), "question_ids": question_ids,
                               "human_annotation_ids": annotation_ids, "annotation_rights": "publisher CC-BY-4.0 declaration",
                               "fulltext_rights": "unresolved", "eligible": False})
            totals[split] = counts
    records.append(record)
receipt = {"schema": "airk.development-qasper-rights-audit/1", "scope": "Rights/schema/identity audit only; no training or model evaluation; original human labels not promoted as AIRK gold.",
           "metadata_revision": "fdc9d8214fbab5dd782958601db4d678e6934a54", "archives": records,
           "totals": totals, "elapsed_seconds": time.monotonic() - started}
receipt_path = DESTINATION / f"audit-receipt-{time.time_ns()}.json"
receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
manifest = {"schema": "airk.development-qasper-paper-rights/1", "status": "not_adopted", "papers": papers}
manifest_path = DESTINATION / f"paper-rights-{time.time_ns()}.json"
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps({"receipt": str(receipt_path), "manifest": str(manifest_path), "totals": totals, "eligible": 0, "elapsed_seconds": receipt["elapsed_seconds"]}, indent=2))
