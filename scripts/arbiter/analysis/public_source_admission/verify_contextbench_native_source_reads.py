# SPDX-License-Identifier: Apache-2.0
"""Run the owned native reader on one isolated, frozen ContextBench revision."""

from pathlib import Path
import runpy
import sys


SHARED = Path(__file__).with_name("native_cohort_source_reads.py")
reader = runpy.run_path(str(SHARED))
CAMPAIGN = reader["CAMPAIGN"]
MANIFEST = "46dede63b8ceb84464471a8d66846989184bf3a2b8525a5bef76278ed1a5a9b8"
DECLARED = "a80f26b9026bf0ee3fdf3dc4f9c10523cb6bfa61b734dcd3d61f9b154636c66e"
INPUTS = "contextbench-six-native-inputs-r1"
DECLARED_PATH = (CAMPAIGN / "repository-confirmation-multilang-r1"
                 / "contextbench-14-declared-lines-r1.private.json")
BASE_COUNTS = {
    "7b36544eca5e72aba689760118b98419ef4dd179": (6, 6, 2),
    "cbd11d8a45dc80392c5f1be9679051085e6a3376": (4, 4, 4),
    "2de810e97d5ce832ff98023a4a9cf215a86244ea": (3, 3, 1),
    "93cdb2628e89dd5831eee22b8df697aea00eca3b": (4, 4, 1),
    "daea6623fd691d52e6ff1ce440f6a7d29b9d3adb": (20, 4, 3),
    "f7997256fc23e202e08df39997afd360d202f9f1": (3, 3, 2),
}


def raw_lines(body, first, last):
    if not 1 <= first <= last:
        raise ValueError("invalid line range")
    lines = body.split(b"\n")
    if last > len(lines) - int(body.endswith(b"\n")):
        raise ValueError("span beyond file end")
    start = sum(len(line) + 1 for line in lines[:first - 1])
    raw = b"\n".join(lines[first - 1:last])
    return {"line_start": first, "line_end_inclusive": last, "byte_start": start,
            "byte_end_exclusive": start + len(raw), "span_sha256": reader["digest"](raw)}


def revision_cases(base, manifest, declared, root):
    from collections import Counter

    total, visible, documents = BASE_COUNTS[base]
    matches = [(key, case) for key, case in manifest["cases"].items() if case["base"] == base]
    if len(matches) != 1:
        raise ValueError("staging base is not unique")
    key, case = matches[0]
    files = {f["path"]: f for f in manifest["staged"] if f["area"] == "sources" and f["base"] == base}
    omissions = {r["path"] for r in manifest["decisions"] if r["base"] == base and not r["staged"]}
    source_rows = [s for s in declared["rows"] if s["instance_id"] == key]
    if (set(files) | omissions != set(case["source_paths"]) or set(files) & omissions or
            len(files) != documents or len(source_rows) != total or
            Counter(s["ordinal"] for s in source_rows) != Counter(range(total))):
        raise ValueError("source population differs")
    bodies = {}
    for path, item in files.items():
        if item["repo"] != case["repo"] or item["relative_path"] != f"sources/{base}/{path}":
            raise ValueError("staging descriptor differs")
        body = (root / path).read_bytes()
        if len(body) != item["bytes"] or reader["digest"](body) != item["sha256"]:
            raise ValueError("source bytes differ")
        bodies[path] = body
    retained, withheld = [], []
    for span in source_rows:
        path = span["path"]
        if ((span["repo"], span["base"]) != (case["repo"], base)
                or span["status"] not in ("exact_declared_lines", "exact_numbered_declared_lines")):
            raise ValueError("unverified declaration")
        selector = {"instance_id": key, "ordinal": span["ordinal"], "alignment": span["status"]}
        if path in omissions:
            withheld.append({**selector, "path": path, "start": span["start"], "end": span["end"]})
        else:
            retained.append({"selector": selector, "document": files[path],
                             "source_span": raw_lines(bodies[path], span["start"], span["end"])})
    if len(retained) != visible or len(withheld) != total - visible:
        raise ValueError("read and withheld counts differ")
    return retained, withheld, files


def main():
    import json
    import argparse

    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--output", type=Path, required=True)
    args, _ = parser.parse_known_args()
    base, = [sha for sha in BASE_COUNTS if args.output.name.startswith(f"cb-{sha[:8]}-native-source-reads-")]
    total, visible, document_count = BASE_COUNTS[base]
    manifest_path = CAMPAIGN / INPUTS / "manifest.private.json"
    reader["checked"](manifest_path, MANIFEST)
    reader["checked"](DECLARED_PATH, DECLARED)
    manifest = json.loads(manifest_path.read_bytes())
    declared = json.loads(DECLARED_PATH.read_bytes())
    cases, withheld, docs = revision_cases(base, manifest, declared, CAMPAIGN / INPUTS / base)
    def prepare(authenticated_manifest):
        if authenticated_manifest != manifest:
            raise ValueError("staging manifest changed")
        return base, cases, docs
    cohort = reader["Cohort"](name=f"cb-{base[:8]}", corpus_directory=INPUTS, manifest_sha256=MANIFEST,
                             case_count=visible, document_count=document_count, selector_field="ordinal",
                             receipt_identity={"base": base, "repo": next(iter(docs.values()))["repo"],
                                               "declared_receipt_sha256": DECLARED, "declared_spans": total,
                                               "withheld": withheld})
    reader["run_cohort"](cohort, Path(__file__).resolve(), prepare_inputs=prepare)


if __name__ == "__main__":
    main()
