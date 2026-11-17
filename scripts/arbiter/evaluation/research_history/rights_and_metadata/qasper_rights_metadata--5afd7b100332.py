# SPDX-License-Identifier: Apache-2.0
"""Inspect a bounded arXiv rights sample; keep unresolved fulltext excluded."""
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import time
import urllib.request

ROOT = Path(__file__).resolve().parent / "qasper-rights-audit-v03"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--manifest", type=Path, required=True)
parser.add_argument("--limit", type=int, default=12)
parser.add_argument("--offset", type=int, default=0)
args = parser.parse_args()
if not 1 <= args.limit <= 24:
    parser.error("rights sample must be between 1 and 24 papers")
if not 0 <= args.offset <= 1000:
    parser.error("invalid sample offset")
corpus = json.loads(args.manifest.read_text())
sample = sorted((paper for paper in corpus["papers"] if paper["publisher_split"] != "test"),
                key=lambda paper: hashlib.sha256(("airk.qasper.rights.v1:" + paper["paper_id"]).encode()).hexdigest())[args.offset:args.offset + args.limit]
records = []
for paper in sample:
    identifier = paper["paper_id"]
    if not re.fullmatch(r"[0-9]{4}\.[0-9]{4,5}", identifier):
        raise ValueError("unexpected paper ID")
    url = "https://arxiv.org/abs/" + identifier
    result = {"paper_id": identifier, "publisher_split": paper["publisher_split"], "url": url,
              "eligible": False, "scope": "Metadata rights observation only; exact extracted-fulltext source provenance remains separately required."}
    try:
        metadata_path = ROOT / ("arxiv-" + identifier + ".html")
        if metadata_path.exists():
            data = metadata_path.read_bytes()
        else:
            with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "AIRK-QASPER-rights-research (metadata only)"}), timeout=30) as response:
                data = response.read(262145)
        if len(data) > 262144:
            raise ValueError("paper metadata exceeds 256 KiB")
        if not metadata_path.exists():
            metadata_path.write_bytes(data)
        text = html.unescape(data.decode("utf-8"))
        licenses = sorted(set(re.findall(r'https?://(?:creativecommons\.org/licenses/[^\s"<>]+|arxiv\.org/licenses/[^\s"<>]+)', text)))
        result.update(metadata_file=metadata_path.name, metadata_sha256=hashlib.sha256(data).hexdigest(), license_urls=licenses,
                      compatible_license_observed=any("creativecommons.org/licenses/by/4.0" in license for license in licenses))
    except Exception as error:
        result["error"] = str(error)
    records.append(result)
output = ROOT / f"rights-sample-{time.time_ns()}.json"
output.write_text(json.dumps({"input_manifest_sha256": hashlib.sha256(args.manifest.read_bytes()).hexdigest(), "sample_policy": "SHA256 domain-separated paper-ID order, non-test only", "offset": args.offset, "records": records}, indent=2) + "\n")
print(json.dumps({"output": str(output), "sample": len(records), "compatible_license_observed": sum(bool(row.get("compatible_license_observed")) for row in records), "errors": [{"paper_id": row["paper_id"], "error": row["error"]} for row in records if "error" in row], "eligible": 0}, indent=2))
