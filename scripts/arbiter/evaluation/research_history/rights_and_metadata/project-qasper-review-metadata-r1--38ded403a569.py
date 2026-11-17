# SPDX-License-Identifier: Apache-2.0
"""Private metadata-only projection; never access question/answer/evidence fields."""
import hashlib
import io
import json
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parent
source = ROOT / "qasper-rights-audit-v03/qasper-test-and-evaluator-v0.3.tgz"
expected = "72a52a41193e2838b8074f80ac074b94f956b84886c36a61c58a7df4171bdd72"
raw = source.read_bytes()
assert hashlib.sha256(raw).hexdigest() == expected
proposal = ROOT / "qasper-version-set-and-private-final-proposal-r1.json"
ids = set(json.loads(proposal.read_bytes())["smallest_fixed_publisher_TEST_pool"]["paper_ids"])
ids.add("1707.07212")
rows = []
with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as archive:
    for member in archive:
        if member.name.endswith(".json"):
            member_bytes = archive.extractfile(member).read()
            corpus = json.loads(member_bytes)
            for paper_id in sorted(ids):
                if paper_id in corpus:
                    paper = corpus[paper_id]
                    rows.append({"paper_id": paper_id, "publisher_split": "test", "archive_sha256": expected,
                                 "member": member.name, "member_sha256": hashlib.sha256(member_bytes).hexdigest(),
                                 "title": paper["title"], "abstract": paper["abstract"]})
assert len(rows) == len(ids)
output = ROOT / "qasper-review-title-abstract-projection-r1.json"
assert not output.exists()
output.write_text(json.dumps({"schema": "airk.private-qasper-title-abstract-projection/1", "scope": "Only title and abstract values accessed after JSON decode. No qas, answers, evidence, full_text or alignment inspected.", "rows": rows}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"path": str(output), "sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "rows": len(rows), "proposal_sha256": hashlib.sha256(proposal.read_bytes()).hexdigest()}))
